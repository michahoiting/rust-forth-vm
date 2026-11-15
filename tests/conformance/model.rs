// Copyright (c) 2026 M.W.R. Hoiting

//! Requirements, measured classifications, and a strict reviewed outcome baseline.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use rust_forth_vm::{boot, Cell, VmConfig};

use super::cases::{self, Case};

/// A requirement has exactly one classification, regardless of its number of probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Passed,
    Deviating,
    Missing,
    Blocked,
    Unassessed,
}

impl Status {
    /// The stable machine-readable spelling used in reports and baselines.
    pub fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Deviating => "deviating",
            Self::Missing => "missing",
            Self::Blocked => "blocked",
            Self::Unassessed => "not yet assessed",
        }
    }
}

/// One mandatory register unit: a word definition or an applicable general clause.
#[derive(Clone, Debug)]
pub struct Requirement {
    pub id: String,
    pub profile: String,
    pub kind: String,
    pub subject: String,
    pub provision: String,
    pub source: String,
    pub assessment: String,
    pub reason: String,
}

/// A measured result; identifiers keep baseline checks independent of platform-specific values.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub requirement: Requirement,
    pub status: Status,
    pub checks: Vec<String>,
    pub failed: Vec<String>,
    pub dependencies: Vec<String>,
    pub reason: String,
    pub signatures: Vec<String>,
}

/// Parses and validates the explicit, versioned requirements register.
pub fn parse_register(text: &str) -> Result<Vec<Requirement>, String> {
    let mut ids = BTreeSet::new();
    let mut subjects = BTreeSet::new();
    let mut rows = Vec::new();
    for line in text
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let columns: Vec<_> = line.split('\t').collect();
        if columns.len() != 8 {
            return Err(format!("expected eight columns: {line}"));
        }
        let [id, profile, kind, subject, provision, source, assessment, reason] =
            <[&str; 8]>::try_from(columns).unwrap();
        if !matches!(profile, "f83" | "iso")
            || !matches!(kind, "word" | "system")
            || (kind == "word") != (assessment == "word")
            || [id, subject, provision, assessment]
                .iter()
                .any(|s| s.is_empty())
            || !(source.starts_with("https://www.complang.tuwien.ac.at/forth/fth83std/")
                || source.starts_with("https://www.taygeta.com/forth/dpans"))
            || !id.starts_with(&format!("{profile}."))
            || !ids.insert(id.to_string())
            || !subjects.insert((profile.to_string(), kind.to_string(), subject.to_string()))
        {
            return Err(format!("invalid or duplicate requirement: {line}"));
        }
        rows.push(Requirement {
            id: id.into(),
            profile: profile.into(),
            kind: kind.into(),
            subject: subject.into(),
            provision: provision.into(),
            source: source.into(),
            assessment: assessment.into(),
            reason: if reason == "-" {
                String::new()
            } else {
                reason.into()
            },
        });
    }
    if rows.is_empty()
        || !rows.iter().any(|r| r.profile == "f83")
        || !rows.iter().any(|r| r.profile == "iso")
    {
        return Err("both profiles need a nonempty register".into());
    }
    Ok(rows)
}

/// The exact word names exported by a freshly booted VM.
pub fn available_words() -> BTreeSet<String> {
    // SAFETY: this VM is only inspected with the public word_names accessor.
    unsafe {
        boot(io::sink(), VmConfig::new(8192))
            .unwrap()
            .word_names()
            .into_iter()
            .collect()
    }
}

