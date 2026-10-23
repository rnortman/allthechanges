//! Runs the built `allthechanges` binary. Bazel supplies its path through
//! `ALLTHECHANGES_BIN`; this test does not build under `cargo test`.
//!
//! Every run strips `GIT_*` and pins git's configuration to `/dev/null`
//! through `brenn_git_fixture::hermetic`, so the git the binary spawns in
//! range mode sees only the fixture repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use brenn_git_fixture::{git, seed_repo};
use serde_json::{Value, json};

fn bin() -> PathBuf {
    fs::canonicalize(env!("ALLTHECHANGES_BIN")).expect("the binary is in the runfiles")
}

/// Runs the binary in `cwd` with `args`.
fn run(cwd: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(bin());
    cmd.current_dir(cwd).args(args);
    brenn_git_fixture::hermetic(&mut cmd);
    cmd.output().expect("spawn allthechanges")
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("stderr is UTF-8")
}

/// Asserts the exit code and the stream contract that goes with it: on 0
/// and 1 nothing on stderr, on 2 and 3 nothing on stdout.
fn assert_exit(output: &Output, code: i32) {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    if code <= 1 {
        assert!(
            output.stderr.is_empty(),
            "stderr on exit {code}: {output:?}"
        );
    } else {
        assert!(
            output.stdout.is_empty(),
            "stdout on exit {code}: {output:?}"
        );
    }
}

/// The JSON report: exactly one document followed by a newline.
fn json(output: &Output) -> Value {
    let text = stdout(output);
    assert!(text.ends_with('\n'), "no trailing newline: {text:?}");
    serde_json::from_str(text).unwrap_or_else(|e| panic!("not one JSON document: {e}: {text}"))
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn version_prints_name_and_version() {
    let tmp = tempfile::tempdir().unwrap();
    let output = run(tmp.path(), &["--version"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        format!("allthechanges {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn usage_errors_exit_two_with_empty_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [&[][..], &["--nope"], &["query", "--nope"], &["frobnicate"]] {
        let output = run(tmp.path(), args);
        assert_exit(&output, 2);
        assert!(!output.stderr.is_empty(), "{args:?}: {output:?}");
    }
}

#[test]
fn help_lists_the_exit_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let output = run(tmp.path(), &["--help"]);
    assert_exit(&output, 0);
    assert!(
        stdout(&output).contains("Exit codes:"),
        "{}",
        stdout(&output)
    );
}

/// A directory with two good fragments in two sections, a dotfile and no
/// problems.
fn clean_fixture(root: &Path) {
    write(root, "changelog.d/b.fixed.md", "- Fixed B.\n");
    write(
        root,
        "changelog.d/2026-10-23.a.added.md",
        "- Added A.\n  More about A.\n",
    );
    write(root, "changelog.d/.gitkeep", "");
}

#[test]
fn query_lists_fragments_for_humans() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(tmp.path(), &["query"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        "changelog.d: 2 fragments, 0 problems\n\
         \n\
         ### Added\n\
         \n\
         - Added A.  [2026-10-23.a.added.md]\n  More about A.\n\
         \n\
         ### Fixed\n\
         \n\
         - Fixed B.  [b.fixed.md]\n"
    );
}

#[test]
fn query_json_matches_the_schema() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(tmp.path(), &["query", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(
        json(&output),
        json!({
            "dir": "changelog.d",
            "range": null,
            "fragments": [
                {"path": "changelog.d/2026-10-23.a.added.md", "date": "2026-10-23",
                 "slug": "a", "section": "added",
                 "body": ["- Added A.", "  More about A."]},
                {"path": "changelog.d/b.fixed.md", "date": null,
                 "slug": "b", "section": "fixed", "body": ["- Fixed B."]},
            ],
            "problems": [],
            "ok": true,
        })
    );
}

#[test]
fn global_flags_may_precede_the_subcommand() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(tmp.path(), &["--json", "query"]);
    assert_exit(&output, 0);
    assert_eq!(json(&output)["ok"], true);
}

#[test]
fn problems_exit_one_with_the_report_on_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(tmp.path(), "changelog.d/typo.fxied.md", "- Typo.\n");
    write(tmp.path(), "changelog.d/prose.added.md", "Just prose.\n");

    let output = run(tmp.path(), &["query", "--json"]);
    assert_exit(&output, 1);
    let report = json(&output);
    assert_eq!(report["ok"], false);
    assert_eq!(report["fragments"].as_array().unwrap().len(), 2);
    let problems: Vec<(&str, &str)> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["path"].as_str().unwrap(), p["code"].as_str().unwrap()))
        .collect();
    assert_eq!(
        problems,
        [
            ("changelog.d/prose.added.md", "not-a-list"),
            ("changelog.d/typo.fxied.md", "unknown-section"),
        ]
    );

    let output = run(tmp.path(), &["query"]);
    assert_exit(&output, 1);
    let text = stdout(&output);
    assert!(
        text.starts_with("changelog.d: 2 fragments, 2 problems\n"),
        "{text}"
    );
    assert!(
        text.contains("\nProblems:\nchangelog.d/prose.added.md: not-a-list: "),
        "{text}"
    );
}

