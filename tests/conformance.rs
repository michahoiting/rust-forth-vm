// Copyright (c) 2026 M.W.R. Hoiting

//! Standard-attributed public-API measurements and tests of their regression/reporting machinery.

#[path = "conformance/cases.rs"]
mod cases;
#[path = "conformance/model.rs"]
mod model;
#[cfg(not(miri))]
#[path = "conformance/report.rs"]
mod report;

use model::{Outcome, Requirement, Status};
use rust_forth_vm::Cell;

/// The versioned, source-attributed requirements register.
const REGISTER: &str = include_str!("conformance/requirements.tsv");
/// All reviewed outcomes, including passing checks and known deficiencies.
const BASELINE: &str = if Cell::BITS == 32 {
    include_str!("conformance/baseline-32.tsv")
} else {
    include_str!("conformance/baseline.tsv")
};

/// Measures once for a test or a deliberate artifact refresh.
fn measurement() -> (Vec<Outcome>, std::collections::BTreeSet<String>) {
    let register = model::parse_register(REGISTER).unwrap();
    let words = model::available_words();
    let outcomes = model::measure(&register, &cases::cases(), &words).unwrap();
    (outcomes, words)
}

#[test]
fn conformance_matches_reviewed_baseline() {
    let (outcomes, _) = measurement();
    for profile in ["f83", "iso"] {
        let rows: Vec<_> = outcomes
            .iter()
            .filter(|o| o.requirement.profile == profile)
            .collect();
        let passed = rows.iter().filter(|o| o.status == Status::Passed).count();
        println!(
            "{}-bit {}-endian {profile}: {passed}/{} mandatory units, {}",
            Cell::BITS,
            if cfg!(target_endian = "little") {
                "little"
            } else {
                "big"
            },
            rows.len(),
            model::percentage(passed, rows.len()).unwrap()
        );
    }
    for case in cases::cases() {
        assert!(
            outcomes.iter().any(|row| row.checks.contains(&case.id)),
            "unreferenced probe: {}",
            case.id
        );
    }
    model::check_baseline(&outcomes, BASELINE).unwrap();
}

#[test]
fn register_accounts_for_both_required_word_sets() {
    let register = model::parse_register(REGISTER).unwrap();
    // Counts were checked against chapter 12 and DPANS94 section 6.1, not inferred from the VM.
    for (profile, count) in [("f83", 132), ("iso", 133)] {
        assert_eq!(
            register
                .iter()
                .filter(|r| r.profile == profile && r.kind == "word")
                .count(),
            count
        );
        assert!(register
            .iter()
            .any(|r| r.profile == profile && r.kind == "system"));
    }
    for requirement in register {
        assert!(!requirement.provision.is_empty());
        assert!(requirement.source.starts_with("https://"));
    }
}

#[test]
fn both_architecture_baselines_account_for_every_requirement() {
    let register = model::parse_register(REGISTER).unwrap();
    let expected: std::collections::BTreeSet<_> = register.iter().map(|r| r.id.as_str()).collect();
    for baseline in [
        include_str!("conformance/baseline.tsv"),
        include_str!("conformance/baseline-32.tsv"),
    ] {
        let mut ids = std::collections::BTreeSet::new();
        for row in baseline.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 7, "malformed baseline: {row}");
            assert!(
                ids.insert(fields[0]),
                "duplicate baseline ID: {}",
                fields[0]
            );
            assert!(matches!(
                fields[1],
                "passed" | "deviating" | "missing" | "blocked" | "not yet assessed"
            ));
            assert!(!fields[6].is_empty(), "missing evidence: {row}");
        }
        assert_eq!(ids, expected);
    }
}

#[test]
fn register_rejects_empty_duplicate_and_malformed_rows() {
    assert!(model::parse_register("").is_err());
    assert!(model::parse_register("incomplete\trow").is_err());
    assert!(
        model::parse_register(&format!("{REGISTER}\n{}", REGISTER.lines().nth(1).unwrap()))
            .is_err()
    );
    assert!(model::parse_register(&REGISTER.replacen("https://", "http://", 1)).is_err());
    assert!(model::parse_register(&REGISTER.replace("\tf83\t", "\tunknown\t")).is_err());
}

#[test]
fn percentages_include_all_nonpassing_units_and_round_to_tenths() {
    assert_eq!(model::percentage(0, 10).unwrap(), "0.0%");
    assert_eq!(model::percentage(1, 3).unwrap(), "33.3%");
    assert_eq!(model::percentage(2, 3).unwrap(), "66.7%");
    assert_eq!(model::percentage(1, 16).unwrap(), "6.3%");
    assert_eq!(model::percentage(10, 10).unwrap(), "100.0%");
    assert!(model::percentage(0, 0).is_err());
    assert!(model::percentage(11, 10).is_err());
}

