# Copyright (c) 2026 M.W.R. Hoiting

"""Summarize an incremental cargo-mutants run for GitHub Actions."""

import json
import os
from pathlib import Path

COUNT_FIELDS = ("total_mutants", "caught", "missed", "timeout", "unviable")


def read_counts(path, empty_success=False):
    """Read bounded numeric counts, or report why they are unavailable."""
    try:
        with Path(path).open(encoding="utf-8") as file:
            data = json.load(file)
        counts = tuple(data[field] for field in COUNT_FIELDS)
        if any(type(value) is not int or not 0 <= value <= 10_000_000 for value in counts):
            raise ValueError("invalid mutation count")
        return counts
    except FileNotFoundError as error:
        if empty_success:
            return (0, 0, 0, 0, 0)
        print(f"Mutation counts unavailable: {error}")
        return None
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Mutation counts unavailable: {error}")
        return None


def describe(outcome, exit_code, duration, counts):
    """Format the result without treating missing data as a successful run."""
    if outcome == "success" and exit_code == "0":
        status = "Completed"
    elif outcome == "skipped" or not outcome:
        status = "Mutation command did not run"
    else:
        status = {
            "2": "Surviving mutants found",
            "3": "Mutation tests timed out",
            "4": "Baseline tests failed or timed out",
        }.get(exit_code, "Mutation command failed")
    elapsed = f"{duration} seconds" if duration.isdecimal() else "unavailable"
    lines = [f"**Outcome:** {status}", f"**Mutation command time:** {elapsed}"]
    if counts is None:
        lines.append("**Counts:** unavailable; see the Actions logs.")
    else:
        total, caught, missed, timeout, unviable = counts
        lines.append(
            f"**Mutants tested:** {total}; caught: {caught}; missed: {missed}; "
            f"timeouts: {timeout}; unviable: {unviable}."
        )
        if total == 0 and outcome == "success" and exit_code == "0":
            lines.append("No eligible mutants were tested.")
    return "\n\n".join(lines)


def main():
    """Write bounded outputs and the Actions summary."""
    outcome = os.environ.get("MUTANTS_OUTCOME", "")
    exit_code = os.environ.get("MUTANTS_EXIT_CODE", "")
    duration = os.environ.get("MUTANTS_DURATION", "")
    counts = read_counts(
        os.environ.get("MUTANTS_OUTCOMES", "mutants.out/outcomes.json"),
        empty_success=outcome == "success" and exit_code == "0",
    )
    summary = describe(outcome, exit_code, duration, counts)
    print(summary)
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as file:
        file.write(f"## Incremental mutation testing\n\n{summary}\n")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as file:
        file.write(f"counts={','.join(map(str, counts)) if counts is not None else ''}\n")


if __name__ == "__main__":
    main()