#[test]
fn no_directory_is_zero_fragments() {
    let tmp = tempfile::tempdir().unwrap();
    let output = run(tmp.path(), &["query", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(
        json(&output),
        json!({"dir": null, "range": null, "fragments": [], "problems": [], "ok": true})
    );

    let output = run(tmp.path(), &["query"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        "no changelog.d or .changelog.d: 0 fragments\n"
    );
}

#[test]
fn require_without_a_fragment_exits_one() {
    let tmp = tempfile::tempdir().unwrap();
    let output = run(tmp.path(), &["query", "--require", "--json"]);
    assert_exit(&output, 1);
    assert_eq!(json(&output)["ok"], false);

    fs::create_dir(tmp.path().join("changelog.d")).unwrap();
    let output = run(tmp.path(), &["query", "--require"]);
    assert_exit(&output, 1);
    assert!(stdout(&output).contains("--require"), "{output:?}");

    write(tmp.path(), "changelog.d/a.added.md", "- A.\n");
    assert_exit(&run(tmp.path(), &["query", "--require"]), 0);
}

#[test]
fn hidden_directory_is_found() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), ".changelog.d/a.added.md", "- A.\n");
    let output = run(tmp.path(), &["query", "--json"]);
    assert_exit(&output, 0);
    let report = json(&output);
    assert_eq!(report["dir"], ".changelog.d");
    assert_eq!(report["fragments"][0]["path"], ".changelog.d/a.added.md");
}

#[test]
fn both_directories_exit_three() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("changelog.d")).unwrap();
    fs::create_dir(tmp.path().join(".changelog.d")).unwrap();
    let output = run(tmp.path(), &["query", "--json"]);
    assert_exit(&output, 3);
    assert_eq!(
        stderr(&output),
        "allthechanges: both changelog.d and .changelog.d exist; pass --dir\n"
    );

    let output = run(tmp.path(), &["query", "--dir", ".changelog.d"]);
    assert_exit(&output, 0);
}

#[test]
fn explicit_dir_is_reported_as_given() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "notes/a.added.md", "- A.\n");
    let output = run(tmp.path(), &["query", "--json", "--dir", "notes"]);
    assert_exit(&output, 0);
    let report = json(&output);
    assert_eq!(report["dir"], "notes");
    assert_eq!(report["fragments"][0]["path"], "notes/a.added.md");

    let output = run(tmp.path(), &["--dir", "absent", "query", "--json"]);
    assert_exit(&output, 0);
    assert_eq!(
        json(&output),
        json!({"dir": "absent", "range": null, "fragments": [], "problems": [], "ok": true})
    );
}

#[test]
fn dir_naming_a_file_exits_three() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "file", "");
    for args in [
        &["query", "--dir", "file"][..],
        &["query", "--dir", "file", "--range", "HEAD"],
    ] {
        let output = run(tmp.path(), args);
        assert_exit(&output, 3);
        assert_eq!(stderr(&output), "allthechanges: file: not a directory\n");
    }
}

// --- Range mode ---------------------------------------------------------

/// A repository whose first commit holds `changelog.d/old.added.md`; tagged
/// `base`.
fn range_fixture(root: &Path) {
    seed_repo(root);
    write(root, "changelog.d/old.added.md", "- Old.\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "old fragment"]);
    git(root, &["tag", "base"]);
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", message]);
}

fn fragment_paths(report: &Value) -> Vec<&str> {
    report["fragments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect()
}

#[test]
fn range_lists_only_fragments_added_in_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    write(root, "changelog.d/new.fixed.md", "- New.\n");
    write(root, "changelog.d/old.added.md", "- Old, edited.\n");
    commit_all(root, "new fragment");

    let output = run(root, &["query", "--json", "--range", "base..HEAD"]);
    assert_exit(&output, 0);
    let report = json(&output);
    assert_eq!(report["range"], "base..HEAD");
    assert_eq!(fragment_paths(&report), ["changelog.d/new.fixed.md"]);

    let output = run(root, &["query", "--range", "base...HEAD", "--require"]);
    assert_exit(&output, 0);
    assert!(
        stdout(&output).starts_with("changelog.d: 1 fragment added in base...HEAD, 0 problems\n"),
        "{output:?}"
    );
}

#[test]
fn range_with_nothing_added_fails_require() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    write(root, "src.txt", "code\n");
    commit_all(root, "no fragment");

    let output = run(root, &["query", "--json", "--range", "base..HEAD"]);
    assert_exit(&output, 0);
    assert!(fragment_paths(&json(&output)).is_empty());

    let output = run(
        root,
        &["query", "--json", "--range", "base..HEAD", "--require"],
    );
    assert_exit(&output, 1);
    assert_eq!(json(&output)["ok"], false);
}

