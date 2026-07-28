//! The real binary, in a real pty, read back through a vt100 parser.
//!
//! Every other test in this crate can assert each cell of the buffer and still
//! miss the cursor entirely, because the cursor is not *in* the buffer — it is
//! terminal state, set by `frame.set_cursor_position` and carried out over the
//! wire as an escape sequence. `view::composer::layout` is pure and thoroughly
//! tested, and none of that says the answer ever reaches the terminal.
//!
//! That gap was not hypothetical: `view::draw` handed `height` the block's
//! outer width while `render` handed `layout` the inner width, so a draft a
//! little wider than the box wrapped to two rows inside a box sized for one and
//! the first row scrolled out from under the person typing it. Both pure
//! functions were correct. Their wiring was not.
//!
//! So these tests drive the shipped binary the way a person does — spawn it on
//! a pty, type bytes, parse the escape stream into a screen — and assert what a
//! terminal actually shows, cursor included. Deterministic: a temp home, the
//! offline provider, no credentials, no network, no token spend.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

/// Wide enough that a sentence-length draft fits on one row, narrow enough that
/// a slightly longer one is forced to wrap where the test can predict it.
const COLS: u16 = 40;
const ROWS: u16 = 20;

/// Usable text columns inside the composer: the two border columns, the
/// two-column prompt gutter, and the column kept free so a cursor sitting after
/// a full row still paints inside the border. Duplicated from
/// `view::composer::text_width` on purpose — a test that imports the constant it
/// is checking proves only that the constant equals itself.
const TEXT_WIDTH: usize = COLS as usize - 5;

/// The composer is bottom-anchored above a one-row status line, and it grows
/// *upward* as the draft wraps. The row the cursor sits on while typing the
/// last line of a draft is therefore fixed, whatever the draft's height.
const CURSOR_ROW: u16 = ROWS - 3;

struct Term {
    _home: tempfile::TempDir,
    _master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
}

impl Term {
    fn launch() -> Self {
        let home = tempfile::tempdir().expect("temp home");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open pty");

        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_optimus-tui"));
        command.arg(home.path());
        // Without a terminal type crossterm cannot emit the sequences under
        // test, and the whole exercise is about those sequences.
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command).expect("spawn the tui");
        // The child owns the slave now; holding it here would keep the pty open
        // past the child's exit and hang the reader thread.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        let writer = pair.master.take_writer().expect("pty writer");
        let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));

        // Drain continuously. A test that read on demand would deadlock the
        // child as soon as the pty buffer filled.
        let sink = Arc::clone(&parser);
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                sink.lock().expect("parser").process(&buffer[..read]);
            }
        });

        let term = Self {
            _home: home,
            _master: pair.master,
            child,
            writer,
            parser,
        };
        term.wait_for(
            |screen| screen.contains("ready"),
            "launch never reached the ready status line",
        );
        term
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to the pty");
        self.writer.flush().expect("flush the pty");
    }

    fn screen(&self) -> String {
        self.parser.lock().expect("parser").screen().contents()
    }

    /// Row, column — the position a terminal would put its caret at.
    fn cursor(&self) -> (u16, u16) {
        self.parser
            .lock()
            .expect("parser")
            .screen()
            .cursor_position()
    }

    fn wait_for(&self, predicate: impl Fn(&str) -> bool, what: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let screen = self.screen();
            if predicate(&screen) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "{what}\n--- last frame ---\n{screen}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn exited_within(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_cursor_lands_where_the_next_character_will_be_typed() {
    let mut term = Term::launch();
    term.send(b"hello");
    term.wait_for(|s| s.contains("hello"), "the typed draft never painted");

    assert_eq!(
        term.cursor(),
        (CURSOR_ROW, 8),
        "left border (1) + gutter (2) + five graphemes"
    );
}

#[test]
fn the_cursor_follows_a_wrapped_draft_and_the_first_row_stays_visible() {
    let mut term = Term::launch();
    // One grapheme past the wrap: two rows, with the overflow alone on the
    // second. This is the exact shape that used to be sized for one row.
    let head = "a".repeat(TEXT_WIDTH);
    // The overflow grapheme has to be one the idle placeholder cannot supply,
    // or the wait returns before a single keystroke has landed.
    let draft = format!("{head}Z");
    term.send(draft.as_bytes());
    term.wait_for(|s| s.contains('Z'), "the wrapped draft never painted");

    let screen = term.screen();
    assert!(
        screen.contains(&head),
        "the first wrapped row scrolled out from under the typist:\n{screen}"
    );
    assert_eq!(
        term.cursor(),
        (CURSOR_ROW, 4),
        "border (1) + gutter (2) + the single grapheme on the second row"
    );
}

#[test]
fn arrow_keys_move_the_terminal_cursor_not_only_the_model() {
    let mut term = Term::launch();
    term.send(b"hello");
    term.wait_for(|s| s.contains("hello"), "the typed draft never painted");

    term.send(b"\x1b[D\x1b[D");
    let deadline = Instant::now() + Duration::from_secs(5);
    while term.cursor().1 != 6 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        term.cursor(),
        (CURSOR_ROW, 6),
        "two Left presses must move the painted caret, not just the buffer"
    );
}

#[test]
fn quitting_gives_the_terminal_back() {
    let mut term = Term::launch();
    term.send(b"/quit\r");

    assert!(
        term.exited_within(Duration::from_secs(15)),
        "/quit did not exit:\n{}",
        term.screen()
    );
    assert!(
        !term
            .parser
            .lock()
            .expect("parser")
            .screen()
            .alternate_screen(),
        "the alternate screen was never left, so the user's scrollback is gone"
    );
}