/// A small register row used to test the measurement machinery independently of the catalogue.
fn fixture(subject: &str, assessment: &str) -> Requirement {
    Requirement {
        id: format!("fixture.{subject}"),
        profile: "iso".into(),
        kind: if assessment == "word" {
            "word"
        } else {
            "system"
        }
        .into(),
        subject: subject.into(),
        provision: "fixture".into(),
        source: "https://example.invalid".into(),
        assessment: assessment.into(),
        reason: "Documentary audit is incomplete.".into(),
    }
}

#[test]
fn absent_words_and_missing_test_facilities_are_distinct() {
    let words = model::available_words();
    let register = vec![
        fixture("ABSENT", "word"),
        fixture("DUP", "word"),
        fixture("documentation", "review"),
    ];
    let rows = model::measure(&register, &[], &words).unwrap();
    assert_eq!(rows[0].status, Status::Missing);
    assert_eq!(rows[1].status, Status::Blocked);
    assert!(rows[1].reason.contains("behavioral probe"));
    assert_eq!(rows[2].status, Status::Unassessed);
    assert!(rows[2].reason.contains("incomplete"));
}

#[test]
fn missing_prerequisites_block_probes_instead_of_passing_them() {
    let words = model::available_words();
    let register = vec![fixture("DUP", "word"), fixture("NEEDS", "word")];
    let mut case = cases::cases().into_iter().find(|c| c.id == "dup").unwrap();
    case.source = "NEEDS 5 DUP".into();
    let rows = model::measure(&register, &[case], &words).unwrap();
    assert_eq!(rows[0].status, Status::Blocked);
    assert_eq!(rows[0].dependencies, ["NEEDS"]);
    assert!(rows[0].failed.is_empty());
}

#[test]
fn changed_values_and_stack_effects_are_measured_as_deviations() {
    let words = model::available_words();
    let register = vec![fixture("DUP", "word")];
    let mut case = cases::cases().into_iter().find(|c| c.id == "dup").unwrap();
    for source in ["5 6", "5", "5 5 5", "DROP 5 5"] {
        case.source = source.into();
        let rows = model::measure(&register, &[case.clone()], &words).unwrap();
        assert_eq!(rows[0].status, Status::Deviating, "{source}");
        assert_eq!(rows[0].failed, ["dup"]);
    }
}

#[test]
fn immediate_probe_distinguishes_compilation_from_execution() {
    let mut case = cases::cases()
        .into_iter()
        .find(|case| case.id == "immediate")
        .unwrap();
    cases::run(&case).unwrap();
    case.source = case.source.replace("IMMEDIATE", "");
    assert!(cases::run(&case).is_err());
}

#[test]
fn duplicate_and_removed_probe_ids_are_errors() {
    let words = model::available_words();
    let register = vec![fixture("DUP", "word")];
    let case = cases::cases().into_iter().find(|c| c.id == "dup").unwrap();
    assert!(model::measure(&register, &[case.clone(), case], &words).is_err());
    assert!(model::measure(&[fixture("environment", "compiler")], &[], &words).is_err());
}

#[test]
fn baseline_rejects_regressions_improvements_removed_checks_and_stale_entries() {
    let words = model::available_words();
    let rows = model::measure(&[fixture("DUP", "word")], &cases::cases(), &words).unwrap();
    let expected = model::baseline(&rows);
    assert!(model::check_baseline(&rows, &expected).is_ok());
    assert!(model::check_baseline(&rows, &expected.replace('\n', "\r\n")).is_ok());
    let mut changed = rows.clone();
    changed[0].status = Status::Deviating;
    assert!(model::check_baseline(&changed, &expected).is_err());
    assert!(model::check_baseline(&rows, &model::baseline(&changed)).is_err());
    changed = rows.clone();
    changed[0].checks.clear();
    assert!(model::check_baseline(&changed, &expected).is_err());
    changed = rows.clone();
    changed[0].reason = "different observed result".into();
    assert!(model::check_baseline(&changed, &expected).is_err());
    changed = rows.clone();
    changed[0].signatures[0].push_str("changed probe");
    assert!(model::check_baseline(&changed, &expected).is_err());
    assert!(model::check_baseline(&[], &expected).is_err());
    assert!(model::check_baseline(&rows, "").is_err());
}

#[cfg(not(miri))]
#[test]
fn generated_conformance_files_are_current() {
    if !report::canonical() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let (outcomes, words) = measurement();
    model::check_baseline(&outcomes, BASELINE).unwrap();
    let artifacts = report::artifacts(&outcomes, &words, &report::source_revision(root));
    report::check_files(root, &artifacts).unwrap();
}

