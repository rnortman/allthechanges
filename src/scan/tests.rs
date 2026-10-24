use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::*;
use crate::changelog::{Changelog, FragmentName, Section};
use crate::todo::Todo;

fn write(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap();
}

fn paths<K: Kind>(scan: &Scan<K>) -> Vec<String> {
    scan.fragments
        .iter()
        .map(|f| f.path.display().to_string())
        .collect()
}

fn problems<K: Kind>(scan: &Scan<K>) -> Vec<(String, ProblemCode)> {
    scan.problems
        .iter()
        .map(|p| (p.path.clone(), p.code))
        .collect()
}

/// A tempdir holding `changelog.d`, and that directory's path, displayed
/// with the tempdir's own prefix stripped by the tests that need it.
fn fragment_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("changelog.d");
    fs::create_dir(&dir).unwrap();
    (tmp, dir)
}

fn rel(dir: &Path, name: &str) -> String {
    dir.join(name).display().to_string()
}

#[test]
fn order_is_section_then_dated_then_undated() {
    let (_tmp, dir) = fragment_dir();
    for name in [
        "zeta.added.md",
        "alpha.added.md",
        "2026-02-01.beta.added.md",
        "2026-01-15.zulu.added.md",
        "2026-01-15.alpha.added.md",
        "first.security.md",
        "mid.fixed.md",
        "2025-12-31.early.changed.md",
    ] {
        write(&dir, name, "- item\n");
    }
    let scan = scan::<Changelog>(&dir).unwrap();
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    let expected: Vec<String> = [
        "2026-01-15.alpha.added.md",
        "2026-01-15.zulu.added.md",
        "2026-02-01.beta.added.md",
        "alpha.added.md",
        "zeta.added.md",
        "2025-12-31.early.changed.md",
        "mid.fixed.md",
        "first.security.md",
    ]
    .iter()
    .map(|n| rel(&dir, n))
    .collect();
    assert_eq!(paths(&scan), expected);
}

#[test]
fn fragment_carries_name_and_body() {
    let (_tmp, dir) = fragment_dir();
    write(
        &dir,
        "2026-10-23.login.fixed.md",
        "- Fix login.\n  More.\n\n",
    );
    let scan = scan::<Changelog>(&dir).unwrap();
    assert_eq!(
        scan.fragments,
        [Fragment {
            path: dir.join("2026-10-23.login.fixed.md"),
            name: FragmentName {
                date: Some("2026-10-23".into()),
                slug: "login".into(),
                section: Section::Fixed,
            },
            body: vec!["- Fix login.".into(), "  More.".into()],
        }]
    );
}

#[test]
fn dotfiles_are_ignored() {
    let (_tmp, dir) = fragment_dir();
    write(&dir, ".gitkeep", "");
    write(&dir, ".notes.txt", "anything");
    fs::create_dir(dir.join(".hidden")).unwrap();
    fs::write(dir.join(OsStr::from_bytes(b".\xff.swp")), "").unwrap();
    let scan = scan::<Changelog>(&dir).unwrap();
    assert_eq!(scan, Scan::default());
}

#[test]
fn every_other_entry_is_a_fragment_or_a_problem() {
    let (_tmp, dir) = fragment_dir();
    fs::create_dir(dir.join("sub.added.md")).unwrap();
    write(&dir, "notes.txt", "- x\n");
    write(&dir, "README.md", "- x\n");
    write(&dir, "typo.fxied.md", "- x\n");
    write(&dir, "prose.added.md", "Prose.\n");
    write(&dir, "blank.added.md", "\n");
    fs::write(dir.join("latin1.added.md"), b"- caf\xe9\n").unwrap();
    write(&dir, "good.added.md", "- x\n");
    let scan = scan::<Changelog>(&dir).unwrap();
    assert_eq!(paths(&scan), [rel(&dir, "good.added.md")]);
    assert_eq!(
        problems(&scan),
        [
            (rel(&dir, "README.md"), ProblemCode::BadName),
            (rel(&dir, "blank.added.md"), ProblemCode::Empty),
            (rel(&dir, "latin1.added.md"), ProblemCode::NotUtf8),
            (rel(&dir, "notes.txt"), ProblemCode::NotAFragment),
            (rel(&dir, "prose.added.md"), ProblemCode::NotAList),
            (rel(&dir, "sub.added.md"), ProblemCode::NotAFragment),
            (rel(&dir, "typo.fxied.md"), ProblemCode::UnknownSection),
        ]
    );
}

#[test]
fn absent_directory_holds_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        scan::<Changelog>(&tmp.path().join("nope")).unwrap(),
        Scan::default()
    );
}