#[test]
fn range_lints_what_it_lists() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    write(root, "changelog.d/bad.added.md", "prose\n");
    write(root, "changelog.d/nested/x.added.md", "- X.\n");
    write(root, "changelog.d/.gitkeep", "");
    commit_all(root, "bad fragments");

    let output = run(root, &["query", "--json", "--range", "base..HEAD"]);
    assert_exit(&output, 1);
    let report = json(&output);
    let codes: Vec<(&str, &str)> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["path"].as_str().unwrap(), p["code"].as_str().unwrap()))
        .collect();
    assert_eq!(
        codes,
        [
            ("changelog.d/bad.added.md", "not-a-list"),
            ("changelog.d/nested/x.added.md", "not-a-fragment"),
        ]
    );
}

#[test]
fn fragment_deleted_from_the_working_tree_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    write(root, "changelog.d/gone.added.md", "- Gone.\n");
    commit_all(root, "fragment");
    fs::remove_file(root.join("changelog.d/gone.added.md")).unwrap();

    let output = run(root, &["query", "--json", "--range", "base..HEAD"]);
    assert_exit(&output, 1);
    assert_eq!(
        json(&output)["problems"],
        json!([{
            "path": "changelog.d/gone.added.md",
            "code": "missing",
            "message": "added in base..HEAD but not present in the working tree",
        }])
    );
}

#[test]
fn fragment_moved_in_by_git_mv_is_added() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    write(root, "draft.md", "- Drafted elsewhere.\n");
    commit_all(root, "draft");
    git(root, &["tag", "drafted"]);
    git(root, &["config", "diff.renames", "true"]);
    git(root, &["mv", "draft.md", "changelog.d/moved.added.md"]);
    git(root, &["commit", "-qm", "move the draft in"]);

    let output = run(root, &["query", "--json", "--range", "drafted..HEAD"]);
    assert_exit(&output, 0);
    assert_eq!(
        fragment_paths(&json(&output)),
        ["changelog.d/moved.added.md"]
    );
}

#[test]
fn range_from_a_subdirectory_of_the_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_repo(root);
    git(root, &["tag", "base"]);
    write(root, "pkg/changelog.d/a.added.md", "- A.\n");
    write(root, "changelog.d/top.added.md", "- Top.\n");
    commit_all(root, "fragments");

    let output = run(
        &root.join("pkg"),
        &["query", "--json", "--range", "base..HEAD"],
    );
    assert_exit(&output, 0);
    assert_eq!(fragment_paths(&json(&output)), ["changelog.d/a.added.md"]);

    let output = run(
        &root.join("pkg"),
        &[
            "query",
            "--json",
            "--dir",
            "../changelog.d",
            "--range",
            "base..HEAD",
        ],
    );
    assert_exit(&output, 0);
    assert_eq!(
        fragment_paths(&json(&output)),
        ["../changelog.d/top.added.md"]
    );
}

#[test]
fn bad_range_exits_three_with_gits_message() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    range_fixture(root);
    let output = run(root, &["query", "--range", "no-such-ref..HEAD"]);
    assert_exit(&output, 3);
    let message = stderr(&output);
    assert!(message.starts_with("allthechanges: "), "{message}");
    assert!(message.contains("no-such-ref"), "{message}");
    assert_eq!(message.lines().count(), 1, "{message}");
}

#[test]
fn range_outside_a_repository_exits_three() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("changelog.d")).unwrap();
    let output = run(tmp.path(), &["query", "--range", "HEAD"]);
    assert_exit(&output, 3);
    assert!(
        stderr(&output).contains("not a git repository"),
        "{output:?}"
    );
}

#[test]
fn range_without_a_directory_is_zero_fragments() {
    let tmp = tempfile::tempdir().unwrap();
    seed_repo(tmp.path());
    let output = run(tmp.path(), &["query", "--json", "--range", "HEAD"]);
    assert_exit(&output, 0);
    let report = json(&output);
    assert_eq!(report["dir"], Value::Null);
    assert!(fragment_paths(&report).is_empty());
}

