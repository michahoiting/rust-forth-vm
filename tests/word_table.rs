// Copyright (c) 2026 M.W.R. Hoiting

//! Tests of the word table in `docs/words.md`. Keep tests of Markdown out of what Miri runs: CI
//! skips Miri on PRs that only change Markdown, so such a test wouldn't run there.

#![cfg(not(miri))]

use std::collections::BTreeSet;
use std::io;

use rust_forth_vm::{boot, VmConfig};

/// The word table.
const WORDS: &str = include_str!("../docs/words.md");

/// The number of columns in each row: word, stack effect, standard, implemented, description.
const COLUMNS: usize = 5;

/// The size of the dictionary, which must hold every primitive.
const TEST_CELLS: usize = 64 * 1024;

/// Splits a table row into its trimmed cells. A `|` after a backslash is part of a cell.
fn cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in row.trim().chars() {
        if c == '|' && !escaped {
            cells.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
        escaped = c == '\\';
    }
    cells.push(cell.trim().to_string());
    // A row starts and ends with `|`, which leaves an empty cell at each end.
    cells[1..cells.len() - 1].to_vec()
}

/// Returns the cells of every row that describes a word: the rows whose first cell is code.
fn word_rows() -> Vec<Vec<String>> {
    WORDS
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(cells)
        .collect()
}

/// Returns the word of a row, without the backticks around it.
fn word_of(row: &[String]) -> &str {
    row[0]
        .strip_prefix('`')
        .and_then(|word| word.strip_suffix('`'))
        .expect("the word must be code")
}

/// The address of the project's issues, which a planned word links to.
const ISSUES: &str = "https://github.com/michahoiting/rust-forth-vm/issues/";

/// Returns whether `implemented` is `Yes`, `No`, or `No` with a link to the issue that plans
/// the word, such as `No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119))`.
fn is_valid_implemented(implemented: &str) -> bool {
    if implemented == "Yes" || implemented == "No" {
        return true;
    }
    let Some(link) = implemented
        .strip_prefix("No ([#")
        .and_then(|rest| rest.strip_suffix("))"))
    else {
        return false;
    };
    let Some((number, url)) = link.split_once("](") else {
        return false;
    };
    !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && url == format!("{}{}", ISSUES, number)
}

#[test]
fn is_valid_implemented_accepts_yes_no_and_a_planned_issue() {
    assert!(is_valid_implemented("Yes"));
    assert!(is_valid_implemented("No"));
    assert!(is_valid_implemented(
        "No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/119))"
    ));
}

#[test]
fn is_valid_implemented_rejects_anything_else() {
    for value in [
        "",
        "Maybe",
        "No (#119)",
        "No ([#](https://github.com/michahoiting/rust-forth-vm/issues/))",
        "No ([#119](https://github.com/michahoiting/rust-forth-vm/issues/120))",
        "No ([#119](https://example.com/issues/119))",
        "No ([#11x](https://github.com/michahoiting/rust-forth-vm/issues/11x))",
    ] {
        assert!(!is_valid_implemented(value), "accepted: {:?}", value);
    }
}

#[test]
fn cells_keeps_an_escaped_bar_in_its_cell() {
    assert_eq!(
        cells(r"| `?DUP` | `( x -- 0 \| x x )` | ANS CORE | Yes | Copies |"),
        [
            "`?DUP`",
            r"`( x -- 0 \| x x )`",
            "ANS CORE",
            "Yes",
            "Copies"
        ]
    );
}

#[test]
fn cells_keeps_a_backslash_word() {
    assert_eq!(cells(r"| `\` | `( -- )` | X | Yes | Skips |")[0], r"`\`");
}

#[test]
fn every_row_is_complete() {
    let rows = word_rows();
    assert!(!rows.is_empty(), "the table must have words");
    for row in &rows {
        assert_eq!(row.len(), COLUMNS, "row: {:?}", row);
        assert!(!word_of(row).is_empty(), "row: {:?}", row);
        assert!(!row[1].is_empty(), "no stack effect in row: {:?}", row);
        assert!(!row[2].is_empty(), "no standard in row: {:?}", row);
        assert!(!row[4].is_empty(), "no description in row: {:?}", row);
        assert!(
            is_valid_implemented(&row[3]),
            "Implemented must be Yes, No, or No with a link to an issue in row: {:?}",
            row
        );
    }
}

#[test]
fn no_word_is_listed_twice() {
    let mut seen = BTreeSet::new();
    for row in word_rows() {
        let word = word_of(&row).to_string();
        assert!(seen.insert(word.clone()), "{} is listed twice", word);
    }
}

#[test]
fn implemented_words_are_exactly_the_words_of_the_vm() {
    let marked: BTreeSet<String> = word_rows()
        .iter()
        .filter(|row| row[3] == "Yes")
        .map(|row| word_of(row).to_string())
        .collect();

    let names = boot(io::sink(), VmConfig::new(TEST_CELLS))
        .unwrap()
        .word_names();
    let actual: BTreeSet<String> = names.into_iter().collect();

    let not_marked: Vec<_> = actual.difference(&marked).collect();
    let not_in_vm: Vec<_> = marked.difference(&actual).collect();
    assert!(
        not_marked.is_empty() && not_in_vm.is_empty(),
        "words in the VM that aren't marked Yes: {:?}; words marked Yes that the VM doesn't have: {:?}",
        not_marked,
        not_in_vm
    );
}
