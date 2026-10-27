use super::*;

#[test]
fn deeper_headings_are_not_release_headings() {
    let found = headings(
        "### [1.0.0]\n#### x\n##no-space\n## [1.0.0] \n~~~\n## [0.9.0]\n~~~\n## [0.8.0]\n",
        "## ",
    );
    assert_eq!(
        found,
        [
            Heading {
                line: 3,
                text: "[1.0.0]"
            },
            Heading {
                line: 7,
                text: "[0.8.0]"
            }
        ]
    );
}

#[test]
fn atx_headings_report_every_level_outside_fences() {
    let text =
        "# T\n\n## `a` \n#### sub\n```\n## fenced\n```\n##\n##no\n####### seven\n###### six\n";
    let h = |line, level, text| AtxHeading { line, level, text };
    assert_eq!(
        atx_headings(text),
        [
            h(0, 1, "T"),
            h(2, 2, "`a`"),
            h(3, 4, "sub"),
            h(7, 2, ""),
            h(10, 6, "six"),
        ]
    );
}

#[test]
fn fences_are_three_backticks_or_tildes_in_column_zero() {
    for fence in ["```", "```rust", "~~~", "~~~~ md"] {
        assert!(is_fence(fence), "{fence}");
    }
    for not in ["``", "~~", " ```", "text ```"] {
        assert!(!is_fence(not), "{not}");
    }
}

#[test]
fn insert_separates_with_exactly_one_blank_line() {
    assert_eq!(insert("a\n\n\nb\n\n", 3, "new\n"), "a\n\nnew\n\nb\n");
    assert_eq!(insert("a", 1, "new\n"), "a\n\nnew\n");
    assert_eq!(insert("\n\nb\n", 0, "new\n"), "new\n\nb\n");
}

#[test]
fn read_creates_from_the_header_when_absent() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(
        read(&tmp.path().join("TODO.md"), "# H\n").unwrap(),
        ("# H\n".to_owned(), true)
    );
    let path = tmp.path().join("present.md");
    fs::write(&path, "text\n").unwrap();
    assert_eq!(read(&path, "# H\n").unwrap(), ("text\n".to_owned(), false));
}

#[test]
fn a_failed_deletion_names_the_target() {
    let tmp = tempfile::tempdir().unwrap();
    let kept = tmp.path().join("a.md");
    fs::write(&kept, "").unwrap();
    let gone = tmp.path().join("gone.md");
    let err = delete(Path::new("TODO.md"), &[kept.clone(), gone.clone()]).unwrap_err();
    let Error::Delete {
        target,
        deleted,
        remaining,
        ..
    } = &err
    else {
        panic!("{err}");
    };
    assert_eq!(target, Path::new("TODO.md"));
    assert_eq!(deleted, &[kept]);
    assert_eq!(remaining, &[gone]);
    assert!(
        err.to_string()
            .starts_with("TODO.md holds the gathered fragments, but deleting")
    );
}
