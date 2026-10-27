//! The reports the binary prints: the JSON documents of the interface and
//! their human renderings.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::changelog::{Changelog, Outcome, Release, Section, by_section};
use crate::fragment::{Kind, Problem};
use crate::scan::{Fragment, Scan};
use crate::target::Refusal;
use crate::todo::{self, Todo};

/// One fragment as reported.
#[derive(Debug, Serialize)]
pub struct FragmentReport<'a> {
    pub path: String,
    pub date: Option<&'a str>,
    pub slug: &'a str,
    pub section: Section,
    pub body: &'a [String],
}

impl<'a> From<&'a Fragment<Changelog>> for FragmentReport<'a> {
    fn from(fragment: &'a Fragment<Changelog>) -> Self {
        FragmentReport {
            path: fragment.path.display().to_string(),
            date: fragment.name.date.as_deref(),
            slug: &fragment.name.slug,
            section: fragment.name.section,
            body: &fragment.body,
        }
    }
}

/// The `query` report. Field order is the JSON schema.
#[derive(Debug, Serialize)]
pub struct QueryReport<'a> {
    /// The directory used; `None` when neither default exists.
    pub dir: Option<String>,
    pub range: Option<&'a str>,
    pub fragments: Vec<FragmentReport<'a>>,
    pub problems: &'a [Problem],
    /// No problems and, under `--require`, at least one fragment.
    pub ok: bool,
    #[serde(skip)]
    pub require: bool,
    #[serde(skip)]
    scan: &'a Scan<Changelog>,
}

impl<'a> QueryReport<'a> {
    pub fn new(
        dir: Option<&Path>,
        range: Option<&'a str>,
        require: bool,
        scan: &'a Scan<Changelog>,
    ) -> Self {
        QueryReport {
            dir: dir.map(|d| d.display().to_string()),
            range,
            fragments: scan.fragments.iter().map(FragmentReport::from).collect(),
            problems: &scan.problems,
            ok: scan.problems.is_empty() && !(require && scan.fragments.is_empty()),
            require,
            scan,
        }
    }

    /// The JSON document and its trailing newline.
    pub fn json(&self) -> String {
        to_json(self)
    }

    /// The human report: a header line, each non-empty section with its
    /// fragments' bodies verbatim and every first line tagged with its file
    /// name, then the problems.
    pub fn human(&self) -> String {
        let mut out = String::new();
        write_query_header::<Changelog>(
            &mut out,
            self.dir.as_deref(),
            self.range,
            self.fragments.len(),
            self.problems.len(),
        );
        for (section, in_section) in by_section(&self.scan.fragments) {
            let _ = writeln!(out, "\n{}\n", section.heading());
            for fragment in in_section {
                let file_name = fragment
                    .path
                    .file_name()
                    .expect("a fragment path ends in its file name")
                    .display();
                for (i, line) in fragment.body.iter().enumerate() {
                    if i == 0 {
                        let _ = writeln!(out, "{line}  [{file_name}]");
                    } else {
                        let _ = writeln!(out, "{line}");
                    }
                }
            }
        }
        write_problems(&mut out, self.problems.iter());
        if self.require && self.fragments.is_empty() {
            let _ = match self.range {
                Some(range) => writeln!(out, "\n--require: no fragment added in {range}"),
                None => writeln!(out, "\n--require: no fragment present"),
            };
        }
        out
    }
}

/// The `gather` report. Field order is the JSON schema.
#[derive(Debug, Serialize)]
pub struct GatherReport<'a> {
    pub changelog: String,
    /// The changelog did not exist and was (or would be) created.
    pub created: bool,
    pub version: &'a str,
    pub date: &'a str,
    pub dry_run: bool,
    /// The fragments gathered, deleted unless a dry run; empty on refusal.
    pub gathered: Vec<String>,
    /// The section exactly as inserted; `None` on refusal.
    pub rendered: Option<&'a str>,
    /// The fragment problems; non-empty only when refused for them.
    pub problems: &'a [Problem],
    pub refused: Option<Refusal>,
    /// `true` exactly when `refused` is `None`.
    pub ok: bool,
}

