use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::scan::Fragment;

fn name_err(s: &str) -> Finding {
    match TodoName::parse(s) {
        Ok(n) => panic!("{s} parsed as {n:?}"),
        Err(f) => f,
    }
}

fn body(s: &str) -> Vec<String> {
    parse_body(s.as_bytes()).unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
}

/// The `not-an-entry` message for a body that must be rejected.
fn body_err(s: &[u8]) -> String {
    match parse_body(s) {
        Ok(b) => panic!("{s:?} parsed as {b:?}"),
        Err(f) => {
            assert_eq!(f.code, ProblemCode::NotAnEntry, "{s:?}: {}", f.message);
            f.message
        }
    }
}

#[test]
fn slug_names() {
    assert_eq!(
        TodoName::parse("a-b_1.md").unwrap(),
        TodoName {
            slug: "a-b_1".into()
        }
    );
    for bad in ["a.b.md", ".md", "a b.md", "x.added.md"] {
        let finding = name_err(bad);
        assert_eq!(finding.code, ProblemCode::BadName, "{bad}");
        assert_eq!(
            finding.message,
            "expected <slug>.md, where the slug is letters, digits, '-' and '_'"
        );
    }
    for not in ["a.txt", "a.MD", "a"] {
        let finding = name_err(not);
        assert_eq!(finding.code, ProblemCode::NotAFragment, "{not}");
        assert_eq!(finding.message, "not a fragment: fragment files end in .md");
    }
}

#[test]
fn names_order_by_slug() {
    let mut names: Vec<TodoName> = ["b", "a-z", "a", "B", "a_"]
        .iter()
        .map(|s| TodoName::parse(&format!("{s}.md")).unwrap())
        .collect();
    names.sort();
    let slugs: Vec<&str> = names.iter().map(|n| n.slug.as_str()).collect();
    assert_eq!(slugs, ["B", "a", "a-z", "a_", "b"]);
}

#[test]
fn paragraphs_and_lists_are_entries() {
    assert_eq!(
        body("First paragraph\ncontinues.  \n\n- a list\n- item\n\n\n"),
        ["First paragraph", "continues.", "", "- a list", "- item"]
    );
}

#[test]
fn things_the_scanner_never_reads_as_headings_are_accepted() {
    for text in [
        "Text.\n\n```sh\n# a shell comment\n## and more\n```\n",
        "Text.\n\n~~~\n### x\n~~~\n",
        "#42 is the upstream issue.\n",
        "#[derive(Debug)] is in the way.\n",
        "Setext\n---\n",
        "Text.\n ## indented\n",
        "#hashtag\n",
        "####### seven\n",
    ] {
        assert_eq!(body(text).join("\n"), text.trim_end(), "{text:?}");
    }
}

#[test]
fn crlf_bodies_lose_their_carriage_returns() {
    assert_eq!(
        body("Line one.\r\nLine two.\r\n"),
        ["Line one.", "Line two."]
    );
}

#[test]
fn rejected_bodies_name_the_line() {
    for (bytes, message) in [
        (
            &b"Text.\nmid\rline\n"[..],
            "line 2: contains a carriage return",
        ),
        (b"\nText.\n", "line 1: an entry starts with text"),
        (b"  \nText.\n", "line 1: an entry starts with text"),
        (
            b"### x\n",
            "line 1: a heading; the entry's heading is its file name",
        ),
        (
            b"Text.\n# x\n",
            "line 2: a heading; the entry's heading is its file name",
        ),
        (
            b"Text.\n\n###### x\n",
            "line 3: a heading; the entry's heading is its file name",
        ),
        (
            b"Text.\n#\n",
            "line 2: a heading; the entry's heading is its file name",
        ),
        (
            b"Text.\n```\ncode\n```\n## after\n",
            "line 5: a heading; the entry's heading is its file name",
        ),
        (
            b"Text.\n\n```\ncode\n```\n```sh\nopen\n",
            "line 6: code fence not closed",
        ),
        (b"Text.\n~~~\n## hidden\n", "line 2: code fence not closed"),
    ] {
        assert_eq!(body_err(bytes), message, "{bytes:?}");
    }
}

