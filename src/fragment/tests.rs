use super::*;

fn name(s: &str) -> FragmentName {
    FragmentName::parse(s).unwrap_or_else(|e| panic!("{s}: {e:?}"))
}

fn name_err(s: &str) -> ProblemCode {
    match FragmentName::parse(s) {
        Ok(n) => panic!("{s} parsed as {n:?}"),
        Err(f) => f.code,
    }
}

fn body(s: &str) -> Vec<String> {
    parse_body(s.as_bytes()).unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
}

fn body_err(s: &[u8]) -> ProblemCode {
    match parse_body(s) {
        Ok(b) => panic!("{s:?} parsed as {b:?}"),
        Err(f) => f.code,
    }
}

#[test]
fn undated_name() {
    assert_eq!(
        name("login-fix.fixed.md"),
        FragmentName {
            date: None,
            slug: "login-fix".into(),
            section: Section::Fixed,
        }
    );
}

#[test]
fn dated_name() {
    assert_eq!(
        name("2026-10-23.login_fix2.security.md"),
        FragmentName {
            date: Some("2026-10-23".into()),
            slug: "login_fix2".into(),
            section: Section::Security,
        }
    );
}

#[test]
fn date_is_shape_checked_only() {
    assert_eq!(
        name("2026-13-99.x.added.md").date.as_deref(),
        Some("2026-13-99")
    );
}

#[test]
fn a_date_shaped_slug_without_a_date_is_a_slug() {
    let n = name("2026-10-23.added.md");
    assert_eq!(n.date, None);
    assert_eq!(n.slug, "2026-10-23");
}

#[test]
fn every_section_name_parses() {
    for section in Section::ALL {
        assert_eq!(name(&format!("x.{section}.md")).section, section);
    }
}

#[test]
fn slug_with_a_dot_is_a_bad_name() {
    assert_eq!(name_err("v1.2.fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err("a.b.c.fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err("2026-10-23.a.b.fixed.md"), ProblemCode::BadName);
}

#[test]
fn bad_slugs_and_shapes_are_bad_names() {
    assert_eq!(name_err("fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err("README.md"), ProblemCode::BadName);
    assert_eq!(name_err("has space.fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err("x..fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err("2026-10-23..fixed.md"), ProblemCode::BadName);
    assert_eq!(name_err(".md"), ProblemCode::BadName);
}

#[test]
fn unknown_and_uppercase_sections() {
    assert_eq!(name_err("typo.fxied.md"), ProblemCode::UnknownSection);
    assert_eq!(name_err("x.Fixed.md"), ProblemCode::UnknownSection);
    let finding = FragmentName::parse("typo.fxied.md").unwrap_err();
    assert_eq!(
        finding.message,
        "\"fxied\" is not a section; expected one of added, changed, deprecated, removed, fixed, security"
    );
}

#[test]
fn wrong_extension_is_not_a_fragment() {
    assert_eq!(name_err("x.fixed.MD"), ProblemCode::NotAFragment);
    assert_eq!(name_err("x.fixed.txt"), ProblemCode::NotAFragment);
    assert_eq!(name_err("x.fixed"), ProblemCode::NotAFragment);
}

#[test]
fn section_order_and_headings() {
    let names: Vec<&str> = Section::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        [
            "added",
            "changed",
            "deprecated",
            "removed",
            "fixed",
            "security"
        ]
    );
    let mut sorted = Section::ALL;
    sorted.reverse();
    sorted.sort();
    assert_eq!(sorted, Section::ALL);
    for section in Section::ALL {
        let name = section.name();
        let capitalised = name[..1].to_uppercase() + &name[1..];
        assert_eq!(section.heading(), format!("### {capitalised}"));
        assert_eq!(
            serde_json::to_string(&section).unwrap(),
            format!("\"{name}\"")
        );
    }
    assert_eq!("security".parse::<Section>(), Ok(Section::Security));
    assert!("Security".parse::<Section>().is_err());
}

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
        (ProblemCode::Missing, "missing"),
    ] {
        assert_eq!(code.to_string(), text);
    }
}

#[test]
fn single_item() {
    assert_eq!(
        body("- Fix the login timeout.\n"),
        ["- Fix the login timeout."]
    );
}

#[test]
fn continuation_nested_and_blank_lines() {
    let text = "- First item\n  continues here.\n\n- Second item\n  - nested\n    deeper\n";
    assert_eq!(
        body(text),
        [
            "- First item",
            "  continues here.",
            "",
            "- Second item",
            "  - nested",
            "    deeper",
        ]
    );
}

#[test]
fn trailing_whitespace_and_blank_lines_trimmed() {
    assert_eq!(body("- item  \t\n  more \n\n  \n\n"), ["- item", "  more"]);
    assert_eq!(body("- item\r\n  more\r\n"), ["- item", "  more"]);
}

#[test]
fn prose_heading_and_star_bullets_are_not_lists() {
    assert_eq!(body_err(b"Fixed it.\n"), ProblemCode::NotAList);
    assert_eq!(body_err(b"### Fixed\n- item\n"), ProblemCode::NotAList);
    assert_eq!(body_err(b"- item\n# Heading\n"), ProblemCode::NotAList);
    assert_eq!(body_err(b"* item\n"), ProblemCode::NotAList);
    assert_eq!(body_err(b"- item\n* other\n"), ProblemCode::NotAList);
    assert_eq!(
        body_err(b"- item\n continued by one space\n"),
        ProblemCode::NotAList
    );
    assert_eq!(body_err(b"\n- leading blank line\n"), ProblemCode::NotAList);
    assert_eq!(body_err(b"-no space\n"), ProblemCode::NotAList);
}

#[test]
fn not_a_list_names_the_line() {
    let finding = parse_body(b"- ok\nprose\n").unwrap_err();
    assert!(
        finding.message.starts_with("line 2:"),
        "{}",
        finding.message
    );
}

#[test]
fn lone_carriage_returns_are_not_lists() {
    for (bytes, line) in [
        (&b"- a\r- b\r"[..], "line 1:"),
        (b"- fix\rthe thing\n", "line 1:"),
        (b"- ok\n  more\rtext\n", "line 2:"),
    ] {
        let finding = parse_body(bytes).unwrap_err();
        assert_eq!(finding.code, ProblemCode::NotAList);
        assert_eq!(
            finding.message,
            format!("{line} contains a carriage return")
        );
    }
}

#[test]
fn empty_bodies() {
    assert_eq!(body_err(b""), ProblemCode::Empty);
    assert_eq!(body_err(b"\n  \n\t\n"), ProblemCode::Empty);
}

#[test]
fn non_utf8_body() {
    assert_eq!(body_err(b"- caf\xe9\n"), ProblemCode::NotUtf8);
}
