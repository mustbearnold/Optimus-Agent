//! Terminal face of the Optimus agent host (ADR-0045).
//!
//! The TUI is not a remote client of the host — it *is* the host process, with a
//! terminal drawn on top. `optimus-host` is linked in and `handle_ipc` is called
//! directly, so there is no transport hop for the surface that owns the session.
//! Other surfaces attach to this process over stdio or loopback HTTP instead.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseEvent,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

mod commands;
mod logging;
mod mouse;
mod picker;
mod preferences;
mod session;
mod tool_line;
mod transcript;
mod view;

pub use session::{Message, Role, TuiSession};

/// Run the terminal face against an Optimus home directory.
pub fn run(home: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Taken before the screen is, and dropped after it is handed back, so no
    // in-process library write can land in the middle of a frame.
    let _stderr = logging::StderrLog::to_file(&logging::log_path(&home))?;
    let mut terminal = enter()?;
    let result = event_loop(&mut terminal, TuiSession::new(home));
    leave(terminal)?;
    result
}

fn enter() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    // Capture costs the terminal's own text selection, which is why `/mouse`
    // exists to hand it back.
    stdout.execute(EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn leave(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
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
            _ => continue,
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        // A picker owns the keyboard while it is open.
        if session.picker.is_some() {
            match key.code {
                KeyCode::Down | KeyCode::Tab => {
                    if let Some(p) = session.picker.as_mut() {
                        p.down();
                    }
                }
                KeyCode::Up | KeyCode::BackTab => {
                    if let Some(p) = session.picker.as_mut() {
                        p.up();
                    }
                }
                KeyCode::Enter => session.confirm_picker(),
                KeyCode::Esc => session.picker = None,
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                    session.picker = None;
                }
                _ => {}
            }
            continue;
        }

        match (key.modifiers, key.code) {
            // Ctrl-C stops a run in flight; with nothing running it exits.
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if session.busy() {
                    session.cancel();
                } else {
                    return Ok(());
                }
            }
            (_, KeyCode::Esc) if !session.busy() => return Ok(()),
            (_, KeyCode::Enter) => session.submit(),
            (_, KeyCode::Backspace) => {
                session.composer.pop();
            }
            (_, KeyCode::PageUp) => scroll_page(terminal, &mut session, 1)?,
            (_, KeyCode::PageDown) => scroll_page(terminal, &mut session, -1)?,
            (_, KeyCode::End) => session.scroll_back = 0,
            (_, KeyCode::Char(c)) => session.composer.push(c),
            _ => {}
        }
    }
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
    match mouse::intent(event, area, session.picker.as_ref(), session.dragging) {
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
    // Transcript viewport: full frame minus composer (3), status (1), borders (2).
    let height = usize::from(size.height.saturating_sub(6));
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
    let height = usize::from(terminal.size()?.height.saturating_sub(6));
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
