//! Terminal face of the Optimus agent host (ADR-0045).
//!
//! The TUI is not a remote client of the host — it *is* the host process, with a
//! terminal drawn on top. `optimus-host` is linked in and `handle_ipc` is called
//! directly, so there is no transport hop for the surface that owns the session.
//! Other surfaces attach to this process over stdio or loopback HTTP instead.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind, MouseEvent,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub mod animation;

mod activity;
mod commands;
mod completion;
mod composer;
mod history;
mod keys;
mod logging;
mod mouse;
mod overlay;
mod picker;
mod preferences;
mod session;
mod sidebar;
mod tool_line;
mod transcript;
mod view;
mod width;
pub mod workbench;

pub use session::{Message, Role, TuiSession};

/// Run the terminal face against an Optimus home directory.
pub fn run(home: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let log = logging::log_path(&home);
    // Taken before the screen is, and dropped after it is handed back, so no
    // in-process library write can land in the middle of a frame.
    let _stderr = logging::StderrLog::to_file(&log)?;
    // Before `enter`, not after: a panic between `enable_raw_mode` succeeding
    // and `Terminal::new` returning leaves a raw terminal with no value to hand
    // back, and that window is covered too.
    install_panic_restore(log);
    let mut terminal = enter()?;
    let result = event_loop(&mut terminal, TuiSession::new(home));
    leave(terminal)?;
    result
}

/// Undo everything [`enter`] did, without a `Terminal` and without failing.
///
/// The panic path cannot borrow the terminal — it was moved into the event loop
/// — so this drives stdout directly. Every step drops its error deliberately:
/// panicking while restoring replaces the original panic reason with this one,
/// and the original is the only one that explains anything.
fn restore_screen() {
    let mut stdout = io::stdout();
    let _ = disable_raw_mode();
    let _ = stdout.execute(DisableBracketedPaste);
    let _ = stdout.execute(DisableMouseCapture);
    let _ = stdout.execute(LeaveAlternateScreen);
    let _ = stdout.execute(Show);
}

/// Hand the terminal back on the way out of a panic, the way [`leave`] does on
/// the way out of the event loop (#92).
///
/// `leave` only runs when the loop *returns*; an unwind goes straight past it.
/// What that leaves behind is not cosmetic. Mouse capture turns every later
/// click in the user's shell into an escape sequence typed at the prompt, and
/// raw mode means the Ctrl-C that would normally clear the line no longer
/// reaches it — the session is over but the terminal is still not theirs.
///
/// `ratatui::init` installs a hook of its own and this face deliberately does
/// not use it: as of 0.29 it restores raw mode and the alternate screen only,
/// and knows nothing about the mouse capture and bracketed paste that `enter`
/// also turns on. A partial restore is the worse failure of the two, because
/// the screen looks handed back while the input side is still hijacked.
///
/// Only the thread that took the screen gives it back. `TuiSession` spawns
/// workers for turns and approvals; one of those panicking kills its own thread
/// and leaves the face running, so tearing the screen down from there would
/// break a session that is still very much alive.
fn install_panic_restore(log: PathBuf) {
    let owner = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == owner {
            restore_screen();
            // stderr points at the log for the whole run, and a hook runs
            // *before* the guard holding that redirect is dropped — so the
            // payload `previous` is about to write lands in the log, not on the
            // screen. Stdout is never redirected, so one line there is what
            // stops the crash from looking like a silent exit.
            let _ = writeln!(
                io::stdout(),
                "\noptimus-tui stopped unexpectedly. Details: {}",
                log.display()
            );
        }
        previous(info);
    }));
}

