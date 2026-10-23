//! Changelog fragments: one file per change under `changelog.d/`, linted and
//! listed by `query`, folded into `CHANGELOG.md` by `gather`.
//!
//! Every behaviour lives in this library; the `allthechanges` binary parses
//! arguments, calls it and maps outcomes to exit codes.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub mod changelog;
pub mod fragment;
pub mod git;
pub mod report;
pub mod scan;

/// An operational failure: the environment, not a fragment, is at fault.
/// The binary reports it on stderr and exits 3.
#[derive(Debug)]
pub enum Error {
    /// Reading or writing `path` failed for a reason other than its absence.
    Io { path: PathBuf, source: io::Error },
    /// The fragment directory named exists but is not a directory.
    NotADirectory(PathBuf),
    /// Both default fragment directories exist and no `--dir` chose one.
    BothDirectories,
    /// `git` could not be started.
    GitSpawn(io::Error),
    /// `git` ran and failed; its trimmed stderr.
    Git(String),
    /// The changelog cannot be read as a changelog.
    Changelog { path: PathBuf, reason: &'static str },
    /// The changelog was written, then deleting a gathered fragment failed.
    Delete {
        changelog: PathBuf,
        deleted: Vec<PathBuf>,
        /// The fragment that failed first, then the ones not attempted.
        remaining: Vec<PathBuf>,
        source: io::Error,
    },
}

impl Error {
    /// Wraps an I/O failure on `path`, for `map_err`.
    pub fn io(path: &Path) -> impl FnOnce(io::Error) -> Error {
        move |source| Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::NotADirectory(path) => write!(f, "{}: not a directory", path.display()),
            Error::BothDirectories => {
                f.write_str("both changelog.d and .changelog.d exist; pass --dir")
            }
            Error::GitSpawn(source) => write!(f, "cannot run git: {source}"),
            Error::Git(message) => f.write_str(message),
            Error::Changelog { path, reason } => write!(f, "{}: {reason}", path.display()),
            Error::Delete {
                changelog,
                deleted,
                remaining,
                source,
            } => write!(
                f,
                "{} holds the gathered section, but deleting {} failed: {source}; \
                 deleted: {}; delete by hand: {}",
                changelog.display(),
                remaining[0].display(),
                list(deleted),
                list(remaining),
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } | Error::GitSpawn(source) | Error::Delete { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

/// The paths, comma-separated; `none` when there are none.
fn list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "none".into();
    }
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
