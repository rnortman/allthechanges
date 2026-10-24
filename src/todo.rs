//! The TODO kind: fragments `<slug>.md` whose bodies are an entry's
//! paragraphs, folded into the `## Open` section of `TODO.md`, each entry
//! under `### <slug>` at its alphabetical position.
//!
//! Only the inserted entries are written: the title, the preamble, every
//! existing entry and every other section stay as they are.

use std::path::{Path, PathBuf};

use crate::Error;
use crate::fragment::{self, Finding, Kind, Problem, ProblemCode, is_slug};
use crate::scan::Scan;
use crate::target::{self, Refusal};

/// The file a `gather` creates when `TODO.md` does not exist.
const HEADER: &str = "# TODO\n\n## Open\n";

/// The text of the heading entries are gathered under.
const OPEN: &str = "Open";

/// The TODO fragment kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Todo;

impl Kind for Todo {
    const DIRS: [&'static str; 2] = ["todo.d", ".todo.d"];
    type Name = TodoName;

    fn parse_name(file_name: &str) -> Result<TodoName, Finding> {
        TodoName::parse(file_name)
    }

    fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding> {
        parse_body(bytes)
    }
}

/// A parsed TODO fragment file name, `<slug>.md`; ordered by slug.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TodoName {
    pub slug: String,
}

impl TodoName {
    /// Parses `<slug>.md`. A name not ending in exactly `.md` is
    /// `not-a-fragment`; a stem that is not a slug is `bad-name`.
    fn parse(file_name: &str) -> Result<TodoName, Finding> {
        let Some(stem) = file_name.strip_suffix(".md") else {
            return Err(fragment::not_markdown());
        };
        if !is_slug(stem) {
            return Err(Finding::new(
                ProblemCode::BadName,
                "expected <slug>.md, where the slug is letters, digits, '-' and '_'",
            ));
        }
        Ok(TodoName {
            slug: stem.to_owned(),
        })
    }
}

/// Checks a fragment's contents and returns its body lines: trailing
/// whitespace trimmed from every line, trailing blank lines dropped. The
/// first line is not blank; no line outside a fenced code block is an ATX
/// heading; every fence opened is closed. A carriage return anywhere but
/// before a newline is `not-an-entry`.
fn parse_body(bytes: &[u8]) -> Result<Vec<String>, Finding> {
    let lines = fragment::lines(bytes)?;
    if let Some(n) = fragment::carriage_return(&lines) {
        return Err(not_an_entry(n, "contains a carriage return"));
    }
    if lines[0].is_empty() {
        return Err(not_an_entry(1, "an entry starts with text"));
    }
    let mut open_fence = None;
    for (i, line) in lines.iter().enumerate() {
        if target::is_fence(line) {
            open_fence = match open_fence {
                None => Some(i + 1),
                Some(_) => None,
            };
        } else if open_fence.is_none() && is_atx_heading(line) {
            return Err(not_an_entry(
                i + 1,
                "a heading; the entry's heading is its file name",
            ));
        }
    }
    if let Some(n) = open_fence {
        return Err(not_an_entry(n, "code fence not closed"));
    }
    Ok(lines)
}

fn not_an_entry(line: usize, why: &str) -> Finding {
    Finding::new(ProblemCode::NotAnEntry, format!("line {line}: {why}"))
}

/// One to six `#` in column 0, then a space or the end of the line.
fn is_atx_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes) && matches!(line.as_bytes().get(hashes), None | Some(b' '))
}

/// What to gather into.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    pub todo: &'a Path,
    /// Render only: nothing is written or deleted.
    pub dry_run: bool,
}

/// The result of a `gather` that did not fail operationally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing was written. `present` holds a `present` problem for every
    /// fragment whose slug already has an entry.
    Refused {
        refusal: Refusal,
        present: Vec<Problem>,
    },
    /// The entries were rendered and, unless a dry run, inserted and the
    /// fragments deleted.
    Gathered {
        /// `TODO.md` did not exist and was (or would be) created.
        created: bool,
        /// Every entry in slug order, blank-line separated, each exactly as
        /// inserted.
        rendered: String,
        /// The fragment files, in the order written and deleted.
        fragments: Vec<PathBuf>,
    },
}

