use std::path::{Path, PathBuf};

use super::*;
use crate::changelog::{FragmentName, Outcome, Release};
use crate::fragment::{Finding, ProblemCode};

fn fragment(file_name: &str, body: &[&str]) -> Fragment<Changelog> {
    Fragment {
        path: PathBuf::from("changelog.d").join(file_name),
        name: FragmentName::parse(file_name).unwrap(),
        body: body.iter().map(|l| l.to_string()).collect(),
    }
}

fn sample() -> Scan<Changelog> {
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

fn gathered(scan: &Scan<Changelog>) -> Outcome {
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

// --- The TODO kind ---------------------------------------------------------

fn todo_fragment(slug: &str, body: &[&str]) -> Fragment<Todo> {
    Fragment {
        path: PathBuf::from("todo.d").join(format!("{slug}.md")),
        name: todo::TodoName { slug: slug.into() },
        body: body.iter().map(|l| l.to_string()).collect(),
    }
}

fn todo_sample() -> Scan<Todo> {
    Scan {
        fragments: vec![
            todo_fragment("audit", &["Audit the graph.", "", "Second paragraph."]),
            todo_fragment("loops", &["Detect loops."]),
        ],
        problems: vec![
            Finding::new(
                ProblemCode::BadName,
                "expected <slug>.md, where the slug is letters, digits, '-' and '_'",
            )
            .at("todo.d/bad slug.md"),
        ],
    }
}

#[test]
fn todo_query_json_has_the_schema_fields_in_order() {
    let scan = todo_sample();
    let report = TodoQueryReport::new(Some(Path::new("todo.d")), None, &scan);
    let json = report.json();
    assert!(json.ends_with("}\n"), "{json}");
    let keys = ["dir", "range", "fragments", "problems", "ok"];
    let at = |key: &str| json.find(&format!("\"{key}\":")).unwrap();
    assert!(keys.windows(2).all(|w| at(w[0]) < at(w[1])), "{json}");
    let fragment_keys = ["path", "slug", "body"];
    assert!(
        fragment_keys.windows(2).all(|w| at(w[0]) < at(w[1])),
        "{json}"
    );
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value.as_object().unwrap().len(), keys.len());
    assert_eq!(
        value["fragments"][1],
        serde_json::json!({
            "path": "todo.d/loops.md",
            "slug": "loops",
            "body": ["Detect loops."],
        })
    );
    assert_eq!(
        value["fragments"][0].as_object().unwrap().len(),
        fragment_keys.len()
    );
    assert_eq!(value["problems"][0]["code"], "bad-name");
    assert_eq!(value["ok"], false);
}

#[test]
fn todo_query_ok_is_no_problems() {
    let clean = Scan {
        fragments: todo_sample().fragments,
        problems: vec![],
    };
    let dir = Some(Path::new("todo.d"));
    assert!(TodoQueryReport::new(dir, None, &clean).ok);
    assert!(TodoQueryReport::new(dir, None, &Scan::default()).ok);
    assert!(!TodoQueryReport::new(dir, None, &todo_sample()).ok);
}

#[test]
fn todo_query_human_listing() {
    let scan = todo_sample();
    let report = TodoQueryReport::new(Some(Path::new("todo.d")), None, &scan);
    assert_eq!(
        report.human(),
        "todo.d: 2 fragments, 1 problem\n\
         \n\
         ## `audit`\n\
         \n\
         Audit the graph.\n\
         \n\
         Second paragraph.\n\
         \n\
         ## `loops`\n\
         \n\
         Detect loops.\n\
         \n\
         Problems:\n\
         todo.d/bad slug.md: bad-name: expected <slug>.md, where the slug is letters, digits, '-' and '_'\n"
    );
}

#[test]
fn todo_query_headers() {
    let scan = Scan::default();
    assert_eq!(
        TodoQueryReport::new(None, None, &scan).human(),
        "no todo.d or .todo.d: 0 fragments\n"
    );
    assert_eq!(
        TodoQueryReport::new(Some(Path::new(".todo.d")), Some("A..B"), &scan).human(),
        ".todo.d: 0 fragments added in A..B, 0 problems\n"
    );
}

fn todo_target() -> todo::Target<'static> {
    todo::Target {
        todo: Path::new("TODO.md"),
        dry_run: false,
    }
}

