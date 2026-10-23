//! Folding the fragments into `CHANGELOG.md` as one new release section.
//!
//! Only the inserted section is written: the `[Unreleased]` body, earlier
//! releases and link-reference definitions stay as they are.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Error;
use crate::scan::{Fragment, Scan, by_section};

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

/// Why `gather` wrote nothing. Serialised as the interface's `refused`
/// values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The fragments have lint problems.
    Problems,
    /// No fragment directory, or no fragments in it.
    NothingToGather,
    /// The changelog already has a section for the version.
    VersionPresent,
}

impl Refusal {
    fn as_str(self) -> &'static str {
        match self {
            Refusal::Problems => "problems",
            Refusal::NothingToGather => "nothing-to-gather",
            Refusal::VersionPresent => "version-present",
        }
    }
}

impl Serialize for Refusal {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
pub fn gather(scan: &Scan, release: &Release<'_>) -> Result<Outcome, Error> {
    if !scan.problems.is_empty() {
        return Ok(Outcome::Refused(Refusal::Problems));
    }
    if scan.fragments.is_empty() {
        return Ok(Outcome::Refused(Refusal::NothingToGather));
    }
    let (existing, created) = read(release.changelog)?;
    let headings = headings(&existing);
    if headings
        .iter()
        .any(|h| names_version(h.text, release.version))
    {
        return Ok(Outcome::Refused(Refusal::VersionPresent));
    }
    let rendered = render(release.version, release.date, &scan.fragments);
    let at = insertion_line(&headings, existing.lines().count());
    let content = insert(&existing, at, &rendered);
    let fragments: Vec<PathBuf> = scan.fragments.iter().map(|f| f.path.clone()).collect();
    if !release.dry_run {
        write_atomically(release.changelog, &content)?;
        delete(release.changelog, &fragments)?;
    }
    Ok(Outcome::Gathered {
        created,
        rendered,
        fragments,
    })
}

/// The changelog's text and whether it is to be created: [`HEADER`] when
/// the file does not exist. Anything but a regular file at the path (a
/// symlink included, which the rename would replace rather than update),
/// CRLF line endings and invalid UTF-8 are errors.
fn read(path: &Path) -> Result<(String, bool), Error> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => {}
        Ok(_) => {
            return Err(Error::Changelog {
                path: path.to_path_buf(),
                reason: "not a regular file",
            });
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((HEADER.to_owned(), true)),
        Err(e) => return Err(Error::io(path)(e)),
    }
    let bytes = fs::read(path).map_err(Error::io(path))?;
    let text = String::from_utf8(bytes).map_err(|_| Error::Changelog {
        path: path.to_path_buf(),
        reason: "not valid UTF-8",
    })?;
    if text.contains('\r') {
        return Err(Error::Changelog {
            path: path.to_path_buf(),
            reason: "CRLF line endings are not supported",
        });
    }
    Ok((text, false))
}

/// A `## ` heading: its line index and the text after `## `, trailing
/// whitespace trimmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Heading<'a> {
    line: usize,
    text: &'a str,
}

/// Every `## ` heading outside fenced code blocks. A line starting with
/// three backticks or three tildes opens or closes a fence.
fn headings(text: &str) -> Vec<Heading<'_>> {
    let mut fenced = false;
    let mut found = Vec::new();
    for (line, content) in text.lines().enumerate() {
        if content.starts_with("```") || content.starts_with("~~~") {
            fenced = !fenced;
        } else if !fenced && let Some(rest) = content.strip_prefix("## ") {
            found.push(Heading {
                line,
                text: rest.trim_end(),
            });
        }
    }
    found
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
fn render(version: &str, date: &str, fragments: &[Fragment]) -> String {
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

/// `text` with `section` inserted before line `at`, exactly one blank line
/// on either side of it, and a single `\n` at the end of the file.
fn insert(text: &str, at: usize, section: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let (before, after) = lines.split_at(at);
    let before = trim_blank(before, false);
    let after = trim_blank(after, true);
    let mut out = String::with_capacity(text.len() + section.len() + 2);
    for line in before {
        out.push_str(line);
        out.push('\n');
    }
    if !before.is_empty() {
        out.push('\n');
    }
    out.push_str(section);
    if !after.is_empty() {
        out.push('\n');
    }
    for line in after {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// `lines` without trailing blank lines, and without leading ones too when
/// `leading`.
fn trim_blank<'a, 'b>(mut lines: &'a [&'b str], leading: bool) -> &'a [&'b str] {
    while let [rest @ .., last] = lines
        && last.trim().is_empty()
    {
        lines = rest;
    }
    while leading
        && let [first, rest @ ..] = lines
        && first.trim().is_empty()
    {
        lines = rest;
    }
    lines
}

/// Writes `content` to `<path>.tmp` beside `path`, then renames it over
/// `path`. The temporary file is created new: anything already at its path,
/// a symlink included, is an error and is left untouched.
fn write_atomically(path: &Path, content: &str) -> Result<(), Error> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .map_err(Error::io(&tmp))?;
    fs::rename(&tmp, path).map_err(Error::io(path))
}

/// Deletes the gathered fragments in order; the first failure stops it and
/// names what was and was not deleted.
fn delete(changelog: &Path, fragments: &[PathBuf]) -> Result<(), Error> {
    for (i, path) in fragments.iter().enumerate() {
        if let Err(source) = fs::remove_file(path) {
            return Err(Error::Delete {
                changelog: changelog.to_path_buf(),
                deleted: fragments[..i].to_vec(),
                remaining: fragments[i..].to_vec(),
                source,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