#[test]
fn empty_and_non_utf8_bodies_keep_their_codes() {
    assert_eq!(parse_body(b"\n\n").unwrap_err().code, ProblemCode::Empty);
    assert_eq!(
        parse_body(b"caf\xe9\n").unwrap_err().code,
        ProblemCode::NotUtf8
    );
}

fn fragment(dir: &Path, slug: &str, body: &[&str]) -> Fragment<Todo> {
    Fragment {
        path: dir.join(format!("{slug}.md")),
        name: TodoName { slug: slug.into() },
        body: body.iter().map(|l| l.to_string()).collect(),
    }
}

/// A scan of fragments with the given slugs, in slug order, each with a
/// one-line body naming it.
fn scan_of(dir: &Path, slugs: &[&str]) -> Scan<Todo> {
    let mut slugs = slugs.to_vec();
    slugs.sort();
    Scan {
        fragments: slugs
            .iter()
            .map(|s| fragment(dir, s, &[&format!("Body of {s}.")]))
            .collect(),
        problems: Vec::new(),
    }
}

fn entry(slug: &str) -> String {
    format!("### {slug}\n\nBody of {slug}.\n")
}

/// A tempdir holding the fragments on disk and a TODO path in it, written
/// with `todo` when given.
fn on_disk(slugs: &[&str], todo: Option<&str>) -> (tempfile::TempDir, Scan<Todo>, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("todo.d");
    fs::create_dir(&dir).unwrap();
    let scan = scan_of(&dir, slugs);
    for f in &scan.fragments {
        fs::write(&f.path, f.body.join("\n") + "\n").unwrap();
    }
    let path = tmp.path().join("TODO.md");
    if let Some(text) = todo {
        fs::write(&path, text).unwrap();
    }
    (tmp, scan, path)
}

/// Gathers the slugs into `existing` and returns the file written.
fn gathered(existing: &str, slugs: &[&str]) -> String {
    let (_tmp, scan, path) = on_disk(slugs, Some(existing));
    gather(&scan, &target(&path, false)).unwrap();
    fs::read_to_string(&path).unwrap()
}

fn target(todo: &Path, dry_run: bool) -> Target<'_> {
    Target { todo, dry_run }
}

const SORTED: &str = "\
# TODO

Preamble.

## Open

Open intro.

### beta

Body of beta.

### delta

Body of delta.

## Done

### alpha

Old.
";

#[test]
fn entries_go_before_the_first_greater_key_or_at_the_section_end() {
    assert_eq!(
        gathered(SORTED, &["alpha", "charlie", "echo"]),
        format!(
            "# TODO\n\nPreamble.\n\n## Open\n\nOpen intro.\n\n{}\n### beta\n\nBody of beta.\n\n{}\n### delta\n\nBody of delta.\n\n{}\n## Done\n\n### alpha\n\nOld.\n",
            entry("alpha"),
            entry("charlie"),
            entry("echo"),
        )
    );
}

#[test]
fn entries_sharing_an_insertion_line_go_in_slug_order() {
    let existing = "# TODO\n\n## Open\n\n### m\n\nM.\n";
    assert_eq!(
        gathered(existing, &["c", "a", "b"]),
        format!(
            "# TODO\n\n## Open\n\n{}\n{}\n{}\n### m\n\nM.\n",
            entry("a"),
            entry("b"),
            entry("c")
        )
    );
}

#[test]
fn an_empty_section_receives_entries_after_its_preamble() {
    let existing = "# TODO\n\n## Open\n\nNothing yet.\n\n\n## Done\n";
    assert_eq!(
        gathered(existing, &["x"]),
        format!(
            "# TODO\n\n## Open\n\nNothing yet.\n\n{}\n## Done\n",
            entry("x")
        )
    );
}

