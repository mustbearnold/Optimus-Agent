//! One scheduling contract for terminal motion and repaint invalidation.
//!
//! Domain events decide what the workbench says. This module only decides when
//! the surface needs another chance to paint it. All timestamps are supplied by
//! the caller rather than read internally, which keeps the clock deterministic
//! under unit tests and leaves the event loop free to use its real clock.

use std::time::{Duration, Instant};

/// The wake used to drain a live worker when no visible animation is enabled.
/// It is a transport pump, not a repaint cadence: a frame is still drawn only
/// when [`FrameInvalidation`] is marked.
const ACTIVE_PUMP: Duration = Duration::from_millis(40);

/// The adaptive terminal rate is deliberately conservative. A spinner does
/// not need a 60 Hz frame ceiling, and this preserves the pre-workbench face's
/// roughly 12.5 visible spinner steps per second.
const ADAPTIVE_FRAME: Duration = Duration::from_millis(40);
const FPS30_FRAME: Duration = Duration::from_millis(33);
const FPS60_FRAME: Duration = Duration::from_millis(16);

/// How often the single terminal animation clock is allowed to tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AnimationMode {
    /// Do not animate. A live worker still gets a bounded event-drain wake.
    Off,
    /// Cap animation work at approximately 30 ticks per second.
    Fps30,
    /// Cap animation work at approximately 60 ticks per second.
    Fps60,
    /// Use the terminal-safe default cadence.
    #[default]
    Adaptive,
}

impl AnimationMode {
    /// Parse the stable environment spelling used by the TUI.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(Self::Off),
            "30" | "fps30" | "30hz" => Some(Self::Fps30),
            "60" | "fps60" | "60hz" => Some(Self::Fps60),
            "adaptive" | "auto" => Some(Self::Adaptive),
            _ => None,
        }
    }

    const fn frame_period(self) -> Option<Duration> {
        match self {
            Self::Off => None,
            Self::Fps30 => Some(FPS30_FRAME),
            Self::Fps60 => Some(FPS60_FRAME),
            Self::Adaptive => Some(ADAPTIVE_FRAME),
        }
    }

    /// Number of animation ticks between visible spinner glyphs.
    ///
    /// The glyph family stays around 15 steps per second even when the frame
    /// ceiling is raised explicitly. Static modes never tick, so the value is
    /// only a harmless default for callers that inspect the configuration.
    pub const fn spinner_ticks(self) -> usize {
        match self {
            Self::Fps60 => 4,
            Self::Off | Self::Fps30 | Self::Adaptive => 2,
        }
    }
}

/// A single clock for all visible terminal motion.
#[derive(Debug, Clone)]
pub struct AnimationClock {
    mode: AnimationMode,
    reduced_motion: bool,
    active: bool,
    next_frame: Option<Instant>,
}

impl Default for AnimationClock {
    fn default() -> Self {
        Self::new(AnimationMode::default(), false)
    }
}

impl AnimationClock {
    pub const fn new(mode: AnimationMode, reduced_motion: bool) -> Self {
        Self {
            mode,
            reduced_motion,
            active: false,
            next_frame: None,
        }
    }

    /// Read the opt-in terminal settings once, at startup.
    ///
    /// `OPTIMUS_TUI_ANIMATION` accepts `off`, `30`, `60`, or `adaptive` (with
    /// `fps30`/`fps60` aliases). `OPTIMUS_TUI_REDUCED_MOTION` accepts the usual
    /// true spellings. Invalid values fall back to the safe adaptive default.
    pub fn from_environment() -> Self {
        let mode = std::env::var("OPTIMUS_TUI_ANIMATION")
            .ok()
            .and_then(|value| AnimationMode::parse(&value))
            .unwrap_or_default();
        let reduced_motion = std::env::var("OPTIMUS_TUI_REDUCED_MOTION")
            .ok()
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            });
        Self::new(mode, reduced_motion)
    }

    pub const fn mode(&self) -> AnimationMode {
        self.mode
    }

    pub const fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub const fn animates(&self) -> bool {
        !self.reduced_motion && !matches!(self.mode, AnimationMode::Off)
    }

    pub const fn spinner_ticks(&self) -> usize {
        self.mode.spinner_ticks()
    }

    /// Start or stop the clock for the currently visible running work.
    pub fn set_active(&mut self, active: bool, now: Instant) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.next_frame = active.then(|| now + self.frame_period().unwrap_or(ACTIVE_PUMP));
    }

    /// The next time the event loop should wake, or `None` when it can block on
    /// input indefinitely. Static live work gets a pump wake; idle does not.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if !self.active {
            return None;
        }
        if self.animates() {
            self.next_frame.or(Some(now))
        } else {
            Some(now + ACTIVE_PUMP)
        }
    }

    /// Advance the animation deadline if a frame is due.
    ///
    /// Returning a bool lets the render loop mark exactly one repaint for a
    /// missed or coalesced deadline. Advancing from the old deadline, rather
    /// than from `now`, prevents a slow draw from permanently drifting the
    /// clock; the loop skips already-missed ticks instead of replaying them.
    pub fn tick_if_due(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.next_frame else {
            return false;
        };
        if !self.animates() || now < deadline {
            return false;
        }
        let period = self
            .mode
            .frame_period()
            .expect("animated clocks always have a frame period");
        let mut next = deadline;
        while next <= now {
            next += period;
        }
        self.next_frame = Some(next);
        true
    }

    fn frame_period(&self) -> Option<Duration> {
        self.animates().then(|| self.mode.frame_period()).flatten()
    }
}