fn todo_gathered(scan: &Scan<Todo>, created: bool) -> todo::Outcome {
    todo::Outcome::Gathered {
        created,
        rendered: "## `loops`\n\nDetect loops.\n".into(),
        fragments: scan.fragments.iter().map(|f| f.path.clone()).collect(),
    }
}

fn present(slug: &str) -> Problem {
    Finding::new(
        ProblemCode::Present,
        format!("TODO.md already has an entry `{slug}` at line 3"),
    )
    .at(format!("todo.d/{slug}.md"))
}

#[test]
fn todo_gather_json_has_the_schema_fields_in_order() {
    let scan = Scan {
        fragments: vec![todo_fragment("loops", &["Detect loops."])],
        problems: Vec::new(),
    };
    let outcome = todo_gathered(&scan, false);
    let json = TodoGatherReport::new(&todo_target(), &scan, &outcome).json();
    assert!(json.ends_with("}\n"), "{json}");
    let keys = [
        "todo", "created", "dry_run", "gathered", "rendered", "problems", "refused", "ok",
    ];
    let at = |key: &str| json.find(&format!("\"{key}\":")).unwrap();
    assert!(keys.windows(2).all(|w| at(w[0]) < at(w[1])), "{json}");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value.as_object().unwrap().len(), keys.len());
    assert_eq!(
        value,
        serde_json::json!({
            "todo": "TODO.md",
            "created": false,
            "dry_run": false,
            "gathered": ["todo.d/loops.md"],
            "rendered": "## `loops`\n\nDetect loops.\n",
            "problems": [],
            "refused": null,
            "ok": true,
        })
    );
}

#[test]
fn todo_gather_lists_present_after_the_scans_problems() {
    let scan = todo_sample();
    let outcome = todo::Outcome::Refused {
        refusal: Refusal::Problems,
        present: vec![present("audit")],
    };
    let report = TodoGatherReport::new(&todo_target(), &scan, &outcome);
    let value: serde_json::Value = serde_json::from_str(&report.json()).unwrap();
    assert_eq!(value["refused"], "problems");
    assert_eq!(value["ok"], false);
    assert_eq!(value["rendered"], serde_json::Value::Null);
    assert_eq!(value["gathered"], serde_json::json!([]));
    let codes: Vec<&str> = value["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["bad-name", "present"]);
}

#[test]
fn todo_gather_human_forms() {
    let none = Scan::default();
    let refused = todo::Outcome::Refused {
        refusal: Refusal::Problems,
        present: vec![present("audit")],
    };
    assert_eq!(
        TodoGatherReport::new(&todo_target(), &none, &refused).human(),
        "nothing gathered: 1 problem in the fragments\n\
         \n\
         Problems:\n\
         todo.d/audit.md: present: TODO.md already has an entry `audit` at line 3\n"
    );
    let nothing = todo::Outcome::Refused {
        refusal: Refusal::NothingToGather,
        present: Vec::new(),
    };
    let report = TodoGatherReport::new(&todo_target(), &none, &nothing);
    assert_eq!(report.human(), "nothing gathered: no fragments to gather\n");
    assert_eq!(report.refused, Some(Refusal::NothingToGather));
    assert!(report.problems.is_empty());

    let scan = Scan {
        fragments: vec![todo_fragment("loops", &["Detect loops."])],
        problems: Vec::new(),
    };
    let dry = todo::Target {
        dry_run: true,
        ..todo_target()
    };
    let created = todo_gathered(&scan, true);
    assert_eq!(
        TodoGatherReport::new(&dry, &scan, &created).human(),
        "dry run: would write to TODO.md (created):\n\n\
         ## `loops`\n\nDetect loops.\n"
    );
    let outcome = todo_gathered(&scan, false);
    assert_eq!(
        TodoGatherReport::new(&todo_target(), &scan, &outcome).human(),
        "gathered 1 fragment into TODO.md\n\
         deleted todo.d/loops.md\n"
    );
    assert_eq!(
        TodoGatherReport::new(&todo_target(), &scan, &created).human(),
        "gathered 1 fragment into TODO.md (created)\n\
         deleted todo.d/loops.md\n"
    );
}