/// Gathers `scan` into the `## Open` section of the TODO file: refuses on
/// any problem, on no fragments and on a slug that already has an entry;
/// otherwise inserts each entry before the first existing entry whose key
/// is greater, else at the section's end, and, unless a dry run, writes the
/// file atomically and then deletes the fragments in order.
pub fn gather(scan: &Scan<Todo>, dest: &Target<'_>) -> Result<Outcome, Error> {
    if !scan.problems.is_empty() {
        return Ok(refused(Refusal::Problems, Vec::new()));
    }
    if scan.fragments.is_empty() {
        return Ok(refused(Refusal::NothingToGather, Vec::new()));
    }
    let (existing, created) = target::read(dest.todo, HEADER)?;
    let (start, end) = open_section(&existing, dest.todo)?;
    let keys: Vec<target::Heading<'_>> = target::headings(&existing, "### ")
        .into_iter()
        .filter(|h| start < h.line && h.line < end)
        .collect();
    let present: Vec<Problem> = scan
        .fragments
        .iter()
        .filter(|f| keys.iter().any(|k| k.text == f.name.slug))
        .map(|f| {
            Finding::new(
                ProblemCode::Present,
                format!(
                    "{} already has an entry \"### {}\"",
                    dest.todo.display(),
                    f.name.slug
                ),
            )
            .at(f.path.display().to_string())
        })
        .collect();
    if !present.is_empty() {
        return Ok(refused(Refusal::Problems, present));
    }
    let entries: Vec<String> = scan
        .fragments
        .iter()
        .map(|f| render(&f.name.slug, &f.body))
        .collect();
    let mut groups: Vec<(usize, Vec<&str>)> = Vec::new();
    for (fragment, entry) in scan.fragments.iter().zip(&entries) {
        let at = keys
            .iter()
            .find(|k| k.text > fragment.name.slug.as_str())
            .map_or(end, |k| k.line);
        match groups.iter_mut().find(|(line, _)| *line == at) {
            Some((_, group)) => group.push(entry),
            None => groups.push((at, vec![entry])),
        }
    }
    groups.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let mut content = existing;
    for (at, group) in &groups {
        content = target::insert(&content, *at, &group.join("\n"));
    }
    let rendered = entries.join("\n");
    let fragments: Vec<PathBuf> = scan.fragments.iter().map(|f| f.path.clone()).collect();
    if !dest.dry_run {
        target::commit(dest.todo, &content, &fragments)?;
    }
    Ok(Outcome::Gathered {
        created,
        rendered,
        fragments,
    })
}

fn refused(refusal: Refusal, present: Vec<Problem>) -> Outcome {
    Outcome::Refused { refusal, present }
}

/// The line indices of the one `## Open` heading and of the end of its
/// section: the next `# ` or `## ` heading, else the line count. None or several
/// `## Open` headings are an [`Error::Target`].
fn open_section(text: &str, path: &Path) -> Result<(usize, usize), Error> {
    let sections = target::headings(text, "## ");
    let mut open = sections.iter().enumerate().filter(|(_, h)| h.text == OPEN);
    let Some((i, heading)) = open.next() else {
        return Err(Error::Target {
            path: path.to_path_buf(),
            reason: "no \"## Open\" section",
        });
    };
    if open.next().is_some() {
        return Err(Error::Target {
            path: path.to_path_buf(),
            reason: "more than one \"## Open\" section",
        });
    }
    let next_section = sections.get(i + 1).map(|h| h.line);
    let next_part = target::headings(text, "# ")
        .into_iter()
        .map(|h| h.line)
        .find(|&line| line > heading.line);
    let end = [next_section, next_part]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(text.lines().count());
    Ok((heading.line, end))
}

/// An entry: `### <slug>`, a blank line, the body. Ends with one `\n`.
fn render(slug: &str, body: &[String]) -> String {
    let mut out = format!("### {slug}\n\n");
    for line in body {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests;
