// Copyright (c) 2026 M.W.R. Hoiting

//! Passes the build date and the git state to the `forth-vm` program as `BUILD_INFO`, which
//! its `--version` option prints. A build without git still works.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The number of seconds in a day.
const SECONDS_PER_DAY: u64 = 86_400;

/// The git state that says what the next build should show. These are per-worktree or shared
/// paths, so git has to say where each one is.
const GIT_PATHS: [&str; 5] = ["HEAD", "index", "packed-refs", "refs/heads", "refs/tags"];

/// Runs git with `arguments`, and returns its trimmed output, or `None` if it couldn't run or
/// it failed.
fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

/// Returns the given number of days since 1970-01-01 as a date in the form `YYYY-MM-DD`. Works
/// in eras of 400 years, which all have the same length, starting on 0000-03-01, so that a leap
/// day is the last day of its year.
fn date_from_days(days: u64) -> String {
    /// The number of days from 0000-03-01 to 1970-01-01.
    const DAYS_TO_EPOCH: u64 = 719_468;
    /// The number of days in an era.
    const DAYS_PER_ERA: u64 = 146_097;

    let days = days + DAYS_TO_EPOCH;
    let day_of_era = days % DAYS_PER_ERA;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + days / DAYS_PER_ERA * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Returns today's date in UTC, in the form `YYYY-MM-DD`.
fn build_date() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock must be after 1970");
    date_from_days(now.as_secs() / SECONDS_PER_DAY)
}

/// Returns the tag and the hash of `HEAD`, followed by `(dirty)` unless the build is a clean
/// release. That means `HEAD` has a tag of its own, and the working tree has no changes.
fn git_state() -> String {
    let Some(hash) = git(&["rev-parse", "--short", "HEAD"]) else {
        return "unknown unknown".to_string();
    };
    let tag = git(&["tag", "--points-at", "HEAD", "--sort=-version:refname"])
        .and_then(|tags| tags.lines().next().map(str::to_string));
    // Without the option, git can rewrite the index here, which would trigger another build.
    let clean = git(&["--no-optional-locks", "status", "--porcelain"])
        .is_some_and(|changes| changes.is_empty());
    match tag {
        Some(tag) if clean => format!("{tag} {hash}"),
        Some(tag) => format!("{tag} {hash} (dirty)"),
        None => format!("none {hash} (dirty)"),
    }
}

/// Asks Cargo to run this script again when anything that `git_state` reads changes: git's own
/// state, and each file git knows about, because editing one changes nothing in `.git`.
fn watch_git_state() {
    for name in GIT_PATHS {
        // Cargo reruns this script on every build if a watched path doesn't exist.
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            if Path::new(&path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    let files = git(&["ls-files", "--cached", "--others", "--exclude-standard"]);
    for file in files.iter().flat_map(|files| files.lines()) {
        println!("cargo:rerun-if-changed={file}");
    }
}

/// Prints the instructions for Cargo.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    watch_git_state();
    println!(
        "cargo:rustc-env=BUILD_INFO=Built {}, Git {}",
        build_date(),
        git_state()
    );
}
