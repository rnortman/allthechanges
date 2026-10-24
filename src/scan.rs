//! Finding the fragment directory and reading every fragment in it.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::fragment::{Finding, Kind, Problem, ProblemCode};

/// Chooses the fragment directory. An explicit `--dir` is returned as given,
/// whether or not it exists. Otherwise the one of `K::DIRS` that exists
/// under `base`, joined to `base`; `None` when neither does. A directory
/// that is a symlink is followed.
// TODO(security-symlinked-fragment-directory-read-and-deleted): refuse a
// default fragment directory that is a symlink.
pub fn locate<K: Kind>(base: &Path, explicit: Option<&Path>) -> Result<Option<PathBuf>, Error> {
    if let Some(dir) = explicit {
        return Ok(Some(dir.to_path_buf()));
    }
    let mut found = None;
    for name in K::DIRS {
        let path = base.join(name);
        if path.try_exists().map_err(Error::io(&path))? {
            if found.is_some() {
                return Err(Error::BothDirectories(K::DIRS));
            }
            found = Some(path);
        }
    }
    Ok(found)
}

/// A fragment read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment<K: Kind> {
    /// The fragment directory joined with the file name.
    pub path: PathBuf,
    pub name: K::Name,
    pub body: Vec<String>,
}

/// Every fragment and every problem found, each in report order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan<K: Kind> {
    pub fragments: Vec<Fragment<K>>,
    pub problems: Vec<Problem>,
}

impl<K: Kind> Default for Scan<K> {
    fn default() -> Self {
        Scan {
            fragments: Vec::new(),
            problems: Vec::new(),
        }
    }
}

impl<K: Kind> Scan<K> {
    /// Puts fragments and problems in report order.
    pub(crate) fn sort(&mut self) {
        self.fragments.sort_by(|a, b| a.name.cmp(&b.name));
        self.problems.sort_by(|a, b| a.path.cmp(&b.path));
    }
}

/// Reads every entry of `dir`. A directory that does not exist holds no
/// fragments; one that is not a directory, or any I/O failure other than
/// absence, is an [`Error`]. Names beginning with `.` are ignored; a symlink
/// is `not-a-fragment`.
pub fn scan<K: Kind>(dir: &Path) -> Result<Scan<K>, Error> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Scan::default()),
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => {
            return Err(Error::NotADirectory(dir.to_path_buf()));
        }
        Err(e) => return Err(Error::io(dir)(e)),
    };
    let mut scan = Scan::default();
    for entry in entries {
        let file_name = entry.map_err(Error::io(dir))?.file_name();
        classify(&mut scan, dir, &file_name, || {
            Finding::new(
                ProblemCode::NotAFragment,
                "not a fragment: removed while scanning",
            )
        })?;
    }
    scan.sort();
    Ok(scan)
}

/// Treats `names`, the files git reports as added to `dir` in `range` and
/// relative to it, as the directory listing; a name absent from the working
/// tree is `missing`.
pub(crate) fn scan_added<K: Kind, N: AsRef<OsStr>>(
    dir: &Path,
    names: &[N],
    range: &str,
) -> Result<Scan<K>, Error> {
    let mut scan = Scan::default();
    for name in names {
        classify(&mut scan, dir, name.as_ref(), || {
            Finding::new(
                ProblemCode::Missing,
                format!("added in {range} but not present in the working tree"),
            )
        })?;
    }
    scan.sort();
    Ok(scan)
}

/// Classifies the listed entry `name` of `dir`: ignored when it begins with
/// `.`; `not-a-fragment` when its name is not UTF-8, it lies inside a
/// subdirectory, it is a symlink (never read, whatever it resolves to) or it
/// is not a regular file; the `absent` finding when nothing is at its path;
/// otherwise read as a fragment.
fn classify<K: Kind>(
    scan: &mut Scan<K>,
    dir: &Path,
    name: &OsStr,
    absent: impl FnOnce() -> Finding,
) -> Result<(), Error> {
    if name.as_encoded_bytes().first() == Some(&b'.') {
        return Ok(());
    }
    let path = dir.join(name);
    let display = path.display().to_string();
    let Some(name) = name.to_str() else {
        return not_a_fragment(scan, display, "file name is not valid UTF-8");
    };
    if name.contains('/') {
        return not_a_fragment(scan, display, "inside a subdirectory");
    }
    match fs::symlink_metadata(&path) {
        Ok(m) if m.is_symlink() => not_a_fragment(scan, display, "a symlink"),
        Ok(m) if m.is_file() => read_fragment(scan, path, name, display),
        Ok(_) => not_a_fragment(scan, display, "not a regular file"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            scan.problems.push(absent().at(display));
            Ok(())
        }
        Err(e) => Err(Error::io(&path)(e)),
    }
}

fn not_a_fragment<K: Kind>(scan: &mut Scan<K>, display: String, why: &str) -> Result<(), Error> {
    scan.problems.push(
        Finding::new(ProblemCode::NotAFragment, format!("not a fragment: {why}")).at(display),
    );
    Ok(())
}

/// Parses the name of the regular file at `path`, then its contents, and
/// records the fragment or the first problem found.
fn read_fragment<K: Kind>(
    scan: &mut Scan<K>,
    path: PathBuf,
    file_name: &str,
    display: String,
) -> Result<(), Error> {
    let name = match K::parse_name(file_name) {
        Ok(name) => name,
        Err(finding) => {
            scan.problems.push(finding.at(display));
            return Ok(());
        }
    };
    let bytes = fs::read(&path).map_err(Error::io(&path))?;
    match K::parse_body(&bytes) {
        Ok(body) => scan.fragments.push(Fragment { path, name, body }),
        Err(finding) => scan.problems.push(finding.at(display)),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
