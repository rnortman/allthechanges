//! The changelog kind: fragments `[<date>.]<slug>.<section>.md` whose
//! bodies are Markdown list items, folded into `CHANGELOG.md` as one new
//! release section.
//!
//! Only the inserted section is written: the `[Unreleased]` body, earlier
//! releases and link-reference definitions stay as they are.

use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;

use crate::Error;
use crate::fragment::{self, Finding, Kind, ProblemCode, is_slug};
use crate::scan::{Fragment, Scan};
use crate::target::{self, Heading, Refusal};

/// The changelog fragment kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changelog;

impl Kind for Changelog {
    const DIRS: [&'static str; 2] = ["changelog.d", ".changelog.d"];
    type Name = FragmentName;

    fn parse_name(file_name: &str) -> Result<FragmentName, Finding> {
        FragmentName::parse(file_name)
    }

    fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding> {
        parse_body(bytes)
    }
}

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

/// A parsed changelog fragment file name. Ordered by section, then dated
/// before undated, then date, then slug: the report order.
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
            return Err(fragment::not_markdown());
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

    fn key(&self) -> (Section, bool, &Option<String>, &str) {
        (self.section, self.date.is_none(), &self.date, &self.slug)
    }
}

impl Ord for FragmentName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl PartialOrd for FragmentName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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

/// Checks a fragment's contents and returns its body lines: trailing
/// whitespace trimmed from every line, trailing blank lines dropped. The
/// first line starts a list item (`- `); every later line is blank, another
/// item, or indented by at least two spaces. A carriage return anywhere but
/// before a newline is `not-a-list`.
pub(crate) fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding> {
    let lines = fragment::lines(bytes)?;
    if let Some(n) = fragment::carriage_return(&lines) {
        return Err(Finding::new(
            ProblemCode::NotAList,
            format!("line {n}: contains a carriage return"),
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

/// The sections that have fragments, in canonical order, each with its
/// fragments in the order given.
pub(crate) fn by_section(
    fragments: &[Fragment<Changelog>],
) -> impl Iterator<Item = (Section, Vec<&Fragment<Changelog>>)> {
    Section::ALL.into_iter().filter_map(move |section| {
        let in_section: Vec<&Fragment<Changelog>> = fragments
            .iter()
            .filter(|f| f.name.section == section)
            .collect();
        (!in_section.is_empty()).then_some((section, in_section))
    })
}

/// The file a `gather` creates when `CHANGELOG.md` does not exist.
const HEADER: &str = "\
# Changelog

All notable changes to this project are documented in this file. The
format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
";

/// Checks a `--version` value: non-empty, no whitespace, no `[` or `]`.
/// The value is written verbatim.
pub fn parse_version(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("the version is empty".into());
    }
    if value.contains(|c: char| c.is_whitespace() || c == '[' || c == ']') {
        return Err("the version must not contain whitespace, '[' or ']'".into());
    }
    Ok(value.to_owned())
}

/// Checks a `--date` value: `YYYY-MM-DD` naming a real calendar date.
pub fn parse_date(value: &str) -> Result<String, String> {
    let shaped = value.len() == 10
        && value.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !shaped {
        return Err("expected YYYY-MM-DD".into());
    }
    value
        .parse::<jiff::civil::Date>()
        .map_err(|e| format!("not a calendar date: {e}"))?;
    Ok(value.to_owned())
}

/// Today's local date as `YYYY-MM-DD`; UTC when the local time zone cannot
/// be determined.
pub fn today() -> String {
    jiff::Zoned::now().date().strftime("%Y-%m-%d").to_string()
}

/// What to gather into, and as which release.
#[derive(Debug, Clone, Copy)]
pub struct Release<'a> {
    pub changelog: &'a Path,
    pub version: &'a str,
    /// `YYYY-MM-DD`.
    pub date: &'a str,
    /// Render only: nothing is written or deleted.
    pub dry_run: bool,
}

/// The result of a `gather` that did not fail operationally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing was written.
    Refused(Refusal),
    /// The section was rendered and, unless a dry run, inserted and the
    /// fragments deleted.
    Gathered {
        /// The changelog did not exist and was (or would be) created.
        created: bool,
        /// The section exactly as inserted.
        rendered: String,
        /// The fragment files, in the order written and deleted.
        fragments: Vec<PathBuf>,
    },
}

/// Gathers `scan` into the changelog as `release`: refuses on any problem,
/// on no fragments and on a version already present; otherwise renders the
/// section and, unless a dry run, writes the changelog atomically and then
/// deletes the fragments in order.
pub fn gather(scan: &Scan<Changelog>, release: &Release<'_>) -> Result<Outcome, Error> {
    if !scan.problems.is_empty() {
        return Ok(Outcome::Refused(Refusal::Problems));
    }
    if scan.fragments.is_empty() {
        return Ok(Outcome::Refused(Refusal::NothingToGather));
    }
    let (existing, created) = target::read(release.changelog, HEADER)?;
    let headings = target::headings(&existing, "## ");
    if headings
        .iter()
        .any(|h| names_version(h.text, release.version))
    {
        return Ok(Outcome::Refused(Refusal::VersionPresent));
    }
    let rendered = render(release.version, release.date, &scan.fragments);
    let at = insertion_line(&headings, existing.lines().count());
    let content = target::insert(&existing, at, &rendered);
    let fragments: Vec<PathBuf> = scan.fragments.iter().map(|f| f.path.clone()).collect();
    if !release.dry_run {
        target::commit(release.changelog, &content, &fragments)?;
    }
    Ok(Outcome::Gathered {
        created,
        rendered,
        fragments,
    })
}

/// The bracketed text at the start of a heading, and what follows the `]`.
fn bracketed(text: &str) -> Option<(&str, &str)> {
    text.strip_prefix('[')?.split_once(']')
}

/// Whether the heading is `[<version>]`, alone or followed by ` - ...`.
fn names_version(text: &str, version: &str) -> bool {
    bracketed(text).is_some_and(|(inside, rest)| {
        inside == version && (rest.is_empty() || rest.starts_with(" - "))
    })
}

/// The line the new section goes before: the heading after
/// `## [Unreleased]` (or the end) when that heading exists, else the first
/// heading, else the end.
fn insertion_line(headings: &[Heading<'_>], line_count: usize) -> usize {
    let unreleased = headings
        .iter()
        .position(|h| bracketed(h.text).is_some_and(|(inside, _)| inside == "Unreleased"));
    let next = match unreleased {
        Some(i) => i + 1,
        None => 0,
    };
    headings.get(next).map_or(line_count, |h| h.line)
}

/// The release section: `## [<version>] - <date>`, then each section with
/// fragments in canonical order, its heading and every fragment's body in
/// the order given, blank-line separated. Ends with one `\n`.
fn render(version: &str, date: &str, fragments: &[Fragment<Changelog>]) -> String {
    let mut out = format!("## [{version}] - {date}\n");
    for (section, in_section) in by_section(fragments) {
        out.push('\n');
        out.push_str(section.heading());
        out.push_str("\n\n");
        for fragment in in_section {
            for line in &fragment.body {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
