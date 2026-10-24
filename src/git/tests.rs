use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use super::*;
use crate::changelog::Changelog;
use crate::fragment::ProblemCode;
use crate::todo::Todo;

#[test]
fn names_split_on_nul() {
    assert_eq!(
        parse_names(b"a.added.md\0x/b.fixed.md\0.gitkeep\0".to_vec()),
        vec![
            OsString::from("a.added.md"),
            OsString::from("x/b.fixed.md"),
            OsString::from(".gitkeep"),
        ]
    );
}

#[test]
fn no_output_is_no_names() {
    assert!(parse_names(Vec::new()).is_empty());
}

#[test]
fn names_keep_spaces_newlines_and_non_utf8_bytes() {
    assert_eq!(
        parse_names(b"with space.added.md\0new\nline.md\0caf\xe9.added.md\0".to_vec()),
        vec![
            OsString::from("with space.added.md"),
            OsString::from("new\nline.md"),
            OsString::from_vec(b"caf\xe9.added.md".to_vec()),
        ]
    );
}

#[test]
fn parsed_names_classify_like_a_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("changelog.d");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("a.added.md"), "- A.\n").unwrap();
    let names = parse_names(b"a.added.md\0sub/b.fixed.md\0.gitkeep\0caf\xe9.added.md\0".to_vec());
    let scan = scan_added::<Changelog, _>(&dir, &names, "A..B").unwrap();
    assert_eq!(scan.fragments.len(), 1);
    assert_eq!(scan.fragments[0].path, dir.join("a.added.md"));
    let codes: Vec<ProblemCode> = scan.problems.iter().map(|p| p.code).collect();
    assert_eq!(
        codes,
        [ProblemCode::NotAFragment, ProblemCode::NotAFragment]
    );
}

#[test]
fn diff_lists_only_additions_without_rename_pairing() {
    let args = diff_args("origin/main...HEAD", OsStr::new("."));
    assert_eq!(args[0], "diff");
    for flag in [
        "--name-only",
        "--diff-filter=A",
        "--no-renames",
        "-z",
        "--relative",
    ] {
        assert!(
            args.contains(&OsStr::new(flag)),
            "{flag} missing from {args:?}"
        );
    }
}

#[test]
fn range_is_passed_verbatim_after_end_of_options() {
    let args = diff_args("--output=x", OsStr::new("changelog.d"));
    let end = args.iter().position(|a| *a == "--end-of-options").unwrap();
    assert_eq!(args[end + 1], "--output=x");
    assert_eq!(&args[end + 2..], ["--", "changelog.d"]);
}

#[test]
fn existing_directory_is_its_own_base() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("changelog.d");
    fs::create_dir(&dir).unwrap();
    assert_eq!(existing_base(&dir).unwrap(), (dir, PathBuf::new()));
}

#[test]
fn absent_directory_is_reached_from_its_deepest_existing_ancestor() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        existing_base(&tmp.path().join("pkg/changelog.d")).unwrap(),
        (tmp.path().to_path_buf(), PathBuf::from("pkg/changelog.d"))
    );
    assert_eq!(
        existing_base(Path::new("no-such-dir")).unwrap(),
        (PathBuf::from("."), PathBuf::from("no-such-dir"))
    );
}

#[test]
fn absent_path_ending_in_parent_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("nope/..");
    assert!(matches!(existing_base(&dir), Err(Error::Io { path, .. }) if path == dir));
}

#[test]
fn file_in_place_of_the_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("changelog.d");
    fs::write(&dir, "").unwrap();
    assert!(matches!(
        scan_range::<Changelog>(&dir, "A..B"),
        Err(Error::NotADirectory(path)) if path == dir
    ));
}

#[test]
fn todo_names_added_but_absent_are_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("todo.d");
    let names = [OsStr::new("gone.md"), OsStr::new("bad name.md")];
    let scan = scan_added::<Todo, _>(&dir, &names, "A..B").unwrap();
    assert!(scan.fragments.is_empty());
    let problems: Vec<(PathBuf, ProblemCode)> = scan
        .problems
        .iter()
        .map(|p| (PathBuf::from(&p.path), p.code))
        .collect();
    assert_eq!(
        problems,
        [
            (dir.join("bad name.md"), ProblemCode::Missing),
            (dir.join("gone.md"), ProblemCode::Missing),
        ]
    );
}
