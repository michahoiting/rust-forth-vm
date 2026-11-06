//! The input source: the text being read by the outer interpreter and by parsing
//! words such as `:`, `CREATE` and `(`.

/// The text being read, and how far parsing has got.
#[derive(Default)]
pub(crate) struct InputSource {
    /// The text being interpreted, which can span several lines.
    text: String,
    /// Parse offset into `text`.
    pos: usize,
}

impl InputSource {
    /// Replaces the current text and starts parsing from its beginning.
    pub(crate) fn load(&mut self, text: &str) {
        self.text = text.to_string();
        self.pos = 0;
    }

    /// Parses the next whitespace-delimited token. Returns `None` at the end of the text.
    pub(crate) fn parse_name(&mut self) -> Option<String> {
        let rest = &self.text[self.pos..];
        let start = rest.find(|c: char| !c.is_whitespace())?;
        let tail = &rest[start..];
        let len = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let token = tail[..len].to_string();
        self.pos += start + len;
        Some(token)
    }

    /// Skips up to and including the next `delimiter`. Returns false, without moving, if there
    /// isn't one.
    pub(crate) fn skip_past(&mut self, delimiter: char) -> bool {
        match self.text[self.pos..].find(delimiter) {
            Some(offset) => {
                self.pos += offset + delimiter.len_utf8();
                true
            }
            None => false,
        }
    }

    /// Skips the rest of the current line.
    pub(crate) fn skip_line(&mut self) {
        if !self.skip_past('\n') {
            self.pos = self.text.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> InputSource {
        let mut input = InputSource::default();
        input.load(text);
        input
    }

    #[test]
    fn skip_past_moves_past_the_delimiter() {
        let mut input = source("skipped ) next");
        assert!(input.skip_past(')'));
        assert_eq!(input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn skip_past_starts_at_the_current_position() {
        let mut input = source(") first ) second");
        input.parse_name();
        assert!(input.skip_past(')'));
        assert_eq!(input.parse_name().as_deref(), Some("second"));
    }

    #[test]
    fn skip_past_reports_a_missing_delimiter_without_moving() {
        let mut input = source("no delimiter");
        assert!(!input.skip_past(')'));
        assert_eq!(input.parse_name().as_deref(), Some("no"));
    }

    #[test]
    fn skip_past_handles_non_ascii_text() {
        let mut input = source("één ) next");
        assert!(input.skip_past(')'));
        assert_eq!(input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn skip_line_stops_after_the_next_newline() {
        let mut input = source("skipped\nnext");
        input.skip_line();
        assert_eq!(input.parse_name().as_deref(), Some("next"));
    }

    #[test]
    fn skip_line_stops_at_the_end_of_the_text() {
        let mut input = source("skipped");
        input.skip_line();
        assert_eq!(input.parse_name(), None);
    }

    #[test]
    fn skip_line_handles_an_empty_rest_of_the_line() {
        let mut input = source("first\nnext");
        input.parse_name();
        input.skip_line();
        assert_eq!(input.parse_name().as_deref(), Some("next"));
    }
}
