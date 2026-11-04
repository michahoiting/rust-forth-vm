//! The input source: the line of text being read by the outer interpreter and
//! by parsing words such as `:` and `CREATE`.

#[derive(Default)]
pub(crate) struct InputSource {
    text: String, // The line being interpreted
    pos: usize,   // Parse offset into `text`
}

impl InputSource {
    // Replaces the current line and starts parsing from its beginning.
    pub(crate) fn load(&mut self, text: &str) {
        self.text = text.to_string();
        self.pos = 0;
    }

    // Parses the next whitespace-delimited token.
    pub(crate) fn parse_name(&mut self) -> Option<String> {
        let rest = &self.text[self.pos..];
        let start = rest.find(|c: char| !c.is_whitespace())?;
        let tail = &rest[start..];
        let len = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let token = tail[..len].to_string();
        self.pos += start + len;
        Some(token)
    }
}
