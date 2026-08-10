//! The composer's text buffer: what the human is typing, where the cursor
//! is, and every edit a key can perform.
//!
//! All motion and deletion operate on extended grapheme clusters, not
//! `char`s — `String::pop` on `"👩‍�py👦"`-class input deletes half a cluster
//! and paints tofu, which is exactly the defect this type replaces. The
//! cursor is a byte offset into `text` and is always on a grapheme
//! boundary. Kills and Home/End are scoped to the current line because the
//! buffer is multiline; the view derives visual layout from `text()` +
//! `cursor()` and owns wrapping.

use unicode_segmentation::UnicodeSegmentation;

/// A word for motion purposes: a segment carrying letters or digits.
fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

#[derive(Debug, Default)]
pub struct Composer {
    text: String,
    cursor: usize,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Drain the buffer for submit; the cursor resets with it.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }

    /// Replace the draft wholesale (history recall); cursor lands at the end.
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\r' {
            self.newline();
            return;
        }
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.snap();
    }

    /// Bulk insertion — the paste sink. Newlines are preserved as newlines
    /// (never as submits); CRLF and lone CR normalize to `\n`.
    pub fn insert_str(&mut self, s: &str) {
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        self.text.insert_str(self.cursor, &normalized);
        self.cursor += normalized.len();
        self.snap();
    }

    pub fn newline(&mut self) {
        self.text.insert(self.cursor, '\n');
        self.cursor += 1;
        self.snap();
    }

    pub fn backspace(&mut self) {
        let start = self.prev_boundary();
        self.text.drain(start..self.cursor);
        self.cursor = start;
        self.snap();
    }

    pub fn delete(&mut self) {
        let end = self.next_boundary();
        self.text.drain(self.cursor..end);
        self.snap();
    }

    pub fn left(&mut self) {
        self.cursor = self.prev_boundary();
    }

    pub fn right(&mut self) {
        self.cursor = self.next_boundary();
    }

    /// Move to the start of the previous word, where a word is a run
    /// containing letters or digits. Punctuation is stepped over rather than
    /// stopped on: `--release` is one hop, not three.
    pub fn word_left(&mut self) {
        let mut target = 0;
        for (i, word) in self.text.split_word_bound_indices() {
            if i >= self.cursor {
                break;
            }
            if is_word(word) {
                target = i;
            }
        }
        self.cursor = target;
    }

    pub fn word_right(&mut self) {
        for (i, word) in self.text.split_word_bound_indices() {
            let end = i + word.len();
            if end > self.cursor && is_word(word) {
                self.cursor = end;
                return;
            }
        }
        self.cursor = self.text.len();
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start();
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end();
    }

    /// Ctrl-K: kill to end of line; at end of line, join with the next
    /// (readline semantics).
    pub fn kill_to_end(&mut self) {
        let end = self.line_end();
        if self.cursor == end && end < self.text.len() {
            self.text.drain(end..end + 1);
        } else {
            self.text.drain(self.cursor..end);
        }
        self.snap();
    }

    /// Ctrl-U: kill from start of line to the cursor.
    pub fn kill_to_start(&mut self) {
        let start = self.line_start();
        self.text.drain(start..self.cursor);
        self.cursor = start;
        self.snap();
    }

    /// Ctrl-W: kill back to the previous whitespace, punctuation included —
    /// readline's unix-word-rubout. Deleting `--release` should take one
    /// chord, not three, which is why this is not `word_left` plus a drain.
    pub fn kill_word(&mut self) {
        let end = self.cursor;
        let head = &self.text[..end];
        let trimmed = head.trim_end_matches(char::is_whitespace);
        let start = trimmed
            .rfind(char::is_whitespace)
            .map(|i| i + trimmed[i..].chars().next().map_or(1, char::len_utf8))
            .unwrap_or(0);
        self.text.drain(start..end);
        self.cursor = start;
        self.snap();
    }

    /// Pull the cursor back to a cluster start if an edit fused it inside one.
    /// Inserting a base character immediately before a combining mark makes the
    /// two a single glyph — `a` typed before U+0301 becomes `á` — and a cursor
    /// left between them is inside a glyph the terminal paints as one column.
    /// Every slice this type takes assumes a boundary, and the view counts
    /// clusters to place the caret, so the two would disagree about where
    /// typing lands. Backwards, never forwards: a caret must not travel in the
    /// opposite direction to the edit that moved it.
    fn snap(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        if self.cursor == 0 || self.cursor == self.text.len() {
            return;
        }
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|&index| index <= self.cursor)
            .last()
            .unwrap_or(0);
    }

    fn line_start_of(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map(|g| self.cursor + g.len())
            .unwrap_or(self.text.len())
    }

    fn line_start(&self) -> usize {
        self.line_start_of(self.cursor)
    }

    fn line_end(&self) -> usize {
        self.cursor
            + self.text[self.cursor..]
                .find('\n')
                .unwrap_or(self.text.len() - self.cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::Composer;

    fn composer_with(text: &str) -> Composer {
        let mut c = Composer::new();
        c.set(text);
        c
    }

    #[test]
    fn backspace_removes_a_whole_grapheme_cluster_not_a_char() {
        // A family emoji is several scalars joined by ZWJ; popping a char
        // leaves tofu. One backspace must remove the whole cluster.
        let mut c = composer_with("hi👩‍👩‍👦");
        c.backspace();
        assert_eq!(c.text(), "hi");
        c.backspace();
        assert_eq!(c.text(), "h");
    }

    #[test]
    fn delete_removes_the_cluster_under_the_cursor() {
        let mut c = composer_with("a👍b");
        c.home();
        c.right();
        c.delete();
        assert_eq!(c.text(), "ab");
        assert_eq!(c.cursor(), 1);
    }

    #[test]
    fn arrows_move_by_grapheme_and_clamp_at_the_edges() {
        let mut c = composer_with("aé👍");
        c.left();
        c.left();
        c.insert_char('x');
        assert_eq!(c.text(), "axé👍");
        c.home();
        c.left();
        assert_eq!(c.cursor(), 0);
        c.end();
        c.right();
        assert_eq!(c.cursor(), c.text().len());
    }

    #[test]
    fn word_jumps_land_on_word_starts_and_ends() {
        let mut c = composer_with("cargo build --release");
        c.word_left();
        assert_eq!(&c.text()[c.cursor()..], "release");
        c.word_left();
        assert_eq!(
            &c.text()[c.cursor()..],
            "build --release",
            "punctuation is stepped over, not stopped on"
        );
        c.home();
        c.word_right();
        assert_eq!(&c.text()[..c.cursor()], "cargo");
        c.word_right();
        assert_eq!(&c.text()[..c.cursor()], "cargo build");
    }

    #[test]
    fn paste_preserves_newlines_and_normalizes_crlf() {
        // The whole point of bracketed paste: a multiline paste is text,
        // never a submit.
        let mut c = Composer::new();
        c.insert_str("line one\r\nline two\rline three");
        assert_eq!(c.text(), "line one\nline two\nline three");
        assert_eq!(c.cursor(), c.text().len());
    }

    #[test]
    fn kills_are_scoped_to_the_current_line() {
        let mut c = composer_with("first\nsecond\nthird");
        c.home();
        // Cursor at start of "third"; Ctrl-K kills the line's text.
        c.kill_to_end();
        assert_eq!(c.text(), "first\nsecond\n");
        // At end of line, Ctrl-K joins with the next line.
        let mut c = composer_with("ab\ncd");
        c.home();
        c.end();
        // cursor sits at end of "cd" (last line): nothing to join.
        c.kill_to_end();
        assert_eq!(c.text(), "ab\ncd");
        let mut c = composer_with("ab\ncd");
        c.left(); // between 'c' and 'd'? no: end of text minus one grapheme
        c.home();
        // cursor at start of "cd"; move up conceptually not needed — place
        // at end of first line via explicit cursor math:
        let mut c2 = composer_with("ab\ncd");
        c2.home(); // start of "cd"
        c2.left(); // onto the newline boundary — end of "ab"
        c2.kill_to_end();
        assert_eq!(c2.text(), "abcd");
        let _ = c;
    }

    #[test]
    fn kill_to_start_and_kill_word_edit_backwards() {
        let mut c = composer_with("run the tests");
        c.kill_word();
        assert_eq!(c.text(), "run the ");
        // Whitespace-delimited: punctuation goes with the word it hangs on.
        let mut flags = composer_with("cargo build --release");
        flags.kill_word();
        assert_eq!(flags.text(), "cargo build ");
        c.kill_to_start();
        assert_eq!(c.text(), "");
        let mut c = composer_with("one\ntwo three");
        c.kill_to_start();
        assert_eq!(c.text(), "one\n");
    }

    #[test]
    fn take_drains_and_resets_for_the_next_draft() {
        let mut c = composer_with("ship it");
        assert_eq!(c.take(), "ship it");
        assert!(c.is_empty());
        assert_eq!(c.cursor(), 0);
        c.insert_char('x');
        assert_eq!(c.text(), "x");
    }
}

/// Invariants, checked against generated edit sequences rather than the
/// handful of strings a person thinks to write down.
///
/// Every method here is byte-offset arithmetic over UTF-8 that must land on
/// extended grapheme cluster boundaries. The interesting inputs — a combining
/// acute after a vowel, a ZWJ family, a wide CJK glyph next to an ASCII space —
/// are exactly the ones absent from hand-written cases, and slicing a `String`
/// off a boundary panics rather than degrading.
#[cfg(test)]
mod properties {
    use super::Composer;
    use proptest::prelude::*;
    use unicode_segmentation::UnicodeSegmentation;

    #[derive(Debug, Clone, Copy)]
    enum Op {
        Left,
        Right,
        WordLeft,
        WordRight,
        Home,
        End,
        Backspace,
        Delete,
        KillToEnd,
        KillToStart,
        KillWord,
        Newline,
        Insert(char),
    }

    /// A deliberately awkward alphabet: a combining mark that fuses with the
    /// grapheme before it, a ZWJ emoji, a wide glyph, and the whitespace and
    /// punctuation the word motions key off.
    fn grapheme() -> impl Strategy<Value = char> {
        prop_oneof![
            Just('a'),
            Just(' '),
            Just('-'),
            Just('é'),
            Just('中'),
            Just('👍'),
            Just('\u{0301}'),
        ]
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            Just(Op::Left),
            Just(Op::Right),
            Just(Op::WordLeft),
            Just(Op::WordRight),
            Just(Op::Home),
            Just(Op::End),
            Just(Op::Backspace),
            Just(Op::Delete),
            Just(Op::KillToEnd),
            Just(Op::KillToStart),
            Just(Op::KillWord),
            Just(Op::Newline),
            grapheme().prop_map(Op::Insert),
        ]
    }

    fn apply(composer: &mut Composer, op: Op) {
        match op {
            Op::Left => composer.left(),
            Op::Right => composer.right(),
            Op::WordLeft => composer.word_left(),
            Op::WordRight => composer.word_right(),
            Op::Home => composer.home(),
            Op::End => composer.end(),
            Op::Backspace => composer.backspace(),
            Op::Delete => composer.delete(),
            Op::KillToEnd => composer.kill_to_end(),
            Op::KillToStart => composer.kill_to_start(),
            Op::KillWord => composer.kill_word(),
            Op::Newline => composer.newline(),
            Op::Insert(c) => composer.insert_char(c),
        }
    }

    fn on_a_boundary(text: &str, cursor: usize) -> bool {
        text.grapheme_indices(true)
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .any(|index| index == cursor)
    }

    proptest! {
        /// The one that keeps the process alive: every slice this type takes is
        /// `text[..cursor]` or `text[cursor..]`, and a cursor off a boundary
        /// panics the whole TUI mid-keystroke.
        #[test]
        fn the_cursor_stays_on_a_grapheme_boundary(
            seed in prop::collection::vec(grapheme(), 0..24),
            ops in prop::collection::vec(op(), 0..40),
        ) {
            let mut composer = Composer::new();
            composer.set(seed.into_iter().collect::<String>());
            prop_assert!(on_a_boundary(composer.text(), composer.cursor()));
            for op in ops {
                apply(&mut composer, op);
                prop_assert!(
                    on_a_boundary(composer.text(), composer.cursor()),
                    "{op:?} left the cursor at {} inside {:?}",
                    composer.cursor(),
                    composer.text(),
                );
                prop_assert!(composer.cursor() <= composer.text().len());
                let (line_start, line_end) = (composer.line_start(), composer.line_end());
                prop_assert!(
                    line_start <= composer.cursor() && composer.cursor() <= line_end,
                    "{op:?} left the cursor outside its line: {line_start}..={line_end} vs {}",
                    composer.cursor(),
                );
            }
        }

        /// Insert-then-backspace is identity at the grapheme level: the two
        /// primitive edits compose to nothing wherever the cursor sits. Both
        /// the seed and the inserted char exclude the combining mark: a base
        /// char typed before it fuses into one glyph (`á`), and `snap()` then
        /// pulls the caret back to the cluster start — backspace at a cluster
        /// start removes nothing, which is correct editor behaviour and a
        /// different invariant (already pinned by the boundary property test,
        /// which includes combining marks).
        #[test]
        fn insert_then_backspace_is_identity(
            seed in prop::collection::vec(
                prop_oneof![Just('a'), Just(' '), Just('-'), Just('é'), Just('中'), Just('👍')],
                0..24,
            ),
            steps in 0..24usize,
            ch in prop_oneof![Just('a'), Just(' '), Just('-'), Just('é'), Just('中'), Just('👍')],
        ) {
            let mut composer = Composer::new();
            composer.set(seed.into_iter().collect::<String>());
            let before = composer.text().to_string();
            composer.home();
            for _ in 0..steps {
                composer.right(); // boundary-aware; extra moves are no-ops
            }
            composer.insert_char(ch);
            composer.backspace();
            prop_assert_eq!(composer.text(), before, "insert+backspace must be identity");
            prop_assert!(on_a_boundary(composer.text(), composer.cursor()));
        }

        /// Submitting hands the whole draft over and leaves nothing behind —
        /// a stale cursor into a drained buffer is an immediate panic.
        #[test]
        fn taking_the_draft_drains_it_completely(
            seed in prop::collection::vec(grapheme(), 0..24),
            ops in prop::collection::vec(op(), 0..20),
        ) {
            let mut composer = Composer::new();
            composer.set(seed.into_iter().collect::<String>());
            for op in ops {
                apply(&mut composer, op);
            }
            let taken = composer.take();
            prop_assert_eq!(composer.text(), "");
            prop_assert_eq!(composer.cursor(), 0);
            prop_assert!(composer.is_empty());
            prop_assert!(taken.is_char_boundary(0));
        }
    }
}
