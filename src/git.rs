//! Range mode: the fragments added to the directory in a git commit range.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::Error;
use crate::scan::{DEFAULT_DIRS, Scan, scan_added};

/// Lints and lists the fragments added to `dir` in `range`, read from the
/// working tree. git is asked whether or not `dir` exists, so a bad range or
/// a working directory outside a repository is an [`Error`] either way; a
/// fragment added in the range to a directory now absent is `missing`.
/// A `dir` that is not a directory is an [`Error`].
pub fn scan_range(dir: &Path, range: &str) -> Result<Scan, Error> {
    let (base, below) = existing_base(dir)?;
    let names = added_in_range(&base, &below, range)?;
    scan_added(dir, &names, range)
}

/// Range mode when no fragment directory exists: both default names are
/// asked for, so a fragment added under either is `missing`.
pub fn scan_range_defaults(range: &str) -> Result<Scan, Error> {
    let mut scan = Scan::default();
    for name in DEFAULT_DIRS {
        let found = scan_range(Path::new(name), range)?;
        scan.fragments.extend(found.fragments);
        scan.problems.extend(found.problems);
    }
    scan.sort();
    Ok(scan)
}

/// The deepest existing directory at or above `dir`, reached by dropping
/// trailing names, and the path from it down to `dir`. The empty path is
/// the current directory. A `dir` that exists and is not a directory, or an
/// absent `dir` whose last component is not a name (`..`), is an [`Error`].
fn existing_base(dir: &Path) -> Result<(PathBuf, PathBuf), Error> {
    let mut base = dir;
    let probe = loop {
        let probe = if base.as_os_str().is_empty() {
            Path::new(".")
        } else {
            base
        };
        match fs::metadata(probe) {
            Ok(m) if m.is_dir() => break probe,
            Ok(_) => return Err(Error::NotADirectory(dir.to_path_buf())),
            Err(e)
                if e.kind() == io::ErrorKind::NotFound
                    && matches!(base.components().next_back(), Some(Component::Normal(_))) =>
            {
                base = base.parent().expect("a path ending in a name has a parent");
            }
            Err(e) => return Err(Error::io(base)(e)),
        }
    };
    let below = dir
        .strip_prefix(base)
        .expect("base is dir with trailing names dropped");
    Ok((probe.to_path_buf(), below.to_path_buf()))
}

/// The names of the files git reports as added in `range` under
/// `base/below`, relative to it. A git failure is [`Error::Git`] carrying
/// its stderr on one line.
///
/// git runs inside `base` with `--relative`, so the names are relative to it
/// wherever `base` lies in the repository; `--relative=<path>` would take a
/// path from the repository root instead. `--no-renames` makes every path
/// that exists at the end of the range and not at its start an addition,
/// whatever the user's `diff.renames`. `rev-parse --git-dir` runs first
/// because outside a repository `git diff` falls back to `--no-index` and
/// answers with its usage text.
fn added_in_range(base: &Path, below: &Path, range: &str) -> Result<Vec<OsString>, Error> {
    git(base, &["rev-parse".as_ref(), "--git-dir".as_ref()])?;
    let pathspec = if below.as_os_str().is_empty() {
        OsStr::new(".")
    } else {
        below.as_os_str()
    };
    let names = parse_names(git(base, &diff_args(range, pathspec))?);
    Ok(names
        .into_iter()
        .filter_map(|name| {
            let inside = Path::new(&name).strip_prefix(below).ok()?.as_os_str();
            (!inside.is_empty()).then(|| inside.to_owned())
        })
        .collect())
}

/// Runs `git <args>` in `dir` and returns its stdout. Pathspecs are literal,
/// so a directory name beginning with `:` is not read as pathspec magic.
fn git(dir: &Path, args: &[&OsStr]) -> Result<Vec<u8>, Error> {
    let output = Command::new("git")
        .current_dir(dir)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args(args)
        .output()
        .map_err(Error::GitSpawn)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        return Err(Error::Git(if lines.is_empty() {
            format!(
                "git {} failed with {}",
                args[0].to_string_lossy(),
                output.status
            )
        } else {
            lines.join("; ")
        }));
    }
    Ok(output.stdout)
}

/// The `git diff` argument vector listing the files added in `range` under
/// `pathspec`, relative to the current directory, NUL-separated.
/// `--end-of-options` keeps a range that begins with `-` from being read as
/// an option.
fn diff_args<'a>(range: &'a str, pathspec: &'a OsStr) -> Vec<&'a OsStr> {
    [
        "diff",
        "--name-only",
        "--diff-filter=A",
        "--no-renames",
        "-z",
        "--relative",
        "--end-of-options",
        range,
        "--",
    ]
    .into_iter()
    .map(OsStr::new)
    .chain([pathspec])
    .collect()
}

/// Splits `git diff -z --name-only` output into names. git ends every name
/// with a NUL; a name that is not UTF-8 is kept as bytes.
fn parse_names(stdout: Vec<u8>) -> Vec<OsString> {
    stdout
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(os_string)
        .collect()
}

#[cfg(unix)]
fn os_string(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes).to_os_string()
}

#[cfg(not(unix))]
fn os_string(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(test)]
mod tests;
