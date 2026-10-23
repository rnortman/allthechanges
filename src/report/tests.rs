use std::path::{Path, PathBuf};

use super::*;
use crate::changelog::{Outcome, Refusal, Release};
use crate::fragment::{Finding, FragmentName, ProblemCode};

fn fragment(file_name: &str, body: &[&str]) -> Fragment {
    Fragment {
        path: PathBuf::from("changelog.d").join(file_name),
        name: FragmentName::parse(file_name).unwrap(),
        body: body.iter().map(|l| l.to_string()).collect(),
    }
}

fn sample() -> Scan {
    Scan {
        fragments: vec![
            fragment(
                "2026-10-23.login.added.md",
                &["- Log in.", "  Continuation."],
            ),
            fragment("z.added.md", &["- Zed."]),
            fragment("crash.fixed.md", &["- No crash."]),
        ],
        problems: vec![
            Finding::new(ProblemCode::UnknownSection, "\"fxied\" is not a section")
                .at("changelog.d/typo.fxied.md"),
        ],
    }
}

#[test]
fn query_json_has_the_schema_fields() {
    let scan = sample();
    let report = QueryReport::new(Some(Path::new("changelog.d")), None, false, &scan);
    let json = report.json();
    assert!(json.ends_with("}\n"), "{json}");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 5);
    assert_eq!(value["dir"], "changelog.d");
    assert_eq!(value["range"], serde_json::Value::Null);
    assert_eq!(value["ok"], false);
    assert_eq!(
        value["fragments"][0],
        serde_json::json!({
            "path": "changelog.d/2026-10-23.login.added.md",
            "date": "2026-10-23",
            "slug": "login",
            "section": "added",
            "body": ["- Log in.", "  Continuation."],
        })
    );
    assert_eq!(value["fragments"][1]["date"], serde_json::Value::Null);
    assert_eq!(
        value["problems"][0],
        serde_json::json!({
            "path": "changelog.d/typo.fxied.md",
            "code": "unknown-section",
            "message": "\"fxied\" is not a section",
        })
    );
}

#[test]
fn query_json_key_order_is_the_schema_order() {
    let scan = sample();
    let json = QueryReport::new(Some(Path::new("changelog.d")), None, false, &scan).json();
    let at = |key: &str| json.find(&format!("\"{key}\":")).unwrap();
    assert!(at("dir") < at("range"));
    assert!(at("range") < at("fragments"));
    assert!(at("fragments") < at("problems"));
    assert!(at("problems") < at("ok"));
}

#[test]
fn ok_needs_no_problems_and_under_require_a_fragment() {
    let clean = Scan {
        fragments: sample().fragments,
        problems: vec![],
    };
    let empty = Scan::default();
    let dir = Some(Path::new("changelog.d"));
    assert!(QueryReport::new(dir, None, false, &clean).ok);
    assert!(QueryReport::new(dir, None, true, &clean).ok);
    assert!(QueryReport::new(dir, None, false, &empty).ok);
    assert!(!QueryReport::new(dir, None, true, &empty).ok);
    assert!(!QueryReport::new(dir, None, false, &sample()).ok);
}

#[test]
fn human_listing() {
    let scan = sample();
    let report = QueryReport::new(Some(Path::new("changelog.d")), None, false, &scan);
    assert_eq!(
        report.human(),
        "changelog.d: 3 fragments, 1 problem\n\
         \n\
         ### Added\n\
         \n\
         - Log in.  [2026-10-23.login.added.md]\n  Continuation.\n\
         - Zed.  [z.added.md]\n\
         \n\
         ### Fixed\n\
         \n\
         - No crash.  [crash.fixed.md]\n\
         \n\
         Problems:\n\
         changelog.d/typo.fxied.md: unknown-section: \"fxied\" is not a section\n"
    );
}

#[test]
fn human_range_header() {
    let scan = Scan {
        fragments: vec![fragment("a.added.md", &["- A."])],
        problems: vec![],
    };
    let report = QueryReport::new(
        Some(Path::new("changelog.d")),
        Some("origin/main...HEAD"),
        true,
        &scan,
    );
    assert_eq!(
        report.human().lines().next().unwrap(),
        "changelog.d: 1 fragment added in origin/main...HEAD, 0 problems"
    );
}