#[cfg(not(miri))]
#[test]
fn badges_links_and_reports_share_the_measured_results() {
    let (rows, words) = measurement();
    let files = report::artifacts(&rows, &words, "fixture-source");
    assert_eq!(files, report::artifacts(&rows, &words, "fixture-source"));
    assert_eq!(files.len(), 3);
    let markdown = &files["docs/conformance/report.md"];
    assert!(markdown.contains("## FORTH-83"));
    assert!(markdown.contains("## ISO Forth"));
    assert!(markdown.contains("word / `#>`"));
    assert!(!markdown.contains("word / `#&gt;`"));
    for (profile, label) in [
        ("f83", "FORTH-83 conformance"),
        ("iso", "ISO Forth conformance"),
    ] {
        let subset: Vec<_> = rows
            .iter()
            .filter(|r| r.requirement.profile == profile)
            .collect();
        let score = model::percentage(
            subset.iter().filter(|r| r.status == Status::Passed).count(),
            subset.len(),
        )
        .unwrap();
        assert!(markdown.contains(&format!("**{score}")));
        assert!(files[&format!("docs/conformance/{profile}.svg")]
            .contains(&format!("<title>{label}: {score}</title>")));
    }
    let readme = include_str!("../README.md");
    for link in [
        "docs/conformance/f83.svg",
        "docs/conformance/iso.svg",
        "docs/conformance/report.md#forth-83",
        "docs/conformance/report.md#iso-forth",
        "https://www.iso.org/standard/26479.html",
        "https://www.complang.tuwien.ac.at/forth/fth83std/FORTH83.TXT",
        "https://www.taygeta.com/forth/dpans6.htm#6.1",
        "https://www.taygeta.com/forth/dpans3.htm",
    ] {
        assert!(readme.contains(link), "missing README link: {link}");
    }
}

#[cfg(not(miri))]
#[test]
fn freshness_checks_reject_missing_and_stale_files() {
    let root = std::env::temp_dir().join(format!(
        "forth-conformance-freshness-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let files = std::collections::BTreeMap::from([("report.md".into(), "measured\n".into())]);
    assert!(report::check_files(&root, &files).is_err());
    std::fs::write(root.join("report.md"), "stale").unwrap();
    assert!(report::check_files(&root, &files).is_err());
    std::fs::write(root.join("report.md"), "measured\n").unwrap();
    assert!(report::check_files(&root, &files).is_ok());
    std::fs::write(root.join("report.md"), "measured\r\n").unwrap();
    assert!(report::check_files(&root, &files).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(not(miri))]
#[test]
fn source_fingerprints_ignore_checkout_line_endings_but_detect_new_inputs() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = std::env::temp_dir().join(format!(
        "forth-conformance-fingerprint-{}",
        std::process::id()
    ));
    let paths = report::source_paths(project);
    for path in &paths {
        let destination = root.join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let text = std::fs::read_to_string(project.join(path))
            .unwrap()
            .replace("\r\n", "\n");
        std::fs::write(destination, text).unwrap();
    }
    let original = report::source_revision(&root);
    assert_eq!(original, report::source_revision(project));
    for path in &paths {
        let text = std::fs::read_to_string(root.join(path))
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(root.join(path), text).unwrap();
    }
    assert_eq!(original, report::source_revision(&root));
    std::fs::write(
        root.join("src/conformance_extra.rs"),
        "// Copyright (c) 2026 M.W.R. Hoiting\n\n// Additional source input.\n",
    )
    .unwrap();
    assert_ne!(original, report::source_revision(&root));
    std::fs::remove_dir_all(root).unwrap();
}

/// Explicit maintenance command; ordinary tests never modify tracked files.
#[cfg(not(miri))]
#[test]
#[ignore = "run only through make conformance or make conformance-baseline"]
fn regenerate() {
    assert!(
        report::canonical(),
        "generate README artifacts only on a 64-bit little-endian target"
    );
    let mode = std::env::var("CONFORMANCE_WRITE").expect("use the Makefile maintenance target");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let (outcomes, words) = measurement();
    if mode == "baseline" {
        std::fs::write(
            root.join("tests/conformance/baseline.tsv"),
            model::baseline(&outcomes),
        )
        .unwrap();
    } else {
        assert_eq!(mode, "artifacts");
        model::check_baseline(&outcomes, BASELINE).unwrap();
    }
    for (path, content) in report::artifacts(&outcomes, &words, &report::source_revision(root)) {
        std::fs::write(root.join(path), content).unwrap();
    }
}

/// Prints a separately measured baseline without writing from inside Miri's isolation.
#[test]
#[ignore = "used by make conformance-baseline-32 to print measurements for review"]
fn print_baseline() {
    let (outcomes, _) = measurement();
    for line in model::baseline(&outcomes).lines() {
        println!("CONFORMANCE-BASELINE:{line}");
    }
}