#[test]
fn open_last_with_trailing_blank_lines_ends_with_one_newline() {
    assert_eq!(
        gathered("# TODO\n\n## Open\n\n\n\n", &["x"]),
        format!("# TODO\n\n## Open\n\n{}", entry("x"))
    );
    assert_eq!(
        gathered("# TODO\n\n## Open", &["x"]),
        format!("# TODO\n\n## Open\n\n{}", entry("x"))
    );
}

#[test]
fn split_gathers_give_the_same_file() {
    let base = "# TODO\n\n## Open\n\n### m\n\nM.\n";
    let once = gathered(base, &["a", "n", "z"]);
    let twice = gathered(&gathered(base, &["n"]), &["a", "z"]);
    assert_eq!(once, twice);
}

#[test]
fn an_unsorted_file_is_not_reordered() {
    let existing = "## Open\n\n### z\n\nZ.\n\n### b\n\nB.\n";
    assert_eq!(
        gathered(existing, &["c"]),
        format!("## Open\n\n{}\n### z\n\nZ.\n\n### b\n\nB.\n", entry("c"))
    );
}

#[test]
fn keys_are_trimmed_and_not_required_to_be_slugs() {
    let existing = "## Open\n\n### Some title  \n\nT.\n\n#### m-sub\n\n### n\n";
    assert_eq!(
        gathered(existing, &["A", "m"]),
        format!(
            "## Open\n\n{}\n### Some title  \n\nT.\n\n#### m-sub\n\n{}\n### n\n",
            entry("A"),
            entry("m")
        )
    );
}

#[test]
fn fences_hide_headings_from_the_gather() {
    let existing = "\
# TODO

```md
## Open
### z
```

## Open

~~~
### z
## Done
~~~

### y
";
    assert_eq!(
        gathered(existing, &["z"]),
        format!("{existing}\n{}", entry("z"))
    );
}

#[test]
fn a_created_file_is_the_header_and_the_entries() {
    let (_tmp, scan, path) = on_disk(&["b", "a"], None);
    let outcome = gather(&scan, &target(&path, false)).unwrap();
    let rendered = format!("{}\n{}", entry("a"), entry("b"));
    assert_eq!(
        outcome,
        Outcome::Gathered {
            created: true,
            rendered: rendered.clone(),
            fragments: scan.fragments.iter().map(|f| f.path.clone()).collect(),
        }
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{HEADER}\n{rendered}")
    );
    for f in &scan.fragments {
        assert!(!f.path.exists(), "{}", f.path.display());
    }
}

#[test]
fn rendered_is_every_entry_in_slug_order_whatever_the_insertion_points() {
    let (_tmp, scan, path) = on_disk(&["alpha", "echo"], Some(SORTED));
    let Outcome::Gathered { rendered, .. } = gather(&scan, &target(&path, false)).unwrap() else {
        panic!("refused");
    };
    assert_eq!(rendered, format!("{}\n{}", entry("alpha"), entry("echo")));
}

#[test]
fn dry_run_writes_and_deletes_nothing() {
    let (_tmp, scan, path) = on_disk(&["a"], Some(SORTED));
    let outcome = gather(&scan, &target(&path, true)).unwrap();
    assert!(matches!(
        &outcome,
        Outcome::Gathered { created: false, rendered, .. } if *rendered == entry("a")
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), SORTED);
    assert!(scan.fragments[0].path.exists());
}

#[test]
fn a_present_slug_refuses_every_fragment_and_names_it() {
    let (_tmp, scan, path) = on_disk(&["a", "beta", "z"], Some(SORTED));
    let outcome = gather(&scan, &target(&path, false)).unwrap();
    let Outcome::Refused { refusal, present } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(refusal, Refusal::Problems);
    assert_eq!(
        present,
        [Problem {
            path: scan.fragments[1].path.display().to_string(),
            code: ProblemCode::Present,
            message: format!("{} already has an entry \"### beta\"", path.display()),
        }]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), SORTED);
    for f in &scan.fragments {
        assert!(f.path.exists(), "{}", f.path.display());
    }
}