impl<'a> GatherReport<'a> {
    pub fn new(release: &Release<'a>, scan: &'a Scan<Changelog>, outcome: &'a Outcome) -> Self {
        let (created, gathered, rendered, refused) = match outcome {
            Outcome::Refused(refusal) => (false, Vec::new(), None, Some(*refusal)),
            Outcome::Gathered {
                created,
                rendered,
                fragments,
            } => (
                *created,
                fragments.iter().map(|p| p.display().to_string()).collect(),
                Some(rendered.as_str()),
                None,
            ),
        };
        let problems: &[Problem] = if refused == Some(Refusal::Problems) {
            &scan.problems
        } else {
            &[]
        };
        GatherReport {
            changelog: release.changelog.display().to_string(),
            created,
            version: release.version,
            date: release.date,
            dry_run: release.dry_run,
            gathered,
            rendered,
            problems,
            refused,
            ok: refused.is_none(),
        }
    }

    /// The JSON document and its trailing newline.
    pub fn json(&self) -> String {
        to_json(self)
    }

    /// The human report: what was gathered and deleted, the section a dry
    /// run would write, or why nothing was written.
    pub fn human(&self) -> String {
        let mut out = String::new();
        let changelog = &self.changelog;
        match (self.refused, self.rendered) {
            (Some(Refusal::Problems), _) => {
                write_refused_for_problems(&mut out, self.problems.iter());
            }
            (Some(Refusal::NothingToGather), _) => {
                out.push_str("nothing gathered: no fragments to gather\n");
            }
            (Some(Refusal::VersionPresent), _) => {
                let _ = writeln!(
                    out,
                    "nothing gathered: version {} is already in {changelog}",
                    self.version
                );
            }
            (None, Some(rendered)) if self.dry_run => {
                let created = if self.created { " (created)" } else { "" };
                let _ = writeln!(out, "dry run: would write to {changelog}{created}:\n");
                out.push_str(rendered);
            }
            (None, _) => {
                let created = if self.created { " (created)" } else { "" };
                let _ = writeln!(
                    out,
                    "gathered {} into {changelog}{created} as [{}] - {}",
                    count(self.gathered.len(), "fragment"),
                    self.version,
                    self.date
                );
                for path in &self.gathered {
                    let _ = writeln!(out, "deleted {path}");
                }
            }
        }
        out
    }
}

/// One TODO fragment as reported.
#[derive(Debug, Serialize)]
pub struct TodoFragmentReport<'a> {
    pub path: String,
    pub slug: &'a str,
    pub body: &'a [String],
}

impl<'a> From<&'a Fragment<Todo>> for TodoFragmentReport<'a> {
    fn from(fragment: &'a Fragment<Todo>) -> Self {
        TodoFragmentReport {
            path: fragment.path.display().to_string(),
            slug: &fragment.name.slug,
            body: &fragment.body,
        }
    }
}

/// The `todo query` report. Field order is the JSON schema.
#[derive(Debug, Serialize)]
pub struct TodoQueryReport<'a> {
    /// The directory used; `None` when neither default exists.
    pub dir: Option<String>,
    pub range: Option<&'a str>,
    pub fragments: Vec<TodoFragmentReport<'a>>,
    pub problems: &'a [Problem],
    /// No problems.
    pub ok: bool,
}

impl<'a> TodoQueryReport<'a> {
    pub fn new(dir: Option<&Path>, range: Option<&'a str>, scan: &'a Scan<Todo>) -> Self {
        TodoQueryReport {
            dir: dir.map(|d| d.display().to_string()),
            range,
            fragments: scan
                .fragments
                .iter()
                .map(TodoFragmentReport::from)
                .collect(),
            problems: &scan.problems,
            ok: scan.problems.is_empty(),
        }
    }

    /// The JSON document and its trailing newline.
    pub fn json(&self) -> String {
        to_json(self)
    }

    /// The human report: a header line, each fragment as `` ## `<slug>` `` and
    /// its body verbatim, blank-line separated, then the problems.
    pub fn human(&self) -> String {
        let mut out = String::new();
        write_query_header::<Todo>(
            &mut out,
            self.dir.as_deref(),
            self.range,
            self.fragments.len(),
            self.problems.len(),
        );
        for fragment in &self.fragments {
            let _ = writeln!(out, "\n## `{}`\n", fragment.slug);
            for line in fragment.body {
                let _ = writeln!(out, "{line}");
            }
        }
        write_problems(&mut out, self.problems.iter());
        out
    }
}

