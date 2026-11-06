# Copyright (c) 2026 M.W.R. Hoiting

"""Reports how fast issues get solved, from GitHub's data. Run it with `make metrics`.

It only reads from GitHub, through the gh CLI. Its output changes with live data, so CI
doesn't run it. See ADR-0001 in docs/adr for what the numbers are used for, and for the limits
of what they show.
"""

import argparse
import collections
import datetime
import json
import statistics
import subprocess
import sys

# GitHub returns at most this many items per list, which is far more than this project has.
LIMIT = 1000

# Review bots. Their reviews don't count as the review that a PR waits for.
BOTS = {"claude"}


def parse_time(text):
    """Parses a date or a time, and treats one without a time zone as UTC."""
    parsed = datetime.datetime.fromisoformat(text.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=datetime.timezone.utc)
    return parsed


def minutes(start, end):
    """Returns the whole number of minutes between two GitHub timestamps."""
    return round((parse_time(end) - parse_time(start)).total_seconds() / 60)


def gh(*args):
    """Runs gh, and returns its JSON output. Stops if gh fails or hits the limit."""
    done = subprocess.run(["gh", *args, "--limit", str(LIMIT)], capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"gh failed: {done.stderr.strip()}")
    items = json.loads(done.stdout)
    if len(items) >= LIMIT:
        sys.exit(f"gh returned {LIMIT} items, so some are missing. Raise LIMIT in this script.")
    return items


def in_window(timestamp, since, until):
    """Says whether a timestamp is at or after `since`, and before `until`."""
    moment = parse_time(timestamp)
    return (since is None or moment >= since) and (until is None or moment < until)


def is_bot(login):
    """Says whether a login is a review bot."""
    return login in BOTS or login.endswith("[bot]")


def pr_row(pr, branch_runs):
    """Returns the numbers of one merged PR.

    Reviews have no author type, and agents and the owner share an account, so every review
    that isn't from a bot counts. The first review time includes the time spent reading.
    """
    reviews = [
        review["submittedAt"]
        for review in pr["reviews"]
        if review["submittedAt"] and not is_bot(review["author"]["login"])
    ]
    first_review = min(reviews, key=parse_time) if reviews else None

    # Only runs while the PR was open, in case a later PR reuses the branch name.
    runs = [
        run
        for run in branch_runs
        if parse_time(pr["createdAt"]) <= parse_time(run["createdAt"]) <= parse_time(pr["mergedAt"])
    ]
    finished = [run for run in runs if run["conclusion"] != "cancelled"]

    return {
        "number": pr["number"],
        "lead": minutes(pr["createdAt"], pr["mergedAt"]),
        "to_review": minutes(pr["createdAt"], first_review) if first_review else None,
        "after_review": minutes(first_review, pr["mergedAt"]) if first_review else None,
        "reviews": len(reviews),
        # None means that GitHub has no CI runs for the PR, not that it had no reruns.
        "reruns": max(len(finished) - 1, 0) if runs else None,
    }


def collect(since, until):
    """Returns a row for each PR merged in the window, oldest first."""
    prs = gh(
        "pr", "list", "--state", "merged",
        "--json", "number,headRefName,createdAt,mergedAt,reviews",
    )
    runs = gh(
        "run", "list", "--workflow", "CI", "--event", "pull_request",
        "--json", "headBranch,conclusion,createdAt",
    )
    by_branch = collections.defaultdict(list)
    for run in runs:
        by_branch[run["headBranch"]].append(run)

    merged = [pr for pr in prs if in_window(pr["mergedAt"], since, until)]
    return [
        pr_row(pr, by_branch[pr["headRefName"]])
        for pr in sorted(merged, key=lambda pr: parse_time(pr["mergedAt"]))
    ]


def closed_per_week(since, until):
    """Counts the issues completed in the window, by ISO week. Not-planned issues don't count."""
    issues = gh("issue", "list", "--state", "closed", "--json", "closedAt,stateReason")
    weeks = collections.Counter()
    for issue in issues:
        if issue["stateReason"] == "COMPLETED" and in_window(issue["closedAt"], since, until):
            year, week, _ = parse_time(issue["closedAt"]).isocalendar()
            weeks[f"{year}-W{week:02d}"] += 1
    return sorted(weeks.items())


def summary_line(label, values, unit):
    """Formats the median of the values that exist, and how many PRs they cover."""
    known = [value for value in values if value is not None]
    if not known:
        return f"{label:<34} n/a"
    return f"{label:<34} {statistics.median(known):g}{unit} (n={len(known)})"


def report(rows, weeks, show_prs):
    """Prints the report."""
    if show_prs:
        print(f"{'PR':>5} {'lead':>6} {'review':>7} {'after':>6} {'reviews':>8} {'reruns':>7}")
        for row in rows:
            cells = [row["to_review"], row["after_review"], row["reviews"], row["reruns"]]
            shown = ["-" if cell is None else cell for cell in cells]
            print(
                f"#{row['number']:<4} {row['lead']:>6} {shown[0]:>7} {shown[1]:>6}"
                f" {shown[2]:>8} {shown[3]:>7}"
            )
        print()

    print(f"Merged PRs: {len(rows)}")
    print(summary_line("Median PR lead time", [row["lead"] for row in rows], " min"))
    print(summary_line("Median open to first review", [row["to_review"] for row in rows], " min"))
    print(summary_line("Median first review to merge", [row["after_review"] for row in rows], " min"))
    reruns = [row["reruns"] for row in rows if row["reruns"] is not None]
    if reruns:
        print(f"{'Mean CI reruns per PR':<34} {statistics.mean(reruns):.2f} (n={len(reruns)})")
    else:
        print(f"{'Mean CI reruns per PR':<34} n/a")
    print("Issues completed per week:")
    for week, count in weeks:
        print(f"  {week}: {count}")
    if not weeks:
        print("  none")


def main():
    """Parses the arguments, and prints the report."""
    parser = argparse.ArgumentParser(description="Reports PR and issue throughput.")
    parser.add_argument("--since", type=parse_time, help="start of the window, inclusive")
    parser.add_argument("--until", type=parse_time, help="end of the window, exclusive")
    parser.add_argument("--prs", action="store_true", help="also list each PR, in minutes")
    args = parser.parse_args()
    report(collect(args.since, args.until), closed_per_week(args.since, args.until), args.prs)


if __name__ == "__main__":
    main()