/// A coalescing repaint invalidation flag.
///
/// The initial frame is dirty so the terminal gets one first paint. Every
/// domain update, input event, resize, or animation tick calls [`mark`], while
/// [`take_for_draw`] clears the flag exactly once. This is deliberately tiny so
/// the dirty-frame rule can be tested without opening a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInvalidation {
    dirty: bool,
}

impl FrameInvalidation {
    pub const fn initial() -> Self {
        Self { dirty: true }
    }

    pub const fn is_dirty(self) -> bool {
        self.dirty
    }

    pub fn mark(&mut self) {
        self.dirty = true;
    }

    pub fn take_for_draw(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn idle_has_no_wake_until_work_becomes_visible() {
        let base = Instant::now();
        let mut clock = AnimationClock::default();
        assert_eq!(clock.next_wake(base), None);

        clock.set_active(true, base);
        assert_eq!(clock.next_wake(base), Some(at(base, 40)));
        clock.set_active(false, at(base, 1));
        assert_eq!(clock.next_wake(at(base, 1)), None);
    }

    #[test]
    fn animation_ticks_at_the_configured_deadline_and_skips_missed_frames() {
        let base = Instant::now();
        let mut clock = AnimationClock::new(AnimationMode::Fps30, false);
        clock.set_active(true, base);
        assert!(!clock.tick_if_due(at(base, 32)));
        assert!(clock.tick_if_due(at(base, 33)));
        assert_eq!(clock.next_wake(at(base, 33)), Some(at(base, 66)));
        assert!(clock.tick_if_due(at(base, 200)));
        assert!(clock
            .next_wake(at(base, 200))
            .is_some_and(|next| next > at(base, 200)));
    }

    #[test]
    fn off_and_reduced_motion_keep_a_live_worker_drained_without_animation() {
        let base = Instant::now();
        for mut clock in [
            AnimationClock::new(AnimationMode::Off, false),
            AnimationClock::new(AnimationMode::Fps60, true),
        ] {
            clock.set_active(true, base);
            assert!(!clock.animates());
            assert_eq!(clock.next_wake(base), Some(at(base, 40)));
            assert!(!clock.tick_if_due(at(base, 10_000)));
        }
    }

    #[test]
    fn mode_parser_accepts_documented_spellings_and_rejects_unknown_values() {
        assert_eq!(AnimationMode::parse("off"), Some(AnimationMode::Off));
        assert_eq!(AnimationMode::parse("FPS30"), Some(AnimationMode::Fps30));
        assert_eq!(AnimationMode::parse("60hz"), Some(AnimationMode::Fps60));
        assert_eq!(AnimationMode::parse("auto"), Some(AnimationMode::Adaptive));
        assert_eq!(AnimationMode::parse("cinematic"), None);
    }

    #[test]
    fn invalidation_paints_once_and_coalesces_marks() {
        let mut frame = FrameInvalidation::initial();
        assert!(frame.is_dirty());
        assert!(frame.take_for_draw());
        assert!(!frame.take_for_draw());
        frame.mark();
        frame.mark();
        assert!(frame.take_for_draw());
        assert!(!frame.is_dirty());
    }
}
