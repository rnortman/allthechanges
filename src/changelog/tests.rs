use std::fs;
use std::path::{Path, PathBuf};

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

fn fragment(dir: &Path, file_name: &str, body: &[&str]) -> Fragment<Changelog> {
    Fragment {
        path: dir.join(file_name),
        name: FragmentName::parse(file_name).unwrap(),
        body: body.iter().map(|l| l.to_string()).collect(),
    }
}

/// Two sections, two fragments in the first, in scan order.
fn sample(dir: &Path) -> Scan<Changelog> {
    Scan {
        fragments: vec![
            fragment(
                dir,
                "2026-10-01.login.added.md",
                &["- Log in.", "  Continuation."],
            ),
            fragment(dir, "zed.added.md", &["- Zed.", "  - Nested."]),
            fragment(dir, "crash.fixed.md", &["- No crash."]),
        ],
        problems: Vec::new(),
    }
}

const SECTION: &str = "\
## [1.0.0] - 2026-10-23

### Added

- Log in.
  Continuation.
- Zed.
  - Nested.

### Fixed

- No crash.
";

/// Inserts the sample section into `existing` as `insert` would place it.
fn inserted(existing: &str) -> String {
    let headings = target::headings(existing, "## ");
    assert!(!headings.iter().any(|h| names_version(h.text, "1.0.0")));
    let at = insertion_line(&headings, existing.lines().count());
    target::insert(existing, at, SECTION)
}

#[test]
fn renders_sections_in_canonical_order_byte_for_byte() {
    let scan = sample(Path::new("changelog.d"));
    assert_eq!(render("1.0.0", "2026-10-23", &scan.fragments), SECTION);
}

#[test]
fn renders_only_sections_with_fragments() {
    let dir = Path::new("changelog.d");
    let fragments = [fragment(dir, "s.security.md", &["- Patched."])];
    assert_eq!(
        render("v2", "2026-01-02", &fragments),
        "## [v2] - 2026-01-02\n\n### Security\n\n- Patched.\n"
    );
}

#[test]
fn inserts_at_the_end_when_there_are_no_headings() {
    assert_eq!(inserted(HEADER), format!("{HEADER}\n{SECTION}"));
}

#[test]
fn trailing_blank_lines_collapse_to_one_separator() {
    assert_eq!(
        inserted("# Changelog\n\nIntro.\n\n\n\n"),
        format!("# Changelog\n\nIntro.\n\n{SECTION}")
    );
}

#[test]
fn inserts_after_unreleased_when_it_is_the_only_section() {
    let existing = "# Changelog\n\n## [Unreleased]\n\n- Pending.\n";
    assert_eq!(
        inserted(existing),
        format!("# Changelog\n\n## [Unreleased]\n\n- Pending.\n\n{SECTION}")
    );
}

#[test]
fn inserts_at_the_end_after_a_link_footer_when_unreleased_is_the_only_section() {
    let existing = "# Changelog\n\n## [Unreleased]\n\n- Pending.\n\n[Unreleased]: https://example.com/compare/v0...HEAD\n";
    assert_eq!(inserted(existing), format!("{existing}\n{SECTION}"));
}

#[test]
fn inserts_between_unreleased_and_the_first_release() {
    let existing = "# Changelog\n\n## [Unreleased]\n\n- Pending.\n\n## [0.9.0] - 2026-01-01\n\n- Old.\n\n[0.9.0]: https://example.com/0.9.0\n";
    assert_eq!(
        inserted(existing),
        format!(
            "# Changelog\n\n## [Unreleased]\n\n- Pending.\n\n{SECTION}\n## [0.9.0] - 2026-01-01\n\n- Old.\n\n[0.9.0]: https://example.com/0.9.0\n"
        )
    );
}

#[test]
fn inserts_before_the_first_release_without_unreleased() {
    let existing =
        "# Changelog\n\nIntro.\n\n## [0.9.0] - 2026-01-01\n\n- Old.\n\n## [0.8.0]\n\n- Older.\n";
    assert_eq!(
        inserted(existing),
        format!(
            "# Changelog\n\nIntro.\n\n{SECTION}\n## [0.9.0] - 2026-01-01\n\n- Old.\n\n## [0.8.0]\n\n- Older.\n"
        )
    );
}