/// The `todo gather` report. Field order is the JSON schema.
#[derive(Debug, Serialize)]
pub struct TodoGatherReport<'a> {
    pub todo: String,
    /// The TODO file did not exist and was (or would be) created.
    pub created: bool,
    pub dry_run: bool,
    /// The fragments gathered, deleted unless a dry run; empty on refusal.
    pub gathered: Vec<String>,
    /// The entries exactly as inserted; `None` on refusal.
    pub rendered: Option<&'a str>,
    /// The fragment problems, then a `present` problem for every slug that
    /// already has an entry; non-empty only when refused for problems.
    pub problems: Vec<&'a Problem>,
    pub refused: Option<Refusal>,
    /// `true` exactly when `refused` is `None`.
    pub ok: bool,
}

impl<'a> TodoGatherReport<'a> {
    pub fn new(
        target: &todo::Target<'_>,
        scan: &'a Scan<Todo>,
        outcome: &'a todo::Outcome,
    ) -> Self {
        let (created, gathered, rendered, refused, problems) = match outcome {
            todo::Outcome::Refused { refusal, present } => {
                let problems = if *refusal == Refusal::Problems {
                    scan.problems.iter().chain(present).collect()
                } else {
                    Vec::new()
                };
                (false, Vec::new(), None, Some(*refusal), problems)
            }
            todo::Outcome::Gathered {
                created,
                rendered,
                fragments,
            } => (
                *created,
                fragments.iter().map(|p| p.display().to_string()).collect(),
                Some(rendered.as_str()),
                None,
                Vec::new(),
            ),
        };
        TodoGatherReport {
            todo: target.todo.display().to_string(),
            created,
            dry_run: target.dry_run,
            gathered,
            rendered,
            problems,
            refused,
            ok: refused.is_none(),
        }
    }

    /// The JSON document and its trailing newline.
    pub fn json(&self) -> String {
        to_json(self)
    }

    /// The human report: what was gathered and deleted, the entries a dry
    /// run would write, or why nothing was written.
    pub fn human(&self) -> String {
        let mut out = String::new();
        let todo = &self.todo;
        let created = if self.created { " (created)" } else { "" };
        match (self.refused, self.rendered) {
            (Some(Refusal::NothingToGather), _) => {
                out.push_str("nothing gathered: no fragments to gather\n");
            }
            (Some(Refusal::Problems), _) => {
                write_refused_for_problems(&mut out, self.problems.iter().copied());
            }
            (Some(Refusal::VersionPresent), _) => {
                unreachable!("todo::gather never refuses for a version")
            }
            (None, Some(rendered)) if self.dry_run => {
                let _ = writeln!(out, "dry run: would write to {todo}{created}:\n");
                out.push_str(rendered);
            }
            (None, _) => {
                let _ = writeln!(
                    out,
                    "gathered {} into {todo}{created}",
                    count(self.gathered.len(), "fragment"),
                );
                for path in &self.gathered {
                    let _ = writeln!(out, "deleted {path}");
                }
            }
        }
        out
    }
}

/// The first line of a query report: the directory, or that neither of
/// `K::DIRS` exists, with the counts and the range.
fn write_query_header<K: Kind>(
    out: &mut String,
    dir: Option<&str>,
    range: Option<&str>,
    fragments: usize,
    problems: usize,
) {
    let fragments = count(fragments, "fragment");
    let problems = count(problems, "problem");
    let _ = match (dir, range) {
        (None, _) => {
            let [first, second] = K::DIRS;
            writeln!(out, "no {first} or {second}: {fragments}")
        }
        (Some(dir), None) => writeln!(out, "{dir}: {fragments}, {problems}"),
        (Some(dir), Some(range)) => {
            writeln!(out, "{dir}: {fragments} added in {range}, {problems}")
        }
    };
}

/// `n` and the noun, plural unless `n` is 1.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A gather refused for problems: the count, then the `Problems:` block.
fn write_refused_for_problems<'p>(
    out: &mut String,
    problems: impl ExactSizeIterator<Item = &'p Problem>,
) {
    let _ = writeln!(
        out,
        "nothing gathered: {} in the fragments",
        count(problems.len(), "problem")
    );
    write_problems(out, problems);
}

/// The `Problems:` block, one `<path>: <code>: <message>` line each, after a
/// blank line; nothing when there are none.
fn write_problems<'p>(out: &mut String, problems: impl ExactSizeIterator<Item = &'p Problem>) {
    if problems.len() == 0 {
        return;
    }
    out.push_str("\nProblems:\n");
    for p in problems {
        let _ = writeln!(out, "{}: {}: {}", p.path, p.code, p.message);
    }
}

fn to_json(value: &impl Serialize) -> String {
    let mut json = serde_json::to_string_pretty(value).expect("reports serialise");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests;