/// Selects the probes and prerequisite words of a general requirement.
fn assessment<'a>(
    name: &str,
    profile: &str,
    cases: &'a [Case],
) -> Result<(Vec<&'a Case>, Vec<String>), String> {
    let (ids, dependencies): (&[&str], &[&str]) = match name {
        "capacity" => (&["dictionary-capacity"], &[]),
        "data-capacity" => (&["data-capacity"], &[]),
        "return-capacity" => (&["return-capacity"], &[]),
        "cell-pair" => (&["two-store-order", "two-fetch-order"], &[]),
        "memory" => (&["fetch-store", "char-fetch-store", "allot"], &[]),
        "compiler" => (&["colon-exit", "immediate", "tick-execute"], &[]),
        "ascii" => (&["ascii-graphics"], &[]),
        "string-output" => (&["type", "s-quote"], &[]),
        "numbers83" => (&["f83-integer-input", "f83-input-bits"], &[]),
        "control" => (&["nested-control", "begin-until"], &[]),
        "flags" => (&["equal", "signed-order", "zero-order"], &[]),
        "division" => (&["divide-signs", "mod-signs", "divmod-signs"], &[]),
        "names" => (&["name-search"], &[]),
        "alignment" => (&["align"], &[]),
        "delimiters" => (&["delimiters"], &[]),
        "syntax" => (&["paren-comment", "colon-exit", "integer-input"], &[]),
        "blocks" => (&[], &["BLOCK", "BUFFER", "UPDATE", "FLUSH"]),
        "return-stack" => (&[], &[">R", "R>", "R@"]),
        "environment" => (&[], &["ENVIRONMENT?"]),
        "input-buffer" => (&[], &["SOURCE", ">IN"]),
        "number-base" => (&[], &["BASE", "DECIMAL"]),
        "terminal" if profile == "f83" => (&[], &["KEY", "EXPECT"]),
        "terminal" => (&[], &["KEY", "ACCEPT"]),
        _ => return Err(format!("unknown assessment: {name}")),
    };
    let mut selected = Vec::new();
    for id in ids {
        selected.push(
            cases
                .iter()
                .find(|c| c.id == *id)
                .ok_or_else(|| format!("missing registered probe {id}"))?,
        );
    }
    Ok((
        selected,
        dependencies.iter().map(|s| (*s).to_string()).collect(),
    ))
}

/// Determines one requirement's status, without treating absent evidence as success.
fn classify(
    requirement: Requirement,
    checks: Vec<String>,
    failed: Vec<String>,
    dependencies: Vec<String>,
    missing_subject: bool,
    explanation: String,
) -> Outcome {
    let status = if missing_subject {
        Status::Missing
    } else if !dependencies.is_empty() {
        Status::Blocked
    } else if !failed.is_empty() {
        Status::Deviating
    } else if checks.is_empty() {
        Status::Unassessed
    } else {
        Status::Passed
    };
    let reason = match status {
        Status::Missing => "Required word is absent from the fresh VM.".into(),
        Status::Blocked => format!("Missing test prerequisites: {}.", dependencies.join(", ")),
        Status::Deviating => explanation,
        Status::Unassessed => {
            if requirement.reason.is_empty() {
                "No behavioral probe or complete documentary assessment yet.".into()
            } else {
                requirement.reason.clone()
            }
        }
        Status::Passed => "All assigned probes passed; see the coverage limitations.".into(),
    };
    Outcome {
        requirement,
        status,
        checks,
        failed,
        dependencies,
        reason,
        signatures: Vec::new(),
    }
}

