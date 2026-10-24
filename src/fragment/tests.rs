use super::*;

#[test]
fn problem_codes_serialise_as_written() {
    let problem = Finding::new(ProblemCode::NotAList, "m").at("changelog.d/x.added.md");
    assert_eq!(
        serde_json::to_string(&problem).unwrap(),
        r#"{"path":"changelog.d/x.added.md","code":"not-a-list","message":"m"}"#
    );
    for (code, text) in [
        (ProblemCode::BadName, "bad-name"),
        (ProblemCode::UnknownSection, "unknown-section"),
        (ProblemCode::NotAFragment, "not-a-fragment"),
        (ProblemCode::NotUtf8, "not-utf8"),
        (ProblemCode::Empty, "empty"),
        (ProblemCode::NotAList, "not-a-list"),
        (ProblemCode::NotAnEntry, "not-an-entry"),
        (ProblemCode::Missing, "missing"),
        (ProblemCode::Present, "present"),
    ] {
        assert_eq!(code.to_string(), text);
    }
}

#[test]
fn lines_trim_trailing_whitespace_and_blank_lines() {
    assert_eq!(
        lines(b"first  \t\n\n  indented \r\n\n \n\n").unwrap(),
        ["first", "", "  indented"]
    );
}

#[test]
fn lines_keep_a_carriage_return_not_before_a_newline() {
    let found = lines(b"a\rb\nc\r\n").unwrap();
    assert_eq!(found, ["a\rb", "c"]);
    assert_eq!(carriage_return(&found), Some(1));
    assert_eq!(
        carriage_return(&["x".to_owned(), "y\rz".to_owned()]),
        Some(2)
    );
    assert_eq!(carriage_return(&["x".to_owned()]), None);
}

#[test]
fn lines_of_nothing_are_empty() {
    for bytes in [&b""[..], b"\n", b" \t\n\n"] {
        let finding = lines(bytes).unwrap_err();
        assert_eq!(finding.code, ProblemCode::Empty, "{bytes:?}");
        assert_eq!(finding.message, "no content");
    }
}

#[test]
fn lines_reject_invalid_utf8() {
    let finding = lines(b"caf\xe9\n").unwrap_err();
    assert_eq!(finding.code, ProblemCode::NotUtf8);
}

#[test]
fn slugs() {
    for good in ["a", "a-b_1", "2026-10-23", "UPPER"] {
        assert!(is_slug(good), "{good}");
    }
    for bad in ["", "a.b", "a b", "caf\u{e9}", "a/b"] {
        assert!(!is_slug(bad), "{bad}");
    }
}