#[test]
fn a_file_is_not_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("changelog.d");
    fs::write(&file, "").unwrap();
    assert!(matches!(scan::<Changelog>(&file), Err(Error::NotADirectory(p)) if p == file));
}

#[test]
fn unreadable_fragment_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, dir) = fragment_dir();
    let file = dir.join("locked.added.md");
    fs::write(&file, "- x\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(
        fs::read(&file).is_err(),
        "a mode-000 file is readable: the tests run with privileges that ignore file modes"
    );
    let err = scan::<Changelog>(&dir).unwrap_err();
    assert!(
        matches!(&err, Error::Io { path, .. } if *path == file),
        "{err:?}"
    );
}

#[test]
fn unresolvable_symlinks_are_not_fragments() {
    let (_tmp, dir) = fragment_dir();
    write(&dir, "plain", "");
    symlink(dir.join("gone"), dir.join("dangling.added.md")).unwrap();
    symlink("loop.added.md", dir.join("loop.added.md")).unwrap();
    symlink(dir.join("plain/child"), dir.join("through.added.md")).unwrap();
    let scan = scan::<Changelog>(&dir).unwrap();
    assert!(scan.fragments.is_empty());
    let expected: Vec<(String, ProblemCode)> = [
        "dangling.added.md",
        "loop.added.md",
        "plain",
        "through.added.md",
    ]
    .iter()
    .map(|n| (rel(&dir, n), ProblemCode::NotAFragment))
    .collect();
    assert_eq!(problems(&scan), expected);
    for p in [&scan.problems[0], &scan.problems[1], &scan.problems[3]] {
        assert_eq!(p.message, "not a fragment: a symlink");
    }
}

#[test]
fn symlink_to_a_fragment_is_not_a_fragment() {
    let (tmp, dir) = fragment_dir();
    write(tmp.path(), "target.md", "- secret\n");
    symlink(tmp.path().join("target.md"), dir.join("linked.added.md")).unwrap();
    let scan = scan::<Changelog>(&dir).unwrap();
    assert!(scan.fragments.is_empty(), "{:?}", scan.fragments);
    assert_eq!(
        problems(&scan),
        [(rel(&dir, "linked.added.md"), ProblemCode::NotAFragment)]
    );
    assert_eq!(scan.problems[0].message, "not a fragment: a symlink");
    assert!(!format!("{scan:?}").contains("secret"));
}

#[test]
fn non_utf8_name_is_not_a_fragment() {
    let (_tmp, dir) = fragment_dir();
    let name = OsStr::from_bytes(b"caf\xe9.added.md");
    fs::write(dir.join(name), "- x\n").unwrap();
    let scan = scan::<Changelog>(&dir).unwrap();
    assert!(scan.fragments.is_empty());
    assert_eq!(
        problems(&scan),
        [(
            dir.join(name).display().to_string(),
            ProblemCode::NotAFragment
        )]
    );
    assert_eq!(
        scan.problems[0].message,
        "not a fragment: file name is not valid UTF-8"
    );
}

#[test]
fn locate_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    assert_eq!(locate::<Changelog>(base, None).unwrap(), None);
    fs::create_dir(base.join(".changelog.d")).unwrap();
    assert_eq!(
        locate::<Changelog>(base, None).unwrap(),
        Some(base.join(".changelog.d"))
    );
    fs::create_dir(base.join("changelog.d")).unwrap();
    assert!(matches!(
        locate::<Changelog>(base, None),
        Err(Error::BothDirectories(_))
    ));
    assert_eq!(
        locate::<Changelog>(base, Some(Path::new("changelog.d"))).unwrap(),
        Some(PathBuf::from("changelog.d"))
    );
    fs::remove_dir(base.join(".changelog.d")).unwrap();
    assert_eq!(
        locate::<Changelog>(base, None).unwrap(),
        Some(base.join("changelog.d"))
    );
}

#[test]
fn locate_returns_an_explicit_dir_as_given_even_if_absent() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        locate::<Changelog>(tmp.path(), Some(Path::new("elsewhere/frags"))).unwrap(),
        Some(PathBuf::from("elsewhere/frags"))
    );
}