#[test]
fn inserts_at_the_top_of_a_file_that_starts_with_a_release() {
    assert_eq!(
        inserted("## [0.9.0]\n\n- Old.\n"),
        format!("{SECTION}\n## [0.9.0]\n\n- Old.\n")
    );
}

#[test]
fn a_file_without_a_final_newline_gains_one() {
    assert_eq!(
        inserted("# Changelog\n\n## [0.9.0]\n\n- Old."),
        format!("# Changelog\n\n{SECTION}\n## [0.9.0]\n\n- Old.\n")
    );
}

#[test]
fn headings_inside_fences_are_not_headings() {
    let existing =
        "# Changelog\n\n```\n## [Unreleased]\n## [1.0.0]\n```\n\n~~~md\n## [0.1.0]\n~~~\n\nText.\n";
    let found = target::headings(existing, "## ");
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(inserted(existing), format!("{existing}\n{SECTION}"));
}

#[test]
fn version_present_with_and_without_a_date() {
    for heading in ["[1.0.0]", "[1.0.0] - 2026-01-01", "[1.0.0] - yanked"] {
        assert!(names_version(heading, "1.0.0"), "{heading}");
    }
    for heading in [
        "[1.0.0-rc1]",
        "[1.0.0]x",
        "1.0.0",
        "[v1.0.0]",
        "[Unreleased]",
        "[1.0.0",
    ] {
        assert!(!names_version(heading, "1.0.0"), "{heading}");
    }
}

