//! The fragment format: file names `[<date>.]<slug>.<section>.md` and bodies
//! that are Markdown list items, copied verbatim.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

/// A Keep a Changelog section. The declaration order is the canonical order
/// sections are listed and written in; it serialises as [`Section::name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    Added,
    Changed,
    Deprecated,
    Removed,
    Fixed,
    Security,
}

impl Section {
    /// Every section, in canonical order.
    pub(crate) const ALL: [Section; 6] = [
        Section::Added,
        Section::Changed,
        Section::Deprecated,
        Section::Removed,
        Section::Fixed,
        Section::Security,
    ];

    /// The name used in fragment file names: `added`, `fixed`, ...
    fn name(self) -> &'static str {
        match self {
            Section::Added => "added",
            Section::Changed => "changed",
            Section::Deprecated => "deprecated",
            Section::Removed => "removed",
            Section::Fixed => "fixed",
            Section::Security => "security",
        }
    }

    /// The Markdown heading the section is written under: `### Added`, ...
    pub(crate) fn heading(self) -> &'static str {
        match self {
            Section::Added => "### Added",
            Section::Changed => "### Changed",
            Section::Deprecated => "### Deprecated",
            Section::Removed => "### Removed",
            Section::Fixed => "### Fixed",
            Section::Security => "### Security",
        }
    }
}

impl Serialize for Section {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The error of [`Section::from_str`]: the text is not a section name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownSection(pub String);

impl fmt::Display for UnknownSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected: Vec<&str> = Section::ALL.iter().map(|s| s.name()).collect();
        write!(
            f,
            "\"{}\" is not a section; expected one of {}",
            self.0,
            expected.join(", ")
        )
    }
}

impl FromStr for Section {
    type Err = UnknownSection;

    /// Exact, lowercase names only: `Fixed` is not `fixed`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Section::ALL
            .into_iter()
            .find(|section| section.name() == s)
            .ok_or_else(|| UnknownSection(s.to_owned()))
    }
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
    Missing,
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
            ProblemCode::Missing => "missing",
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
pub(crate) struct Finding {
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

/// A parsed fragment file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentName {
    /// `YYYY-MM-DD` when the name starts with one; shape-checked only.
    pub date: Option<String>,
    pub slug: String,
    pub section: Section,
}

impl FragmentName {
    /// Parses `[<date>.]<slug>.<section>.md`. A name not ending in exactly
    /// `.md` is `not-a-fragment`; a section that is not one of the six is
    /// `unknown-section`; every other mismatch is `bad-name`.
    pub(crate) fn parse(file_name: &str) -> Result<FragmentName, Finding> {
        let Some(stem) = file_name.strip_suffix(".md") else {
            return Err(Finding::new(
                ProblemCode::NotAFragment,
                "not a fragment: fragment files end in .md",
            ));
        };
        let parts: Vec<&str> = stem.split('.').collect();
        let (date, slug, section) = match parts.as_slice() {
            [slug, section] => (None, *slug, *section),
            [date, slug, section] if is_date_shape(date) => (Some(*date), *slug, *section),
            _ => return Err(bad_name()),
        };
        if !is_slug(slug) {
            return Err(bad_name());
        }
        let section = section
            .parse::<Section>()
            .map_err(|e| Finding::new(ProblemCode::UnknownSection, e.to_string()))?;
        Ok(FragmentName {
            date: date.map(str::to_owned),
            slug: slug.to_owned(),
            section,
        })
    }
}

fn bad_name() -> Finding {
    Finding::new(
        ProblemCode::BadName,
        "expected [<YYYY-MM-DD>.]<slug>.<section>.md, where the slug is letters, digits, '-' and '_'",
    )
}

/// `YYYY-MM-DD` in shape: four digits, hyphen, two digits, hyphen, two digits.
fn is_date_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// Checks a fragment's contents and returns its body lines: trailing
/// whitespace trimmed from every line, trailing blank lines dropped. The
/// first line starts a list item (`- `); every later line is blank, another
/// item, or indented by at least two spaces. A carriage return anywhere but
/// before a newline is `not-a-list`.
pub(crate) fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Finding::new(ProblemCode::NotUtf8, "not valid UTF-8"))?;
    let mut lines: Vec<String> = text.lines().map(|l| l.trim_end().to_owned()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(Finding::new(ProblemCode::Empty, "no content"));
    }
    if let Some(i) = lines.iter().position(|l| l.contains('\r')) {
        return Err(Finding::new(
            ProblemCode::NotAList,
            format!("line {}: contains a carriage return", i + 1),
        ));
    }
    if !lines[0].starts_with("- ") {
        return Err(Finding::new(
            ProblemCode::NotAList,
            "line 1: a fragment starts with a \"- \" list item",
        ));
    }
    for (i, line) in lines.iter().enumerate().skip(1) {
        if !(line.is_empty() || line.starts_with("- ") || line.starts_with("  ")) {
            return Err(Finding::new(
                ProblemCode::NotAList,
                format!(
                    "line {}: expected a \"- \" list item, a blank line or a line indented by two spaces",
                    i + 1
                ),
            ));
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests;
