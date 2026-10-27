//! The TODO kind: fragments `<slug>.md` whose bodies are an entry's
//! paragraphs, folded into `TODO.md`, each entry under `` ## `<slug>` ``
//! directly under the title at its alphabetical position.
//!
//! `TODO.md` is a `# ` title, an optional preamble and one `## <slug>` or
//! `` ## `<slug>` `` heading per entry; any other heading outside a fenced
//! code block is an error. Only the inserted entries are written: the
//! title, the preamble and every existing entry stay as they are.

use std::path::{Path, PathBuf};

use crate::Error;
use crate::fragment::{self, Finding, Kind, Problem, ProblemCode, is_slug};
use crate::scan::Scan;
use crate::target::{self, Refusal};

/// The file a `gather` creates when `TODO.md` does not exist.
const HEADER: &str = "# TODO\n";

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
        } else if open_fence.is_none() && target::atx_level(line).is_some() {
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

/// Gathers `scan` into the TODO file: refuses on any problem, on no
/// fragments and on a slug that already has an entry; otherwise inserts
/// each entry before the first existing entry whose slug sorts after it,
/// else at the end of the file, and, unless a dry run, writes the file
/// atomically and then deletes the fragments in order. A heading that is
/// not the title or an entry is an [`Error::Layout`].
pub fn gather(scan: &Scan<Todo>, dest: &Target<'_>) -> Result<Outcome, Error> {
    if !scan.problems.is_empty() {
        return Ok(refused(Refusal::Problems, Vec::new()));
    }
    if scan.fragments.is_empty() {
        return Ok(refused(Refusal::NothingToGather, Vec::new()));
    }
    let (existing, created) = target::read(dest.todo, HEADER)?;
    let keys = entries(&existing, dest.todo)?;
    let end = existing.lines().count();
    let present: Vec<Problem> = scan
        .fragments
        .iter()
        .filter_map(|f| {
            let key = keys.iter().find(|k| k.slug == f.name.slug)?;
            Some(
                Finding::new(
                    ProblemCode::Present,
                    format!(
                        "{} already has an entry `{}` at line {}",
                        dest.todo.display(),
                        f.name.slug,
                        key.line + 1
                    ),
                )
                .at(f.path.display().to_string()),
            )
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
            .find(|k| k.slug > fragment.name.slug.as_str())
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

/// An entry heading: its line index and its slug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry<'a> {
    line: usize,
    slug: &'a str,
}

/// The entry headings of a TODO file, in file order. Outside fenced code
/// blocks, a `# ` heading before the first entry is a title and a `## `
/// heading whose text, without one pair of surrounding backticks, is a slug
/// is an entry; every other heading, and a bare `## Open`, is an
/// [`Error::Layout`] at its line, as is a fence that is never closed.
fn entries<'a>(text: &'a str, path: &Path) -> Result<Vec<Entry<'a>>, Error> {
    let mut found: Vec<Entry<'a>> = Vec::new();
    for heading in target::atx_headings(text) {
        let reason = match heading.level {
            1 if found.is_empty() => continue,
            1 => "a \"# \" heading after the first entry",
            2 if heading.text == "Open" => {
                "a \"## Open\" section: entries go directly under the title as \"## <slug>\""
            }
            2 => {
                let slug = heading
                    .text
                    .strip_prefix('`')
                    .and_then(|t| t.strip_suffix('`'))
                    .unwrap_or(heading.text);
                if is_slug(slug) {
                    found.push(Entry {
                        line: heading.line,
                        slug,
                    });
                    continue;
                }
                "not an entry heading: expected \"## <slug>\""
            }
            _ => "a heading below level 2: entries have no sub-headings",
        };
        return Err(Error::Layout {
            path: path.to_path_buf(),
            line: heading.line + 1,
            reason,
        });
    }
    if let Some(line) = target::unclosed_fence(text) {
        return Err(Error::Layout {
            path: path.to_path_buf(),
            line: line + 1,
            reason: "a code fence not closed: no heading after it is read",
        });
    }
    Ok(found)
}

/// An entry: `` ## `<slug>` ``, a blank line, the body. Ends with one `\n`.
fn render(slug: &str, body: &[String]) -> String {
    let mut out = format!("## `{slug}`\n\n");
    for line in body {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests;
