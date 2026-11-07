// Copyright (c) 2026 M.W.R. Hoiting

//! Line editing for the interactive prompt on a terminal, with a history that's kept between
//! sessions.

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

/// The name of the history file in the home directory.
const HISTORY_FILE: &str = ".rustforth_history";

/// The text in front of each line. Forth traditionally shows none, and answers with `ok`.
const PROMPT: &str = "";

/// Reads edited lines from the terminal.
pub(crate) struct LineEditor {
    /// The line editor, which holds the history of this session.
    editor: DefaultEditor,
    /// Where to keep the history, or `None` to keep it for this session only.
    history: Option<PathBuf>,
}

impl LineEditor {
    /// Creates an editor with the history of earlier sessions. If that can't be read, this
    /// warns on stderr and keeps the history for this session only, which leaves the file as
    /// it is.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal can't be set up.
    pub(crate) fn new() -> rustyline::Result<Self> {
        let mut editor = DefaultEditor::new()?;
        let mut history = history_path(env::var_os("HOME"));
        if let Some(path) = &history {
            if let Err(error) = editor.load_history(path) {
                if !is_missing_file(&error) {
                    warn(path, &error);
                    history = None;
                }
            }
        }
        Ok(Self { editor, history })
    }

    /// Reads a line, and adds it to the history. Ctrl-C discards the line being edited and
    /// starts a new one. Returns the exit code instead if the program should end.
    pub(crate) fn read_line(&mut self) -> Result<String, ExitCode> {
        loop {
            if let Some(result) = outcome(self.editor.readline(PROMPT)) {
                let line = result?;
                self.remember(&line);
                return Ok(line);
            }
        }
    }

    /// Adds a line to the history, unless it's blank. It's saved right away, because some Forth
    /// input can still crash the process. If saving fails, this warns on stderr once, and keeps
    /// the history for this session only.
    fn remember(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        let added = self.editor.add_history_entry(line).map(|_| ());
        if let Some(path) = &self.history {
            if let Err(error) = added.and_then(|()| self.editor.append_history(path)) {
                warn(path, &error);
                self.history = None;
            }
        }
    }
}

/// Returns the path of the history file in the home directory `home`, if there is one.
fn history_path(home: Option<OsString>) -> Option<PathBuf> {
    home.filter(|home| !home.is_empty())
        .map(|home| Path::new(&home).join(HISTORY_FILE))
}

/// Returns whether the error says that a file doesn't exist.
fn is_missing_file(error: &ReadlineError) -> bool {
    matches!(error, ReadlineError::Io(error) if error.kind() == io::ErrorKind::NotFound)
}

/// Warns on stderr that the history file can't be used.
fn warn(path: &Path, error: &ReadlineError) {
    eprintln!("Cannot use the history file {}: {}", path.display(), error);
}

/// Says what to do with the result of reading a line: use the line, or end the program with
/// the exit code. Returns `None` to read another line instead, after Ctrl-C. The editor has
/// already ended the output line.
fn outcome(result: rustyline::Result<String>) -> Option<Result<String, ExitCode>> {
    match result {
        Ok(line) => Some(Ok(line)),
        Err(ReadlineError::Interrupted) => None,
        Err(ReadlineError::Eof) => Some(Err(ExitCode::SUCCESS)),
        Err(error) => {
            eprintln!("Cannot read the input: {}", error);
            Some(Err(ExitCode::FAILURE))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_file_is_in_the_home_directory() {
        assert_eq!(
            history_path(Some(OsString::from("/home/forth"))),
            Some(Path::new("/home/forth").join(HISTORY_FILE))
        );
    }

    #[test]
    fn there_is_no_history_file_without_a_home_directory() {
        assert_eq!(history_path(None), None);
        assert_eq!(history_path(Some(OsString::new())), None);
    }

    #[test]
    fn only_a_missing_file_counts_as_missing() {
        let error = |kind| ReadlineError::Io(io::Error::from(kind));
        assert!(is_missing_file(&error(io::ErrorKind::NotFound)));
        assert!(!is_missing_file(&error(io::ErrorKind::PermissionDenied)));
        assert!(!is_missing_file(&ReadlineError::Eof));
    }

    #[test]
    fn a_line_is_used_as_it_is() {
        assert_eq!(outcome(Ok("1 .".to_string())), Some(Ok("1 .".to_string())));
    }

    #[test]
    fn ctrl_c_asks_for_another_line() {
        assert_eq!(outcome(Err(ReadlineError::Interrupted)), None);
    }

    #[test]
    fn the_end_of_the_input_ends_the_program_successfully() {
        assert_eq!(
            outcome(Err(ReadlineError::Eof)),
            Some(Err(ExitCode::SUCCESS))
        );
    }

    #[test]
    fn a_read_error_ends_the_program_with_a_failure() {
        let error = ReadlineError::Io(io::Error::from(io::ErrorKind::BrokenPipe));
        assert_eq!(outcome(Err(error)), Some(Err(ExitCode::FAILURE)));
    }
}
