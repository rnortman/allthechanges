//! What every fragment kind shares: the [`Kind`] trait, the slug rule, the
//! line reader under every body rule, and the lint problems.

use std::fmt;

use serde::Serialize;

/// A fragment kind: where its fragments live, how their file names and
/// bodies are read.
pub trait Kind {
    /// The default fragment directories, in the order looked for.
    const DIRS: [&'static str; 2];
    /// A parsed file name; its `Ord` is report order.
    type Name: Ord + Clone + fmt::Debug + Eq;
    /// Parses a file name, or says why it is not a fragment of this kind.
    fn parse_name(file_name: &str) -> Result<Self::Name, Finding>;
    /// Checks a fragment's contents and returns its body lines.
    fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding>;
}

/// Why an entry of the fragment directory is not a usable fragment. The
/// serialised names are part of the JSON interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemCode {
    BadName,
    UnknownSection,
    NotAFragment,
    NotUtf8,
    Empty,
    NotAList,
    NotAnEntry,
    Missing,
    Present,
}

impl ProblemCode {
    /// The code as written in reports: `bad-name`, `not-a-list`, ...
    fn as_str(self) -> &'static str {
        match self {
            ProblemCode::BadName => "bad-name",
            ProblemCode::UnknownSection => "unknown-section",
            ProblemCode::NotAFragment => "not-a-fragment",
            ProblemCode::NotUtf8 => "not-utf8",
            ProblemCode::Empty => "empty",
            ProblemCode::NotAList => "not-a-list",
            ProblemCode::NotAnEntry => "not-an-entry",
            ProblemCode::Missing => "missing",
            ProblemCode::Present => "present",
        }
    }
}

impl Serialize for ProblemCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for ProblemCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What is wrong with a name or a body, before it is tied to a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub(crate) code: ProblemCode,
    pub(crate) message: String,
}

impl Finding {
    pub(crate) fn new(code: ProblemCode, message: impl Into<String>) -> Self {
        Finding {
            code,
            message: message.into(),
        }
    }

    /// Ties the finding to the entry it was found in.
    pub(crate) fn at(self, path: impl Into<String>) -> Problem {
        Problem {
            path: path.into(),
            code: self.code,
            message: self.message,
        }
    }
}

/// A lint problem with one entry of the fragment directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub path: String,
    pub code: ProblemCode,
    pub message: String,
}

/// The finding for a file name not ending in exactly `.md`.
pub(crate) fn not_markdown() -> Finding {
    Finding::new(
        ProblemCode::NotAFragment,
        "not a fragment: fragment files end in .md",
    )
}

/// Letters, digits, `-` and `_`; at least one.
pub(crate) fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// A fragment's contents as lines: UTF-8 or `not-utf8`, trailing whitespace
/// trimmed from every line, trailing blank lines dropped, `empty` when
/// nothing is left. A carriage return before a newline is gone; any other
/// is kept for the kind's body rule to reject.
pub(crate) fn lines(bytes: &[u8]) -> Result<Vec<String>, Finding> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Finding::new(ProblemCode::NotUtf8, "not valid UTF-8"))?;
    let mut lines: Vec<String> = text.lines().map(|l| l.trim_end().to_owned()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(Finding::new(ProblemCode::Empty, "no content"));
    }
    Ok(lines)
}

/// The 1-based number of the first line holding a carriage return.
pub(crate) fn carriage_return(lines: &[String]) -> Option<usize> {
    lines.iter().position(|l| l.contains('\r')).map(|i| i + 1)
}

#[cfg(test)]
mod tests;