#[test]
fn added_names_are_read_like_a_listing() {
    let (tmp, dir) = fragment_dir();
    write(&dir, "b.fixed.md", "- b\n");
    write(&dir, "a.added.md", "- a\n");
    write(&dir, "bad.added.md", "prose\n");
    write(&dir, "unlisted.added.md", "- not added in the range\n");
    fs::create_dir(dir.join("sub")).unwrap();
    symlink(dir.join("nowhere"), dir.join("dangling.added.md")).unwrap();
    write(tmp.path(), "target.md", "- linked secret\n");
    symlink(tmp.path().join("target.md"), dir.join("linked.added.md")).unwrap();
    let names: Vec<String> = [
        "b.fixed.md",
        "a.added.md",
        "bad.added.md",
        ".gitkeep",
        "sub/x.added.md",
        "sub",
        "dangling.added.md",
        "linked.added.md",
        "gone.added.md",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let scan = scan_added::<Changelog, _>(&dir, &names, "main...HEAD").unwrap();
    assert_eq!(
        paths(&scan),
        [rel(&dir, "a.added.md"), rel(&dir, "b.fixed.md")]
    );
    assert_eq!(
        problems(&scan),
        [
            (rel(&dir, "bad.added.md"), ProblemCode::NotAList),
            (rel(&dir, "dangling.added.md"), ProblemCode::NotAFragment),
            (rel(&dir, "gone.added.md"), ProblemCode::Missing),
            (rel(&dir, "linked.added.md"), ProblemCode::NotAFragment),
            (rel(&dir, "sub"), ProblemCode::NotAFragment),
            (rel(&dir, "sub/x.added.md"), ProblemCode::NotAFragment),
        ]
    );
    let missing = &scan.problems[2];
    assert_eq!(
        missing.message,
        "added in main...HEAD but not present in the working tree"
    );
    assert_eq!(scan.problems[3].message, "not a fragment: a symlink");
}

#[test]
fn nothing_added_is_an_empty_scan() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        scan_added::<Changelog, String>(&tmp.path().join("changelog.d"), &[], "A..B").unwrap(),
        Scan::default()
    );
}

#[test]
fn locate_todo_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    fs::create_dir(base.join("changelog.d")).unwrap();
    assert_eq!(locate::<Todo>(base, None).unwrap(), None);
    fs::create_dir(base.join(".todo.d")).unwrap();
    assert_eq!(
        locate::<Todo>(base, None).unwrap(),
        Some(base.join(".todo.d"))
    );
    fs::create_dir(base.join("todo.d")).unwrap();
    let both = locate::<Todo>(base, None).unwrap_err();
    assert_eq!(
        both.to_string(),
        "both todo.d and .todo.d exist; pass --dir"
    );
    fs::remove_dir(base.join(".todo.d")).unwrap();
    assert_eq!(
        locate::<Todo>(base, None).unwrap(),
        Some(base.join("todo.d"))
    );
}

#[test]
fn todo_symlinks_are_not_fragments() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("todo.d");
    fs::create_dir(&dir).unwrap();
    write(tmp.path(), "target.md", "Text.\n");
    symlink(tmp.path().join("target.md"), dir.join("linked.md")).unwrap();
    symlink(dir.join("gone"), dir.join("dangling.md")).unwrap();
    for scan in [
        scan::<Todo>(&dir).unwrap(),
        scan_added::<Todo, _>(&dir, &["dangling.md", "linked.md"], "A..B").unwrap(),
    ] {
        assert!(scan.fragments.is_empty(), "{:?}", scan.fragments);
        let messages: Vec<(String, &str)> = scan
            .problems
            .iter()
            .map(|p| (p.path.clone(), p.message.as_str()))
            .collect();
        assert_eq!(
            messages,
            [
                (rel(&dir, "dangling.md"), "not a fragment: a symlink"),
                (rel(&dir, "linked.md"), "not a fragment: a symlink"),
            ]
        );
        assert!(
            scan.problems
                .iter()
                .all(|p| p.code == ProblemCode::NotAFragment)
        );
    }
}

#[test]
fn todo_scan_sorts_by_slug_and_lints_names() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("todo.d");
    fs::create_dir(&dir).unwrap();
    for name in ["zeta.md", "Alpha.md", "a-b_1.md", "mid.md"] {
        write(&dir, name, "Text.\n");
    }
    write(&dir, "a.added.md", "- A changelog fragment.\n");
    write(&dir, ".gitkeep", "");
    let scan = scan::<Todo>(&dir).unwrap();
    let slugs: Vec<&str> = scan
        .fragments
        .iter()
        .map(|f| f.name.slug.as_str())
        .collect();
    assert_eq!(slugs, ["Alpha", "a-b_1", "mid", "zeta"]);
    assert_eq!(scan.fragments[0].body, ["Text."]);
    assert_eq!(
        problems(&scan),
        [(rel(&dir, "a.added.md"), ProblemCode::BadName)]
    );
}