fn enter() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    // Capture costs the terminal's own text selection, which is why `/mouse`
    // exists to hand it back.
    stdout.execute(EnableMouseCapture)?;
    // Without this a pasted prompt arrives as key presses, and its first
    // newline submits half of it as a live turn — real tokens spent on a
    // fragment. With it, a paste is one `Event::Paste` the composer inserts
    // as text.
    stdout.execute(EnableBracketedPaste)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn leave(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(DisableBracketedPaste)?;
    terminal.backend_mut().execute(DisableMouseCapture)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut session: TuiSession,
) -> Result<(), Box<dyn std::error::Error>> {
    // The clock owns motion; this loop owns no per-widget timers. A running
    // turn gets a bounded wake for event draining, while an idle terminal can
    // block on input without repainting or waking every 40ms.
    let mut animation = animation::AnimationClock::from_environment();
    session.set_spinner_ticks(animation.spinner_ticks());
    let mut repaint = animation::FrameInvalidation::initial();
    let mut captured = true;
    // Proving the panic hook works needs a panic that happens while the screen
    // is taken, and the only honest way to get one is to ask. Read once, and
    // only in debug builds, so no released binary can be made to panic by its
    // environment. `tests/pty.rs` is the sole caller.
    #[cfg(debug_assertions)]
    let panic_key = std::env::var("OPTIMUS_TUI_PANIC_ON_KEY")
        .ok()
        .and_then(|value| value.chars().next());
    loop {
        let now = Instant::now();
        let domain_changed = session.pump();
        if domain_changed {
            repaint.mark();
            // Arriving output lengthens the transcript underneath a selection
            // the human is reading, so the anchor is recomputed with it rather
            // than only when a key moves.
            if session.busy() {
                anchor(terminal, &mut session)?;
            }
        }
        animation.set_active(session.busy(), now);
        if animation.tick_if_due(now) {
            session.tick();
            repaint.mark();
        }
        if session.mouse != captured {
            captured = session.mouse;
            if captured {
                terminal.backend_mut().execute(EnableMouseCapture)?;
            } else {
                terminal.backend_mut().execute(DisableMouseCapture)?;
            }
        }
        if repaint.take_for_draw() {
            terminal.draw(|f| view::draw(f, &session))?;
        }
        let ready = match animation.next_wake(Instant::now()) {
            Some(wake) => event::poll(wake.saturating_duration_since(Instant::now()))?,
            // No animation and no running worker: the terminal has no reason to
            // wake itself. `read` blocks until a human event arrives.
            None => true,
        };
        if !ready {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                #[cfg(debug_assertions)]
                if panic_key
                    .is_some_and(|wanted| key.code == crossterm::event::KeyCode::Char(wanted))
                {
                    panic!("OPTIMUS_TUI_PANIC_ON_KEY");
                }
                let mode = keys::Mode {
                    picker: session.picker.is_some(),
                    busy: session.busy(),
                    drafting: !session.composer.is_empty(),
                    suggesting: !completion::suggestions(session.composer.text()).is_empty(),
                    inspecting: session.workbench.inspecting(),
                };
                if on_key(terminal, &mut session, keys::intent(&key, mode))? {
                    return Ok(());
                }
                repaint.mark();
            }
            Event::Mouse(mouse) => {
                on_mouse(terminal, &mut session, &mouse)?;
                repaint.mark();
            }
            // A paste is text, never a submit — the whole point of turning
            // bracketed paste on.
            Event::Paste(text) => {
                session.history.release();
                session.composer.insert_str(&text);
                repaint.mark();
            }
            Event::Resize(_, _) => repaint.mark(),
            _ => {}
        }
    }
}

