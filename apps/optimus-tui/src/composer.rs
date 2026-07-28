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
    }

    /// Bulk insertion — the paste sink. Newlines are preserved as newlines
    /// (never as submits); CRLF and lone CR normalize to `\n`.
    pub fn insert_str(&mut self, s: &str) {
        let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
        self.text.insert_str(self.cursor, &normalized);
        self.cursor += normalized.len();
    }

    pub fn newline(&mut self) {
        self.text.insert(self.cursor, '\n');
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        let start = self.prev_boundary();
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn delete(&mut self) {
        let end = self.next_boundary();
        self.text.drain(self.cursor..end);
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
    }

    /// Ctrl-U: kill from start of line to the cursor.
    pub fn kill_to_start(&mut self) {
        let start = self.line_start();
        self.text.drain(start..self.cursor);
        self.cursor = start;
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
