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
//!
//! The second half of the file asserts what a terminal shows *while a turn is
//! in flight* — the interrupt, the spinner, text arriving a piece at a time.
//! None of that was reachable before, because the offline model answered in the
//! same tick the turn started and left no window to observe or interrupt. It
//! now takes `OPTIMUS_OFFLINE_LATENCY_MS` between chunks, unset everywhere
//! except here.

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
        Self::launch_paced(0)
    }

    /// Launch with the offline model taking `pace_ms` before each chunk of its
    /// answer, so a turn stays in flight long enough to observe and interrupt.
    /// At zero — every test that does not need a running turn — the model
    /// behaves exactly as it always has.
    fn launch_paced(pace_ms: u64) -> Self {
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
        if pace_ms > 0 {
            command.env("OPTIMUS_OFFLINE_LATENCY_MS", pace_ms.to_string());
        }
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

    /// The glyph leading the activity row, or `None` when nothing is running.
    /// Found by the text beside it rather than by a hard-coded frame list, so
    /// changing the animation does not break the test that it animates.
    fn spinner(&self) -> Option<char> {
        self.screen()
            .lines()
            .find(|line| line.contains(BUSY))
            // The row arrives inside the transcript pane, so the first glyph on
            // the terminal line is the pane's own border, not the spinner.
            .and_then(|line| line.trim_start_matches(['│', ' ']).chars().next())
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

/// Enough of a pause between chunks that a test can act between two of them
/// without racing, and short enough that three tests using it stay quick.
const PACE_MS: u64 = 1000;

/// A prompt whose echo straddles a chunk boundary. The offline model answers
/// `offline echo: {prompt}` in twelve-character pieces, so the first piece is
/// exactly `offline echo` and the marker cannot reach the screen until a later
/// one. While a turn is mid-flight the marker is therefore on screen exactly
/// once — in the user's own row — which is what makes "the assistant never
/// finished its sentence" checkable by counting.
const PROMPT: &str = "PARTIAL";

/// Substring of the activity row's interrupt hint. Deliberately not the whole
/// phrase: at forty columns the row runs out of width and the hint clips to
/// `Ctrl-C to inter`, so a test matching the full sentence silently decides no
/// turn is ever running.
const BUSY: &str = "Ctrl-C";

#[test]
fn ctrl_c_during_a_turn_interrupts_it_and_leaves_the_session_usable() {
    let mut term = Term::launch_paced(PACE_MS);
    term.send(format!("{PROMPT}\r").as_bytes());
    term.wait_for(
        |s| s.contains("offline echo"),
        "the turn never started streaming",
    );
    assert_eq!(
        term.screen().matches(PROMPT).count(),
        1,
        "precondition: only the user's row carries the marker yet"
    );

    term.send(b"\x03");
    let screen = term.wait_for(
        |s| !s.contains(BUSY),
        "Ctrl-C never took the turn out of flight",
    );

    // The whole point of an interrupt: the rest of the answer never arrives.
    // The activity row is gone, so the turn has settled and no further delta
    // can land — a second marker on screen would mean the stream ran on.
    assert_eq!(
        screen.matches(PROMPT).count(),
        1,
        "the answer kept streaming after the interrupt:\n{screen}"
    );
    // And the far worse failure — the one this test exists for — is Ctrl-C
    // being read as "quit" and taking the session down mid-answer.
    assert!(
        !term.exited_within(Duration::from_millis(300)),
        "Ctrl-C killed the session instead of the turn"
    );
    term.send(b"~");
    term.wait_for(
        |s| s.contains('~'),
        "the composer stopped accepting input after an interrupt",
    );
}

#[test]
fn the_spinner_turns_while_a_turn_is_in_flight() {
    let mut term = Term::launch_paced(PACE_MS);
    term.send(b"spin\r");
    term.wait_for(
        |s| s.contains(BUSY),
        "no activity row while a turn was running",
    );

    // A spinner painted once and never repainted looks identical to a live one
    // in any single frame, so the assertion has to be that it *changed*.
    let first = term.spinner().expect("an activity row while busy");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut moved = false;
    while Instant::now() < deadline {
        if term.spinner().is_some_and(|glyph| glyph != first) {
            moved = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        moved,
        "the spinner never advanced past {first:?}, so the frame is frozen while the turn runs:\n{}",
        term.screen()
    );
}

#[test]
fn the_answer_paints_as_it_arrives_rather_than_all_at_once() {
    let mut term = Term::launch_paced(PACE_MS);
    term.send(format!("{PROMPT}\r").as_bytes());

    // First chunk on screen, and the answer demonstrably unfinished: a face
    // that buffered the whole reply and painted it once would never be caught
    // in this state.
    let mid = term.wait_for(
        |s| s.contains("offline echo"),
        "the first chunk never painted",
    );
    assert_eq!(
        mid.matches(PROMPT).count(),
        1,
        "the whole answer landed in one paint, so streaming is not reaching the screen:\n{mid}"
    );

    term.wait_for(
        |s| s.matches(PROMPT).count() == 2,
        "the rest of the answer never arrived",
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