#[test]
fn range_without_a_directory_still_asks_git() {
    let tmp = tempfile::tempdir().unwrap();
    let output = run(tmp.path(), &["query", "--range", "HEAD"]);
    assert_exit(&output, 3);
    assert!(
        stderr(&output).contains("not a git repository"),
        "{output:?}"
    );

    let root = tmp.path();
    range_fixture(root);
    fs::remove_dir_all(root.join("changelog.d")).unwrap();
    for args in [
        &["query", "--range", "no-such-ref..HEAD", "--require"][..],
        &[
            "query",
            "--dir",
            "pkg/changelog.d",
            "--range",
            "no-such-ref..HEAD",
        ],
    ] {
        let output = run(root, args);
        assert_exit(&output, 3);
        assert!(stderr(&output).contains("no-such-ref"), "{output:?}");
    }
}

#[test]
fn fragments_of_a_directory_removed_from_the_working_tree_are_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    seed_repo(root);
    git(root, &["tag", "base"]);
    write(root, "changelog.d/a.added.md", "- A.\n");
    commit_all(root, "fragment");
    fs::remove_dir_all(root.join("changelog.d")).unwrap();

    let output = run(root, &["query", "--json", "--range", "base..HEAD"]);
    assert_exit(&output, 1);
    let report = json(&output);
    assert_eq!(report["dir"], Value::Null);
    assert_eq!(
        report["problems"],
        json!([{
            "path": "changelog.d/a.added.md",
            "code": "missing",
            "message": "added in base..HEAD but not present in the working tree",
        }])
    );
}

const GATHERED: &str = "\
## [1.0.0] - 2026-10-23

### Added

- Added A.
  More about A.

### Fixed

- Fixed B.
";

const EXISTING: &str = "\
# Changelog

## [Unreleased]

- Hand-written.

## [0.9.0] - 2026-01-01

- Old.

[0.9.0]: https://example.com/0.9.0
";

fn read(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(name)).unwrap()
}

/// The `changelog.d` entries left, sorted.
fn left(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("changelog.d"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn gather_writes_the_release_and_deletes_the_fragments() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(tmp.path(), "CHANGELOG.md", EXISTING);
    let output = run(
        tmp.path(),
        &[
            "gather",
            "--version",
            "1.0.0",
            "--date",
            "2026-10-23",
            "--json",
        ],
    );
    assert_exit(&output, 0);
    assert_eq!(
        read(tmp.path(), "CHANGELOG.md"),
        EXISTING.replace("## [0.9.0]", &format!("{GATHERED}\n## [0.9.0]"))
    );
    assert_eq!(left(tmp.path()), [".gitkeep"]);
    assert_eq!(
        json(&output),
        json!({
            "changelog": "CHANGELOG.md",
            "created": false,
            "version": "1.0.0",
            "date": "2026-10-23",
            "dry_run": false,
            "gathered": ["changelog.d/2026-10-23.a.added.md", "changelog.d/b.fixed.md"],
            "rendered": GATHERED,
            "problems": [],
            "refused": null,
            "ok": true,
        })
    );
}

#[test]
fn gather_for_humans_names_what_it_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(
        tmp.path(),
        &["gather", "--version", "v2", "--date", "2026-10-23"],
    );
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        "gathered 2 fragments into CHANGELOG.md (created) as [v2] - 2026-10-23\n\
         deleted changelog.d/2026-10-23.a.added.md\n\
         deleted changelog.d/b.fixed.md\n"
    );
}

#[test]
fn gather_creates_an_absent_changelog() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(
        tmp.path(),
        &[
            "--json",
            "gather",
            "--version",
            "1.0.0",
            "--date",
            "2026-10-23",
        ],
    );
    assert_exit(&output, 0);
    assert_eq!(json(&output)["created"], true);
    let written = read(tmp.path(), "CHANGELOG.md");
    assert!(
        written.starts_with("# Changelog\n\nAll notable changes"),
        "{written}"
    );
    assert!(
        written.ends_with(&format!(".html).\n\n{GATHERED}")),
        "{written}"
    );
}

#[test]
fn gather_dry_run_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(tmp.path(), "CHANGELOG.md", EXISTING);
    let before = left(tmp.path());
    let args = [
        "gather",
        "--version",
        "1.0.0",
        "--date",
        "2026-10-23",
        "--dry-run",
    ];
    let output = run(tmp.path(), &args);
    assert_exit(&output, 0);
    assert_eq!(
        stdout(&output),
        format!("dry run: would write to CHANGELOG.md:\n\n{GATHERED}")
    );
    let output = run(tmp.path(), &[&args[..], &["--json"]].concat());
    assert_exit(&output, 0);
    let report = json(&output);
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["rendered"], GATHERED);
    assert_eq!(read(tmp.path(), "CHANGELOG.md"), EXISTING);
    assert_eq!(left(tmp.path()), before);
}

