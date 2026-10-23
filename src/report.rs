//! The reports the binary prints: the JSON documents of the interface and
//! their human renderings.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::changelog::{Outcome, Refusal, Release};
use crate::fragment::{Problem, Section};
use crate::scan::{DEFAULT_DIRS, Fragment, Scan, by_section};

/// One fragment as reported.
#[derive(Debug, Serialize)]
pub struct FragmentReport<'a> {
    pub path: String,
    pub date: Option<&'a str>,
    pub slug: &'a str,
    pub section: Section,
    pub body: &'a [String],
}

impl<'a> From<&'a Fragment> for FragmentReport<'a> {
    fn from(fragment: &'a Fragment) -> Self {
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
    scan: &'a Scan,
}

impl<'a> QueryReport<'a> {
    pub fn new(dir: Option<&Path>, range: Option<&'a str>, require: bool, scan: &'a Scan) -> Self {
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
        let fragments = count(self.fragments.len(), "fragment");
        let problems = count(self.problems.len(), "problem");
        match (&self.dir, self.range) {
            (None, _) => {
                let _ = writeln!(
                    out,
                    "no {} or {}: {fragments}",
                    DEFAULT_DIRS[0], DEFAULT_DIRS[1]
                );
            }
            (Some(dir), None) => {
                let _ = writeln!(out, "{dir}: {fragments}, {problems}");
            }
            (Some(dir), Some(range)) => {
                let _ = writeln!(out, "{dir}: {fragments} added in {range}, {problems}");
            }
        }
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
        write_problems(&mut out, self.problems);
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
    pub fn new(release: &Release<'a>, scan: &'a Scan, outcome: &'a Outcome) -> Self {
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
                let _ = writeln!(
                    out,
                    "nothing gathered: {} in the fragments",
                    count(self.problems.len(), "problem")
                );
                write_problems(&mut out, self.problems);
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

/// `n` and the noun, plural unless `n` is 1.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The `Problems:` block, one `<path>: <code>: <message>` line each, after a
/// blank line; nothing when there are none.
fn write_problems(out: &mut String, problems: &[Problem]) {
    if problems.is_empty() {
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
