//! The Markdown file a `gather` writes into: reading it, finding its
//! headings, inserting text, writing it atomically and deleting the
//! fragments gathered into it.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Error;

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

/// The file's text and whether it is to be created: `header` when the file
/// does not exist. Anything but a regular file at the path (a symlink
/// included, which the rename would replace rather than update), CRLF line
/// endings and invalid UTF-8 are errors.
pub(crate) fn read(path: &Path, header: &str) -> Result<(String, bool), Error> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => {}
        Ok(_) => {
            return Err(Error::Target {
                path: path.to_path_buf(),
                reason: "not a regular file",
            });
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((header.to_owned(), true)),
        Err(e) => return Err(Error::io(path)(e)),
    }
    let bytes = fs::read(path).map_err(Error::io(path))?;
    let text = String::from_utf8(bytes).map_err(|_| Error::Target {
        path: path.to_path_buf(),
        reason: "not valid UTF-8",
    })?;
    if text.contains('\r') {
        return Err(Error::Target {
            path: path.to_path_buf(),
            reason: "CRLF line endings are not supported",
        });
    }
    Ok((text, false))
}

/// A heading: its line index and the text after the prefix, trailing
/// whitespace trimmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Heading<'a> {
    pub(crate) line: usize,
    pub(crate) text: &'a str,
}

/// Whether the line opens or closes a fenced code block: it starts with
/// three backticks or three tildes.
pub(crate) fn is_fence(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

/// Every line starting with `prefix` (`"## "`, `"### "`) outside fenced
/// code blocks. Every fence line opens or closes a fence.
pub(crate) fn headings<'a>(text: &'a str, prefix: &str) -> Vec<Heading<'a>> {
    let mut fenced = false;
    let mut found = Vec::new();
    for (line, content) in text.lines().enumerate() {
        if is_fence(content) {
            fenced = !fenced;
        } else if !fenced && let Some(rest) = content.strip_prefix(prefix) {
            found.push(Heading {
                line,
                text: rest.trim_end(),
            });
        }
    }
    found
}

/// `text` with `section` inserted before line `at`, exactly one blank line
/// on either side of it, and a single `\n` at the end of the file.
pub(crate) fn insert(text: &str, at: usize, section: &str) -> String {
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

/// Writes `content` to `path` atomically, then deletes the fragments
/// gathered into it in order. Nothing is deleted unless the write succeeded.
pub(crate) fn commit(path: &Path, content: &str, fragments: &[PathBuf]) -> Result<(), Error> {
    write_atomically(path, content)?;
    delete(path, fragments)
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

/// Deletes the fragments gathered into `target` in order; the first failure
/// stops it and names what was and was not deleted.
fn delete(target: &Path, fragments: &[PathBuf]) -> Result<(), Error> {
    for (i, path) in fragments.iter().enumerate() {
        if let Err(source) = fs::remove_file(path) {
            return Err(Error::Delete {
                target: target.to_path_buf(),
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