/// Carry out one key intent. Returns true when the application should leave.
///
/// Every decision about *what* a key means lives in [`keys::intent`]; this is
/// only the arm that moves state, so the whole key table stays assertable
/// without a terminal.
fn on_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut TuiSession,
    intent: keys::Intent,
) -> io::Result<bool> {
    use keys::{
        BlockStep, Edit, FocusStep, HistoryStep, Intent, Motion, PickerStep, ScrollStep,
        SuggestStep,
    };
    use workbench::SelectionStep;
    // Any edit to the draft ends history browsing, so a recalled prompt the
    // human changed stays changed — and it changes which commands still match,
    // so a highlight three rows down the old list starts over rather than
    // pointing at whatever has since moved under it.
    if matches!(
        intent,
        Intent::Insert(_) | Intent::Edit(_) | Intent::Newline | Intent::ClearDraft
    ) {
        session.history.release();
        session.completion.reset();
    }
    match intent {
        Intent::Quit => return Ok(true),
        Intent::Cancel => session.cancel(),
        // `/quit` answers inside submit, so the flag is read straight after.
        Intent::Submit => {
            session.submit();
            if session.quit {
                return Ok(true);
            }
        }
        Intent::Newline => session.composer.newline(),
        Intent::Insert(c) => session.composer.insert_char(c),
        Intent::ClearDraft => {
            session.composer.take();
        }
        Intent::Edit(edit) => match edit {
            Edit::Backspace => session.composer.backspace(),
            Edit::Delete => session.composer.delete(),
            Edit::KillToEnd => session.composer.kill_to_end(),
            Edit::KillToStart => session.composer.kill_to_start(),
            Edit::KillWord => session.composer.kill_word(),
        },
        Intent::Move(motion) => match motion {
            Motion::Left => session.composer.left(),
            Motion::Right => session.composer.right(),
            Motion::WordLeft => session.composer.word_left(),
            Motion::WordRight => session.composer.word_right(),
            Motion::Home => session.composer.home(),
            Motion::End => session.composer.end(),
        },
        Intent::History(step) => {
            let recalled = match step {
                HistoryStep::Older => session.history.older(session.composer.text()),
                HistoryStep::Newer => session.history.newer(),
            };
            if let Some(text) = recalled {
                session.composer.set(text);
            }
        }
        Intent::Scroll(step) => match step {
            ScrollStep::PageUp => scroll_page(terminal, session, 1)?,
            ScrollStep::PageDown => scroll_page(terminal, session, -1)?,
            ScrollStep::Tail => session.scroll_back = 0,
        },
        Intent::Picker(step) => match step {
            PickerStep::Next => {
                if let Some(picker) = session.picker.as_mut() {
                    picker.down();
                }
            }
            PickerStep::Previous => {
                if let Some(picker) = session.picker.as_mut() {
                    picker.up();
                }
            }
            PickerStep::Confirm => session.confirm_picker(),
            PickerStep::Close => session.picker = None,
        },
        Intent::Suggest(step) => {
            let count = completion::suggestions(session.composer.text()).len();
            match step {
                SuggestStep::Next => session.completion.down(count),
                SuggestStep::Previous => session.completion.up(count),
            }
        }
        // Reset after taking the row, never before: resetting first would
        // complete the top of the list instead of the row that is highlighted.
        Intent::Complete => {
            if let Some(text) = session.completion.completed(session.composer.text()) {
                session.history.release();
                session.composer.set(text);
                session.completion.reset();
            }
        }
        Intent::Focus(step) => {
            match step {
                FocusStep::Inspect => session.workbench.inspect(),
                FocusStep::Composer => session.workbench.leave_inspect(),
            }
            anchor(terminal, session)?;
        }
        Intent::Block(step) => {
            match step {
                BlockStep::Next => session.workbench.step(SelectionStep::Next),
                BlockStep::Previous => session.workbench.step(SelectionStep::Previous),
                BlockStep::First => session.workbench.step(SelectionStep::First),
                BlockStep::Last => session.workbench.step(SelectionStep::Last),
                // Nothing to report when the selected block has no body: the
                // absence of movement is the answer.
                BlockStep::Fold => {
                    session.workbench.toggle_fold();
                }
            }
            anchor(terminal, session)?;
        }
        Intent::Redraw => terminal.clear()?,
        Intent::ToggleSidebar => session.sidebar.toggle(),
        Intent::Ignore => {}
    }
    Ok(false)
}

/// Keep the selected block on screen while the transcript has the keyboard.
///
/// Does nothing otherwise, so a session nobody is inspecting scrolls exactly
/// as it always did — following the tail unless the human scrolled back.
fn anchor(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut TuiSession,
) -> io::Result<()> {
    if !session.workbench.inspecting() {
        return Ok(());
    }
    let size = terminal.size()?;
    let chrome = view::composer_height(session, size.width) + 3;
    let height = usize::from(size.height.saturating_sub(chrome));
    let rows = view::visible_rows(session, view::transcript_width_for(session, size.width));
    session.scroll_back = view::anchored(&rows, height, session.scroll_back);
    Ok(())
}