/// Measures all requirements. Probe execution and classification are shared by CI and reports.
pub fn measure(
    register: &[Requirement],
    cases: &[Case],
    words: &BTreeSet<String>,
) -> Result<Vec<Outcome>, String> {
    let mut probe_ids = BTreeSet::new();
    for case in cases {
        if case.id.is_empty() || !probe_ids.insert(case.id.clone()) {
            return Err(format!("duplicate or empty probe ID: {}", case.id));
        }
    }
    let known_words: BTreeSet<&str> = register
        .iter()
        .filter(|r| r.kind == "word")
        .map(|r| r.subject.as_str())
        .chain(words.iter().map(String::as_str))
        .collect();
    let mut cache = BTreeMap::new();
    let mut outcomes = Vec::new();
    for r in register {
        let mut selected = Vec::new();
        let mut prerequisites = Vec::new();
        let mut direct = None;
        let absent = r.kind == "word" && !words.contains(&r.subject);
        if r.kind == "word" {
            selected.extend(cases.iter().filter(|c| {
                c.words.contains(&r.subject.as_str()) && c.profile.is_none_or(|p| p == r.profile)
            }));
        } else {
            match r.assessment.as_str() {
                "review" | "unassessed" => (),
                "width83" => {
                    direct = Some((
                        Cell::BITS == 16,
                        "host.cell-width83",
                        format!(
                            "FORTH-83 requires 16-bit cells; this target has {}-bit cells.",
                            Cell::BITS
                        ),
                    ))
                }
                "widthiso" => {
                    direct = Some((
                        Cell::BITS >= 16,
                        "host.cell-widthiso",
                        "The public Cell type must hold at least 16 bits.".into(),
                    ))
                }
                "all-words" => {
                    prerequisites.extend(
                        register
                            .iter()
                            .filter(|x| {
                                x.profile == r.profile
                                    && x.kind == "word"
                                    && !words.contains(&x.subject)
                            })
                            .map(|x| x.subject.clone()),
                    );
                }
                name => (selected, prerequisites) = assessment(name, &r.profile, cases)?,
            }
        }
        let mut signatures = Vec::new();
        let mut checks = Vec::new();
        let mut failed = Vec::new();
        let mut explanations = Vec::new();
        for case in selected {
            checks.push(case.id.clone());
            signatures.push(format!(
                "{}:{:016x}",
                case.id,
                fingerprint(format!("{case:?}").as_bytes())
            ));
            let needed: BTreeSet<_> = case
                .source
                .split_whitespace()
                .chain(case.resume.split_whitespace())
                .chain([".", "DEPTH"])
                .filter(|w| known_words.contains(w))
                .collect();
            let missing: Vec<_> = needed
                .into_iter()
                .filter(|w| !words.contains(*w))
                .map(str::to_string)
                .collect();
            if !missing.is_empty() {
                prerequisites.extend(missing);
                continue;
            }
            let result = cache
                .entry(case.id.clone())
                .or_insert_with(|| cases::run(case));
            if let Err(reason) = result {
                failed.push(case.id.clone());
                explanations.push(format!("{}: {reason}", case.id));
            }
        }
        if let Some((passed, id, explanation)) = direct {
            checks.push(id.into());
            if !passed {
                failed.push(id.into());
                explanations.push(explanation);
            }
        }
        if r.assessment == "all-words" && prerequisites.is_empty() {
            checks.push("host.required-word-availability".into());
        }
        // A missing test facility is different from a missing required VM word.
        if r.kind == "word" && !absent && checks.is_empty() {
            prerequisites.push("behavioral probe for this word".into());
        } else if r.kind == "system"
            && checks.is_empty()
            && prerequisites.is_empty()
            && !matches!(r.assessment.as_str(), "review" | "unassessed" | "all-words")
        {
            prerequisites.push("behavioral test facility for this clause".into());
        }
        checks.sort();
        checks.dedup();
        failed.sort();
        failed.dedup();
        prerequisites.sort();
        prerequisites.dedup();
        let mut outcome = classify(
            r.clone(),
            checks,
            failed,
            prerequisites,
            absent,
            explanations.join("; "),
        );
        signatures.sort();
        outcome.signatures = signatures;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

/// Serializes all outcomes, not only failures, so removed passing checks cannot go unnoticed.
pub fn baseline(outcomes: &[Outcome]) -> String {
    let mut text = String::from(
        "# id\tstatus\tchecks\tfailed checks\tmissing prerequisites\tprobe signatures\tevidence\n",
    );
    for result in outcomes {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            result.requirement.id,
            result.status.name(),
            result.checks.join(","),
            result.failed.join(","),
            result.dependencies.join(","),
            result.signatures.join(","),
            result.reason.replace(['\t', '\n'], " ")
        ));
    }
    text
}

/// Requires an explicit review of additions, improvements, regressions, and stale exceptions.
pub fn check_baseline(outcomes: &[Outcome], expected: &str) -> Result<(), String> {
    let actual = baseline(outcomes);
    let expected = expected.replace("\r\n", "\n");
    if actual == expected {
        return Ok(());
    }
    let old: BTreeSet<_> = expected.lines().collect();
    let new: BTreeSet<_> = actual.lines().collect();
    Err(format!("conformance baseline changed; review every difference before running make conformance-baseline\nremoved: {:?}\nadded: {:?}", old.difference(&new).collect::<Vec<_>>(), new.difference(&old).collect::<Vec<_>>()))
}

/// Integer rounding to tenths avoids platform-dependent formatting and handles half-up ties.
pub fn percentage(passed: usize, total: usize) -> Result<String, String> {
    if total == 0 || passed > total {
        return Err("invalid conformance denominator or numerator".into());
    }
    let tenths = (passed as u128 * 1000 + total as u128 / 2) / total as u128;
    Ok(format!("{}.{:01}%", tenths / 10, tenths % 10))
}

/// Stable FNV-1a content fingerprint; unrelated to Cell-sized Forth arithmetic.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}