#[test]
fn version_values() {
    for good in ["1.0.0", "v1.2.0", "2026.10", "1.0.0-rc.1+build"] {
        assert_eq!(parse_version(good).as_deref(), Ok(good));
    }
    for bad in ["", "1.0 .0", "1.0\t", "[1.0]", "1]", " 1"] {
        assert!(parse_version(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn date_values() {
    for good in ["2026-10-23", "2024-02-29", "0001-01-01"] {
        assert_eq!(parse_date(good).as_deref(), Ok(good));
    }
    for bad in [
        "2026-02-30",
        "2025-02-29",
        "2026-13-01",
        "2026-1-01",
        "26-10-23",
        "2026/10/23",
        "2026-10-23T00:00",
        "+2026-10-23",
        "",
    ] {
        assert!(parse_date(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn today_has_the_date_shape() {
    let today = today();
    assert_eq!(parse_date(&today).as_deref(), Ok(today.as_str()));
}

/// A tempdir with the sample fragments on disk and a changelog path in it.
fn on_disk(changelog: Option<&str>) -> (tempfile::TempDir, Scan<Changelog>, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("changelog.d");
    fs::create_dir(&dir).unwrap();
    let scan = sample(&dir);
    for f in &scan.fragments {
        fs::write(&f.path, f.body.join("\n") + "\n").unwrap();
    }
    fs::write(dir.join(".gitkeep"), "").unwrap();
    let path = tmp.path().join("CHANGELOG.md");
    if let Some(text) = changelog {
        fs::write(&path, text).unwrap();
    }
    (tmp, scan, path)
}

fn release(changelog: &Path, dry_run: bool) -> Release<'_> {
    Release {
        changelog,
        version: "1.0.0",
        date: "2026-10-23",
        dry_run,
    }
}

#[test]
fn gather_writes_the_section_and_deletes_the_fragments() {
    let (tmp, scan, path) = on_disk(Some("# Changelog\n\n## [0.9.0]\n\n- Old.\n"));
    let outcome = gather(&scan, &release(&path, false)).unwrap();
    let expected: Vec<PathBuf> = scan.fragments.iter().map(|f| f.path.clone()).collect();
    assert_eq!(
        outcome,
        Outcome::Gathered {
            created: false,
            rendered: SECTION.to_owned(),
            fragments: expected.clone(),
        }
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("# Changelog\n\n{SECTION}\n## [0.9.0]\n\n- Old.\n")
    );
    for p in expected {
        assert!(!p.exists(), "{}", p.display());
    }
    let dir = tmp.path().join("changelog.d");
    assert!(dir.join(".gitkeep").exists());
    assert!(!tmp.path().join("CHANGELOG.md.tmp").exists());
}

#[test]
fn gather_creates_an_absent_changelog() {
    let (_tmp, scan, path) = on_disk(None);
    let outcome = gather(&scan, &release(&path, false)).unwrap();
    assert!(matches!(outcome, Outcome::Gathered { created: true, .. }));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("{HEADER}\n{SECTION}")
    );
}

#[test]
fn dry_run_writes_and_deletes_nothing() {
    let (_tmp, scan, path) = on_disk(None);
    let outcome = gather(&scan, &release(&path, true)).unwrap();
    assert!(matches!(
        &outcome,
        Outcome::Gathered { created: true, rendered, .. } if rendered == SECTION
    ));
    assert!(!path.exists());
    for f in &scan.fragments {
        assert!(f.path.exists(), "{}", f.path.display());
    }
}

#[test]
fn refusals_write_nothing() {
    let existing = "# Changelog\n\n## [1.0.0] - 2026-01-01\n\n- Done.\n";
    let (_tmp, scan, path) = on_disk(Some(existing));
    assert_eq!(
        gather(&scan, &release(&path, false)).unwrap(),
        Outcome::Refused(Refusal::VersionPresent)
    );
    assert_eq!(
        gather(&Scan::<Changelog>::default(), &release(&path, false)).unwrap(),
        Outcome::Refused(Refusal::NothingToGather)
    );
    let mut with_problem = scan.clone();
    with_problem.problems.push(
        crate::fragment::Finding::new(crate::fragment::ProblemCode::Empty, "empty")
            .at("changelog.d/e.added.md"),
    );
    assert_eq!(
        gather(&with_problem, &release(&path, false)).unwrap(),
        Outcome::Refused(Refusal::Problems)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), existing);
    for f in &scan.fragments {
        assert!(f.path.exists(), "{}", f.path.display());
    }
}

#[test]
fn crlf_and_non_utf8_changelogs_are_errors() {
    for bytes in [&b"# Changelog\r\n"[..], b"# Changelog \xff\n"] {
        let (_tmp, scan, path) = on_disk(None);
        fs::write(&path, bytes).unwrap();
        let err = gather(&scan, &release(&path, false)).unwrap_err();
        assert!(matches!(err, Error::Target { .. }), "{err}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn a_failed_deletion_names_what_remains() {
    let (_tmp, scan, path) = on_disk(None);
    let gone = scan.fragments[1].path.clone();
    fs::remove_file(&gone).unwrap();
    let err = gather(&scan, &release(&path, false)).unwrap_err();
    let Error::Delete {
        deleted, remaining, ..
    } = &err
    else {
        panic!("{err}");
    };
    assert_eq!(deleted, &[scan.fragments[0].path.clone()]);
    assert_eq!(remaining, &[gone.clone(), scan.fragments[2].path.clone()]);
    let message = err.to_string();
    assert!(
        message.contains("holds the gathered fragments"),
        "{message}"
    );
    assert!(message.contains(&gone.display().to_string()), "{message}");
    assert!(fs::read_to_string(&path).unwrap().contains(SECTION));
    assert!(scan.fragments[2].path.exists());
}

#[test]
fn a_symlinked_changelog_is_an_error() {
    let (tmp, scan, path) = on_disk(None);
    let target = tmp.path().join("docs.md");
    fs::write(&target, "# Changelog\n").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let err = gather(&scan, &release(&path, false)).unwrap_err();
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
    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "# Changelog\n");
    for f in &scan.fragments {
        assert!(f.path.exists(), "{}", f.path.display());
    }
}

#[test]
fn an_existing_temporary_path_is_an_error_and_left_alone() {
    let (tmp, scan, path) = on_disk(Some("# Changelog\n"));
    let victim = tmp.path().join("victim.txt");
    fs::write(&victim, "keep\n").unwrap();
    let planted = tmp.path().join("CHANGELOG.md.tmp");
    std::os::unix::fs::symlink(&victim, &planted).unwrap();
    let err = gather(&scan, &release(&path, false)).unwrap_err();
    assert!(
        matches!(&err, Error::Io { path, source } if *path == planted
            && source.kind() == std::io::ErrorKind::AlreadyExists),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&victim).unwrap(), "keep\n");
    assert!(fs::symlink_metadata(&planted).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(&path).unwrap(), "# Changelog\n");
    for f in &scan.fragments {
        assert!(f.path.exists(), "{}", f.path.display());
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