#[test]
fn human_without_a_directory() {
    let scan = Scan::default();
    let report = QueryReport::new(None, None, false, &scan);
    assert_eq!(
        report.human(),
        "no changelog.d or .changelog.d: 0 fragments\n"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&report.json()).unwrap()["dir"],
        serde_json::Value::Null
    );
}

#[test]
fn human_says_why_require_failed() {
    let scan = Scan::default();
    let dir = Some(Path::new("changelog.d"));
    assert_eq!(
        QueryReport::new(dir, Some("A..B"), true, &scan).human(),
        "changelog.d: 0 fragments added in A..B, 0 problems\n\
         \n\
         --require: no fragment added in A..B\n"
    );
    assert!(
        QueryReport::new(dir, None, true, &scan)
            .human()
            .ends_with("\n--require: no fragment present\n")
    );
}

fn gather_release() -> Release<'static> {
    Release {
        changelog: Path::new("CHANGELOG.md"),
        version: "1.0.0",
        date: "2026-10-23",
        dry_run: false,
    }
}

fn gathered(scan: &Scan) -> Outcome {
    Outcome::Gathered {
        created: false,
        rendered: "## [1.0.0] - 2026-10-23\n\n### Added\n\n- Zed.\n".into(),
        fragments: scan.fragments.iter().map(|f| f.path.clone()).collect(),
    }
}

#[test]
fn gather_json_has_the_schema_fields_in_order() {
    let scan = Scan {
        fragments: vec![fragment("z.added.md", &["- Zed."])],
        problems: Vec::new(),
    };
    let outcome = gathered(&scan);
    let json = GatherReport::new(&gather_release(), &scan, &outcome).json();
    assert!(json.ends_with("}\n"), "{json}");
    let keys = [
        "changelog",
        "created",
        "version",
        "date",
        "dry_run",
        "gathered",
        "rendered",
        "problems",
        "refused",
        "ok",
    ];
    let at = |key: &str| json.find(&format!("\"{key}\":")).unwrap();
    assert!(keys.windows(2).all(|w| at(w[0]) < at(w[1])), "{json}");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value.as_object().unwrap().len(), keys.len());
    assert_eq!(
        value["gathered"],
        serde_json::json!(["changelog.d/z.added.md"])
    );
    assert_eq!(value["refused"], serde_json::Value::Null);
    assert_eq!(value["ok"], true);
}

#[test]
fn gather_refusals_report_the_reason() {
    let scan = sample();
    for (refusal, name, problems) in [
        (Refusal::Problems, "problems", 1),
        (Refusal::NothingToGather, "nothing-to-gather", 0),
        (Refusal::VersionPresent, "version-present", 0),
    ] {
        let outcome = Outcome::Refused(refusal);
        let report = GatherReport::new(&gather_release(), &scan, &outcome);
        let value: serde_json::Value = serde_json::from_str(&report.json()).unwrap();
        assert_eq!(value["refused"], name);
        assert_eq!(value["ok"], false);
        assert_eq!(value["rendered"], serde_json::Value::Null);
        assert_eq!(value["gathered"], serde_json::json!([]));
        assert_eq!(
            value["problems"].as_array().unwrap().len(),
            problems,
            "{name}"
        );
        assert!(report.human().starts_with("nothing gathered: "), "{name}");
    }
}

#[test]
fn gather_human_forms() {
    let scan = Scan {
        fragments: vec![fragment("z.added.md", &["- Zed."])],
        problems: Vec::new(),
    };
    let outcome = gathered(&scan);
    assert_eq!(
        GatherReport::new(&gather_release(), &scan, &outcome).human(),
        "gathered 1 fragment into CHANGELOG.md as [1.0.0] - 2026-10-23\n\
         deleted changelog.d/z.added.md\n"
    );
    let dry = Release {
        dry_run: true,
        ..gather_release()
    };
    assert_eq!(
        GatherReport::new(&dry, &scan, &outcome).human(),
        "dry run: would write to CHANGELOG.md:\n\n\
         ## [1.0.0] - 2026-10-23\n\n### Added\n\n- Zed.\n"
    );
    let problems = sample();
    let refused = Outcome::Refused(Refusal::Problems);
    assert_eq!(
        GatherReport::new(&gather_release(), &problems, &refused).human(),
        "nothing gathered: 1 problem in the fragments\n\
         \n\
         Problems:\n\
         changelog.d/typo.fxied.md: unknown-section: \"fxied\" is not a section\n"
    );
}
