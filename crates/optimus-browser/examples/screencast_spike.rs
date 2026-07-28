//! ADR-0051 gating spike: is preview-as-screencast-pixels viable?
//!
//! Measures, against the same out-of-process Chromium the agent effector
//! uses, the three numbers that decide whether the desktop preview can leave
//! the shell (ADR-0015 §2 restored, ADR-0051 step 2):
//!
//! 1. **Cadence** — inter-arrival time of screencast frames while the page
//!    animates. The preview cannot look smoother than this.
//! 2. **Staleness** — capture-to-delivery delay per frame (Chromium's own
//!    capture timestamp against wall-clock arrival here). How old a pixel is
//!    by the time a shell could paint it.
//! 3. **Click-to-pixel** — CDP input dispatch until the first frame carrying
//!    the resulting damage arrives. What a user's click would feel like.
//!
//! Run: `cargo run -p optimus-browser --example screencast_spike --release`
//!
//! This drives `headless_chrome` directly rather than `BrowserSession`
//! because it models the UserPreview trust domain (ADR-0040), whose entire
//! purpose is localhost and dev pages — the agent effector's SSRF guard
//! (deliberately, see `validate_network_url`) forbids exactly those. Nothing
//! here weakens that guard; this is a separate session in the other domain.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use headless_chrome::protocol::cdp::types::Event;
use headless_chrome::protocol::cdp::{Input, Page};
use headless_chrome::{Browser, LaunchOptionsBuilder};

/// One screencast frame as observed from the receiving side.
struct FrameObservation {
    arrived: Instant,
    /// Capture-to-delivery delay, from Chromium's own metadata timestamp.
    staleness: Duration,
    session_id: u32,
}

/// The page under test. Solid backgrounds so every click repaints the whole
/// surface (worst-case damage), plus a small block that moves every rAF so
/// the cadence phase has continuous animation to chase.
const PAGE: &str = r#"<body style="margin:0;background:#c00">
<div id="b" style="width:120px;height:120px;background:#fff;position:absolute"></div>
<script>
let n = 0, on = false;
function tick() {
  if (!on) return;
  n++;
  b.style.transform = 'translate(' + (n % 600) + 'px,' + (n % 400) + 'px)';
  requestAnimationFrame(tick);
}
window.startAnim = () => { on = true; tick(); };
window.stopAnim = () => { on = false; };
let c = 0;
addEventListener('mousedown', () => {
  c++;
  document.body.style.background = c % 2 ? '#0c0' : '#00c';
});
</script></body>"#;

const CLICK_ROUNDS: usize = 20;
/// No frame for this long = the page has gone quiet and the next frame that
/// arrives is caused by whatever we do next.
const QUIET: Duration = Duration::from_millis(500);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::new(
        LaunchOptionsBuilder::default()
            .headless(true)
            .window_size(Some((1280, 800)))
            .build()?,
    )?;
    let tab = browser.new_tab()?;

    let url = format!("data:text/html,{}", urlencode(PAGE));
    tab.navigate_to(&url)?;
    tab.wait_until_navigated()?;

    // Frames land on the transport thread; timing and acking happen here on
    // the main thread so a slow consumer shows up in the numbers instead of
    // hiding in a background loop.
    let (tx, rx) = mpsc::channel::<FrameObservation>();
    tab.add_event_listener(Arc::new(move |event: &Event| {
        if let Event::PageScreencastFrame(frame) = event {
            let now_epoch = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let captured = frame.params.metadata.timestamp.unwrap_or(now_epoch);
            let _ = tx.send(FrameObservation {
                arrived: Instant::now(),
                staleness: Duration::from_secs_f64((now_epoch - captured).max(0.0)),
                session_id: frame.params.session_id,
            });
        }
    }))?;

    tab.start_screencast(
        Some(Page::StartScreencastFormatOption::Jpeg),
        Some(80),
        Some(1280),
        Some(800),
        Some(1),
    )?;

    // --- Phase 1+2: cadence and staleness under continuous animation -------
    tab.evaluate("startAnim()", false)?;
    let mut cadence_ms = Vec::new();
    let mut staleness_ms = Vec::new();
    let phase_end = Instant::now() + Duration::from_secs(3);
    let mut previous: Option<Instant> = None;
    while Instant::now() < phase_end {
        let Ok(frame) = rx.recv_timeout(Duration::from_millis(250)) else {
            continue;
        };
        tab.ack_screencast(frame.session_id)?;
        if let Some(prev) = previous {
            cadence_ms.push(ms(frame.arrived - prev));
        }
        staleness_ms.push(ms(frame.staleness));
        previous = Some(frame.arrived);
    }
    tab.evaluate("stopAnim()", false)?;

    // --- Phase 3: click-to-pixel ------------------------------------------
    let mut click_ms = Vec::new();
    for _ in 0..CLICK_ROUNDS {
        // Drain until quiet so the next frame is provably ours.
        while let Ok(frame) = rx.recv_timeout(QUIET) {
            tab.ack_screencast(frame.session_id)?;
        }
        let dispatched = Instant::now();
        click(&tab, 640.0, 400.0)?;
        let frame = rx.recv_timeout(Duration::from_secs(2))?;
        tab.ack_screencast(frame.session_id)?;
        click_ms.push(ms(frame.arrived - dispatched));
    }
    tab.stop_screencast()?;

    report("frame cadence (animating)", &mut cadence_ms);
    report("capture->delivery staleness", &mut staleness_ms);
    report("click->pixel round trip", &mut click_ms);

    // The bar the ADR set: cadence p50 <= 50ms (>= 20fps) and click->pixel
    // p95 <= 100ms. DevTools device mode is the reference feel.
    let cadence_p50 = percentile(&mut cadence_ms, 50.0);
    let click_p95 = percentile(&mut click_ms, 95.0);
    let verdict = cadence_p50 <= 50.0 && click_p95 <= 100.0;
    println!(
        "\nverdict: {} (cadence p50 {:.1}ms <= 50ms, click p95 {:.1}ms <= 100ms)",
        if verdict { "PASS" } else { "FAIL" },
        cadence_p50,
        click_p95,
    );
    Ok(())
}

fn click(tab: &headless_chrome::Tab, x: f64, y: f64) -> Result<(), Box<dyn std::error::Error>> {
    for kind in [
        Input::DispatchMouseEventTypeOption::MousePressed,
        Input::DispatchMouseEventTypeOption::MouseReleased,
    ] {
        tab.call_method(Input::DispatchMouseEvent {
            Type: kind,
            x,
            y,
            modifiers: None,
            timestamp: None,
            button: Some(Input::MouseButton::Left),
            buttons: None,
            click_count: Some(1),
            force: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
            delta_x: None,
            delta_y: None,
            pointer_Type: None,
        })?;
    }
    Ok(())
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn percentile(samples: &mut [f64], p: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let rank = (p / 100.0 * (samples.len() - 1) as f64).round() as usize;
    samples[rank.min(samples.len() - 1)]
}

fn report(label: &str, samples: &mut [f64]) {
    println!(
        "{label:30} n={:3}  p50 {:7.1}ms  p95 {:7.1}ms  max {:7.1}ms",
        samples.len(),
        percentile(samples, 50.0),
        percentile(samples, 95.0),
        percentile(samples, 100.0),
    );
}

/// Minimal percent-encoding for a data: URL — only the characters that break
/// one ('#' ends the URL, '%' starts an escape).
fn urlencode(html: &str) -> String {
    html.replace('%', "%25").replace('#', "%23")
}