/// Runs a gather that must refuse with `refused`, and checks nothing changed.
fn assert_refused(root: &Path, refused: &str) -> Value {
    let changelog = fs::read(root.join("CHANGELOG.md")).ok();
    let fragments = root.join("changelog.d").exists().then(|| left(root));
    let output = run(root, &["gather", "--version", "1.0.0", "--json"]);
    assert_exit(&output, 1);
    let report = json(&output);
    assert_eq!(report["refused"], refused);
    assert_eq!(report["ok"], false);
    assert_eq!(report["rendered"], Value::Null);
    assert_eq!(report["gathered"], json!([]));
    assert_eq!(fs::read(root.join("CHANGELOG.md")).ok(), changelog);
    assert_eq!(
        root.join("changelog.d").exists().then(|| left(root)),
        fragments
    );
    let human = run(root, &["gather", "--version", "1.0.0"]);
    assert_exit(&human, 1);
    assert!(
        stdout(&human).starts_with("nothing gathered: "),
        "{human:?}"
    );
    report
}

#[test]
fn gather_refuses_on_problems() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(tmp.path(), "changelog.d/prose.added.md", "Just prose.\n");
    let report = assert_refused(tmp.path(), "problems");
    assert_eq!(report["problems"][0]["code"], "not-a-list");
    assert!(!tmp.path().join("CHANGELOG.md").exists());
}

#[test]
fn gather_refuses_a_version_already_present() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(
        tmp.path(),
        "CHANGELOG.md",
        "# Changelog\n\n## [1.0.0] - 2026-01-01\n\n- Done.\n",
    );
    let report = assert_refused(tmp.path(), "version-present");
    assert_eq!(report["problems"], json!([]));
}

#[test]
fn gather_refuses_with_nothing_to_gather() {
    let tmp = tempfile::tempdir().unwrap();
    assert_refused(tmp.path(), "nothing-to-gather");
    write(tmp.path(), "changelog.d/.gitkeep", "");
    assert_refused(tmp.path(), "nothing-to-gather");
}

#[test]
fn gather_honours_dir_and_changelog() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "notes/x.removed.md", "- Gone.\n");
    let output = run(
        tmp.path(),
        &[
            "gather",
            "--dir",
            "notes",
            "--changelog",
            "HISTORY.md",
            "--version",
            "3",
            "--date",
            "2026-10-23",
        ],
    );
    assert_exit(&output, 0);
    assert!(
        read(tmp.path(), "HISTORY.md").ends_with("## [3] - 2026-10-23\n\n### Removed\n\n- Gone.\n")
    );
    assert!(!tmp.path().join("notes/x.removed.md").exists());
    assert!(tmp.path().join("notes").is_dir());
}

#[test]
fn gather_defaults_the_date_to_today() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    let output = run(
        tmp.path(),
        &["--json", "gather", "--version", "1", "--dry-run"],
    );
    assert_exit(&output, 0);
    let date = json(&output)["date"].as_str().unwrap().to_owned();
    assert_eq!(date.len(), 10, "{date}");
    assert!(stdout(&output).contains(&format!("## [1] - {date}")));
}

#[test]
fn gather_bad_values_are_usage_errors() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    for args in [
        &["gather"][..],
        &["gather", "--version", "1 0"],
        &["gather", "--version", "[1]"],
        &["gather", "--version", ""],
        &["gather", "--version", "1", "--date", "2026-02-30"],
        &["gather", "--version", "1", "--date", "23/10/2026"],
    ] {
        let output = run(tmp.path(), args);
        assert_exit(&output, 2);
    }
    assert_eq!(left(tmp.path()).len(), 3);
}

#[test]
fn gather_crlf_changelog_exits_three() {
    let tmp = tempfile::tempdir().unwrap();
    clean_fixture(tmp.path());
    write(tmp.path(), "CHANGELOG.md", "# Changelog\r\n");
    let output = run(tmp.path(), &["gather", "--version", "1"]);
    assert_exit(&output, 3);
    assert_eq!(
        stderr(&output),
        "allthechanges: CHANGELOG.md: CRLF line endings are not supported\n"
    );
    assert_eq!(read(tmp.path(), "CHANGELOG.md"), "# Changelog\r\n");
    assert_eq!(left(tmp.path()).len(), 3);
}