/// Apply one mouse event.
///
/// Every decision about *what* a click means lives in [`mouse::intent`]; this
/// only carries the answer out, so the rules stay testable without a terminal.
fn on_mouse(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut TuiSession,
    event: &MouseEvent,
) -> io::Result<()> {
    let area = Rect {
        x: 0,
        y: 0,
        width: terminal.size()?.width,
        height: terminal.size()?.height,
    };
    let max_back = scroll_span(terminal, session)?;
    let composer_height = view::composer_height(session, area.width);
    match mouse::intent_with_sidebar(
        event,
        area,
        composer_height,
        session.picker.as_ref(),
        mouse::Interaction {
            scrollbar_dragging: session.dragging,
            sidebar_open: session.sidebar.open,
            sidebar_width: session.sidebar.width,
            sidebar_dragging: session.sidebar.dragging,
            sidebar_state: session.sidebar.hit_state(),
        },
    ) {
        mouse::Intent::Scroll(rows) => session.scroll(rows, max_back),
        mouse::Intent::GrabTrack => session.dragging = true,
        mouse::Intent::Release => session.dragging = false,
        mouse::Intent::ToggleSidebar => session.sidebar.toggle(),
        mouse::Intent::OpenSidebar => session.sidebar.open = true,
        mouse::Intent::SidebarResizeStart => session.sidebar.dragging = true,
        mouse::Intent::SidebarResizeTo(width) => session.sidebar.resize_to(width),
        mouse::Intent::SidebarResizeEnd => session.sidebar.dragging = false,
        mouse::Intent::SidebarScroll(rows) => session.sidebar.scroll(rows),
        mouse::Intent::SidebarClose => session.sidebar.close(),
        mouse::Intent::NewSession => commands::new_session(session),
        mouse::Intent::SidebarSection(section) => session.sidebar.select(section),
        mouse::Intent::SidebarSession(index) => session.open_sidebar_session(index, false),
        mouse::Intent::SidebarPinnedSession(index) => session.open_sidebar_session(index, true),
        mouse::Intent::SidebarProject(index) => session.select_sidebar_project(index),
        mouse::Intent::ScrollTo(fraction) => session.scroll_to(fraction, max_back),
        mouse::Intent::Choose(index) => {
            if let Some(picker) = session.picker.as_mut() {
                picker.select(index);
            }
            session.confirm_picker();
        }
        mouse::Intent::PickerScroll(rows) => {
            if let Some(picker) = session.picker.as_mut() {
                for _ in 0..rows.unsigned_abs() {
                    if rows.is_positive() {
                        picker.up();
                    } else {
                        picker.down();
                    }
                }
            }
        }
        mouse::Intent::OpenMenu => session.picker = Some(commands::menu()),
        // Keyboard and pointer reach the same two moves: select the block, and
        // open or close it when the row clicked is the one that heads it.
        mouse::Intent::Inspect(row) => {
            let height = usize::from(
                mouse::regions_with_sidebar(
                    area,
                    composer_height,
                    session.sidebar.open,
                    session.sidebar.width,
                )
                .transcript
                .height,
            );
            let rows = view::visible_rows(session, view::transcript_width_for(session, area.width));
            let offset = view::scroll_offset(rows.len(), height, session.scroll_back);
            if let Some(hit) = view::hit(&rows, offset + row) {
                session.hovered_block = Some(hit.block);
                session.workbench.select_item(hit.block);
                if hit.head {
                    session.workbench.toggle_fold_of(hit.block);
                }
            }
        }
        mouse::Intent::Hover(row) => {
            let height = usize::from(
                mouse::regions_with_sidebar(
                    area,
                    composer_height,
                    session.sidebar.open,
                    session.sidebar.width,
                )
                .transcript
                .height,
            );
            let rows = view::visible_rows(session, view::transcript_width_for(session, area.width));
            let offset = view::scroll_offset(rows.len(), height, session.scroll_back);
            session.hovered_block =
                row.and_then(|row| view::hit(&rows, offset + row).map(|hit| hit.block));
        }
        mouse::Intent::Nothing => {}
    }
    Ok(())
}

/// Rows the transcript can scroll back before its top row is already on screen.
fn scroll_span(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    session: &TuiSession,
) -> io::Result<usize> {
    let size = terminal.size()?;
    // Transcript viewport: full frame minus the composer (which grows with a
    // multiline draft), context/status/help rails (3), and horizontal inset.
    let chrome = view::composer_height(session, size.width) + 3;
    let height = usize::from(size.height.saturating_sub(chrome));
    let rows =
        view::transcript_text(session, view::transcript_width_for(session, size.width)).len();
    Ok(view::max_scroll_back(rows, height))
}

/// Move the transcript one page in `direction` (+1 up into history, -1 back
/// toward the tail), clamped so the top row stops at the top of the screen.
fn scroll_page(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    session: &mut TuiSession,
    direction: isize,
) -> io::Result<()> {
    let size = terminal.size()?;
    let chrome = view::composer_height(session, size.width) + 3;
    let height = usize::from(size.height.saturating_sub(chrome));
    let page = height.saturating_sub(1).max(1) as isize;
    let max_back = scroll_span(terminal, session)?;
    session.scroll(direction * page, max_back);
    Ok(())
}

/// Rendered once so the widget tree stays testable without a terminal.
pub fn transcript_lines(session: &TuiSession, width: u16) -> Vec<String> {
    view::transcript_text(session, width)
}

pub(crate) fn bordered(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_style(
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
        .title(title)
}

pub(crate) fn wrapped(text: String) -> Paragraph<'static> {
    Paragraph::new(text).wrap(Wrap { trim: false })
}