#[test]
fn an_entry_outside_open_is_not_present() {
    assert_eq!(
        gathered(SORTED, &["alpha"]),
        format!(
            "# TODO\n\nPreamble.\n\n## Open\n\nOpen intro.\n\n{}\n### beta\n\nBody of beta.\n\n### delta\n\nBody of delta.\n\n## Done\n\n### alpha\n\nOld.\n",
            entry("alpha"),
        )
    );
}

#[test]
fn a_key_outside_open_is_not_an_insertion_point() {
    let existing = "# TODO\n\n## Open\n\n### m\n\nM.\n\n## Done\n\n### zz\n\nOld.\n";
    assert_eq!(
        gathered(existing, &["zulu"]),
        format!(
            "# TODO\n\n## Open\n\n### m\n\nM.\n\n{}\n## Done\n\n### zz\n\nOld.\n",
            entry("zulu")
        )
    );
}

#[test]
fn a_level_one_heading_ends_the_open_section() {
    let existing = "# TODO\n\n## Open\n\n### b\n\nB.\n\n# Archive\n\n### old\n\nOld.\n";
    assert_eq!(
        gathered(existing, &["a", "z"]),
        format!(
            "# TODO\n\n## Open\n\n{}\n### b\n\nB.\n\n{}\n# Archive\n\n### old\n\nOld.\n",
            entry("a"),
            entry("z")
        )
    );
    let (_tmp, scan, path) = on_disk(&["old"], Some(existing));
    assert!(matches!(
        gather(&scan, &target(&path, false)).unwrap(),
        Outcome::Gathered { .. }
    ));
}

#[test]
fn problems_and_nothing_to_gather_refuse_before_reading() {
    let missing = Path::new("no/such/TODO.md");
    assert_eq!(
        gather(&Scan::default(), &target(missing, false)).unwrap(),
        Outcome::Refused {
            refusal: Refusal::NothingToGather,
            present: Vec::new()
        }
    );
    let mut with_problem = scan_of(Path::new("todo.d"), &["a"]);
    with_problem
        .problems
        .push(Finding::new(ProblemCode::Empty, "no content").at("todo.d/e.md"));
    assert_eq!(
        gather(&with_problem, &target(missing, false)).unwrap(),
        Outcome::Refused {
            refusal: Refusal::Problems,
            present: Vec::new()
        }
    );
}

#[test]
fn open_must_appear_exactly_once() {
    for (existing, reason) in [
        ("# TODO\n\n## Opened\n", "no \"## Open\" section"),
        ("# TODO\n\n```\n## Open\n```\n", "no \"## Open\" section"),
        (
            "## Open\n\n### a\n\n## Open\n",
            "more than one \"## Open\" section",
        ),
    ] {
        let (_tmp, scan, path) = on_disk(&["x"], Some(existing));
        let err = gather(&scan, &target(&path, false)).unwrap_err();
        assert!(
            matches!(&err, Error::Target { path: p, reason: r } if *p == path && *r == reason),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), existing);
        assert!(scan.fragments[0].path.exists());
    }
}

#[test]
fn crlf_and_symlinked_todo_files_are_errors() {
    let (tmp, scan, path) = on_disk(&["x"], Some("## Open\r\n"));
    let err = gather(&scan, &target(&path, false)).unwrap_err();
    assert!(matches!(err, Error::Target { .. }), "{err}");
    fs::remove_file(&path).unwrap();
    let real = tmp.path().join("real.md");
    fs::write(&real, "## Open\n").unwrap();
    std::os::unix::fs::symlink(&real, &path).unwrap();
    let err = gather(&scan, &target(&path, false)).unwrap_err();
    assert!(
        matches!(
            &err,
            Error::Target {
                reason: "not a regular file",
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&real).unwrap(), "## Open\n");
}
