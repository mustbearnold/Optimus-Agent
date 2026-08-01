//! Terminal face of the Optimus agent host (ADR-0045).
//!
//! The TUI is not a remote client of the host — it *is* the host process, with a
//! terminal drawn on top. `optimus-host` is linked in and `handle_ipc` is called
//! directly, so there is no transport hop for the surface that owns the session.
//! Other surfaces attach to this process over stdio or loopback HTTP instead.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

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

mod activity;
mod commands;
mod completion;
mod composer;
mod history;
mod keys;
mod logging;
mod mouse;
mod picker;
mod preferences;
mod session;
mod tool_line;
mod transcript;
mod view;
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
    // Poll rather than block: a running turn must be able to paint streamed text
    // and accept Ctrl-C while the worker is still talking to the model.
    let frame = Duration::from_millis(40);
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
        session.pump();
        session.tick();
        if session.mouse != captured {
            captured = session.mouse;
            if captured {
                terminal.backend_mut().execute(EnableMouseCapture)?;
            } else {
                terminal.backend_mut().execute(DisableMouseCapture)?;
            }
        }
        terminal.draw(|f| view::draw(f, &session))?;

        if !event::poll(frame)? {
            continue;
        }
        let key = match event::read()? {
            Event::Key(key) => key,
            Event::Mouse(mouse) => {
                on_mouse(terminal, &mut session, &mouse)?;
                continue;
            }
            // A paste is text, never a submit — the whole point of turning
            // bracketed paste on.
            Event::Paste(text) => {
                session.history.release();
                session.composer.insert_str(&text);
                continue;
            }
            _ => continue,
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        #[cfg(debug_assertions)]
        if panic_key.is_some_and(|wanted| key.code == crossterm::event::KeyCode::Char(wanted)) {
            panic!("OPTIMUS_TUI_PANIC_ON_KEY");
        }
        let mode = keys::Mode {
            picker: session.picker.is_some(),
            busy: session.busy(),
            drafting: !session.composer.is_empty(),
            suggesting: !completion::suggestions(session.composer.text()).is_empty(),
        };
        if on_key(terminal, &mut session, keys::intent(&key, mode))? {
            return Ok(());
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
    use keys::{Edit, HistoryStep, Intent, Motion, PickerStep, ScrollStep, SuggestStep};
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
        Intent::Redraw => terminal.clear()?,
        Intent::Ignore => {}
    }
    Ok(false)
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
    match mouse::intent(
        event,
        area,
        composer_height,
        session.picker.as_ref(),
        session.dragging,
    ) {
        mouse::Intent::Scroll(rows) => session.scroll(rows, max_back),
        mouse::Intent::GrabTrack => session.dragging = true,
        mouse::Intent::Release => session.dragging = false,
        mouse::Intent::ScrollTo(fraction) => session.scroll_to(fraction, max_back),
        mouse::Intent::Choose(index) => {
            if let Some(picker) = session.picker.as_mut() {
                picker.select(index);
            }
            session.confirm_picker();
        }
        mouse::Intent::OpenMenu => session.picker = Some(commands::menu()),
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
    // multiline draft), status (1), and the transcript's own borders (2).
    let chrome = view::composer_height(session, size.width) + 3;
    let height = usize::from(size.height.saturating_sub(chrome));
    let rows = view::transcript_text(session, size.width.saturating_sub(2)).len();
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
    Block::default().borders(Borders::ALL).title(title)
}

pub(crate) fn wrapped(text: String) -> Paragraph<'static> {
    Paragraph::new(text).wrap(Wrap { trim: false })
}
