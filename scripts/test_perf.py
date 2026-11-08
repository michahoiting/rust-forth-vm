# Copyright (c) 2026 M.W.R. Hoiting

"""Unit tests for perf.py. Run them with `make perf-test`."""

import argparse
import contextlib
import io
import json
import os
import sqlite3
import tempfile
import unittest
from unittest import mock

import perf

TIME_OUTPUT = """
        0.03 real         0.02 user         0.00 sys
              593920  maximum resident set size
           273012345  instructions retired
            98765432  cycles elapsed
              344064  peak memory footprint
"""

CACHEGRIND_OUTPUT = """desc: Branches: conditional and indirect
cmd: target/perf/examples/perf stack
events: Ir Bc Bcm Bi Bim
fl=/src/vm.rs
fn=rust_forth_vm::vm::next
10 5 1 0 0 0
11 7 2 1 0 0
fn=rust_forth_vm::vm::run
20 100 0 0 3 2
fl=/src/core.rs
fn=rust_forth_vm::vm::next
30 1 0 0 0 0
summary: 113 3 1 3 2
"""


def result(name, value, gated=True, source="same", metric="ir"):
    """Returns one workload result, as measure_tree gives it."""
    return {
        "name": name,
        "category": "Stack",
        "gated": gated,
        "source_sha256": source,
        "metrics": {metric: value},
        "functions": [],
    }


def info(sha="abc", machine="m1", rustc="rustc 1", backend="cachegrind", target="x86"):
    """Returns the identity of a run, as run_info gives it."""
    return {
        "started_at": "2026-11-07T00:00:00Z",
        "git_sha": sha,
        "git_ref": "main",
        "git_dirty": 0,
        "note": None,
        "backend": backend,
        "tool_version": "valgrind-3.22.0",
        "rustc": rustc,
        "target": target,
        "machine": machine,
    }


class ParsingTests(unittest.TestCase):
    """Reading what the tools print."""

    def test_time_output_gives_both_counters(self):
        self.assertEqual(
            perf.parse_time_output(TIME_OUTPUT),
            {"instructions": 273012345, "cycles": 98765432},
        )

    def test_time_output_without_counters_stops_the_script(self):
        with self.assertRaises(SystemExit):
            perf.parse_time_output("0.03 real")

    def test_a_failure_keeps_the_message_without_the_counters(self):
        self.assertEqual(perf.failure_text("stack: wrong" + TIME_OUTPUT), "stack: wrong")

    def test_a_failure_with_nothing_after_it_stays_whole(self):
        self.assertEqual(perf.failure_text("a\nb\n"), "a\nb")

    def test_cachegrind_totals_are_named_after_the_events(self):
        totals, _ = perf.parse_cachegrind(CACHEGRIND_OUTPUT)
        self.assertEqual(totals, {"Ir": 113, "Bc": 3, "Bcm": 1, "Bi": 3, "Bim": 2})

    def test_cachegrind_functions_add_up_by_function_and_file(self):
        _, functions = perf.parse_cachegrind(CACHEGRIND_OUTPUT)
        self.assertEqual(
            functions,
            {
                ("rust_forth_vm::vm::next", "/src/vm.rs"): 12,
                ("rust_forth_vm::vm::run", "/src/vm.rs"): 100,
                ("rust_forth_vm::vm::next", "/src/core.rs"): 1,
            },
        )

    def test_cachegrind_without_a_summary_stops_the_script(self):
        with self.assertRaises(SystemExit):
            perf.parse_cachegrind("events: Ir\n")

    def test_top_functions_are_the_most_expensive_first(self):
        functions = {("a", "f"): 1, ("b", "f"): 3, ("c", "f"): 2}
        self.assertEqual(perf.top_functions(functions, 2), [(("b", "f"), 3), (("c", "f"), 2)])

    def test_median_is_per_key(self):
        rows = [{"a": 1, "b": 9}, {"a": 3, "b": 1}, {"a": 2, "b": 5}]
        self.assertEqual(perf.median_of(rows), {"a": 2, "b": 5})


class StatusTests(unittest.TestCase):
    """What a change means."""

    def test_limits_are_inclusive_from_below(self):
        self.assertEqual(perf.status_of(perf.WARN_PERCENT - 0.01, True), "ok")
        self.assertEqual(perf.status_of(perf.WARN_PERCENT, True), "warn")
        self.assertEqual(perf.status_of(perf.FAIL_PERCENT - 0.01, True), "warn")
        self.assertEqual(perf.status_of(perf.FAIL_PERCENT, True), "fail")

    def test_improvements_never_fail(self):
        self.assertEqual(perf.status_of(-50.0, True), "ok")

    def test_a_workload_that_is_not_gated_never_fails(self):
        self.assertEqual(perf.status_of(500.0, False), "ok")

    def test_a_noisy_backend_never_fails(self):
        self.assertEqual(perf.status_of(500.0, True, decides=False), "ok")


class CompareTests(unittest.TestCase):
    """Comparing two sets of results."""

    def test_a_slowdown_fails_and_sets_the_exit_code(self):
        rows = perf.compare([result("a", 100)], [result("a", 106)], "ir")
        self.assertEqual(rows[0]["status"], "fail")
        self.assertAlmostEqual(rows[0]["change"], 6.0)
        self.assertEqual(perf.worst_exit_code(rows), 1)

    def test_a_warning_keeps_the_exit_code_at_zero(self):
        rows = perf.compare([result("a", 100)], [result("a", 103)], "ir")
        self.assertEqual(rows[0]["status"], "warn")
        self.assertEqual(perf.worst_exit_code(rows), 0)

    def test_a_changed_source_is_not_compared(self):
        rows = perf.compare([result("a", 100)], [result("a", 900, source="new")], "ir")
        self.assertEqual(rows[0]["status"], "changed")
        self.assertIsNone(rows[0]["change"])
        self.assertEqual(perf.worst_exit_code(rows), 0)

    def test_a_new_workload_has_no_base(self):
        rows = perf.compare([], [result("a", 100)], "ir")
        self.assertEqual((rows[0]["status"], rows[0]["base"]), ("new", None))

    def test_a_removed_workload_is_shown(self):
        rows = perf.compare([result("a", 100)], [], "ir")
        self.assertEqual((rows[0]["status"], rows[0]["head"]), ("removed", None))


    def test_rows_say_whether_a_workload_is_gated(self):
        rows = perf.compare([result("a", 1), result("b", 1, gated=False)], [result("a", 1)], "ir")
        self.assertEqual([(row["name"], row["gated"]) for row in rows], [("a", True), ("b", False)])


def summary(rows, analyses=None, backend="cachegrind", elapsed=60.0, base=None, head=None):
    """Returns a summary, as command_ci writes it."""
    metric = perf.MAIN_METRIC[backend]
    return {
        "base_sha": "a" * 40,
        "head_sha": "b" * 40,
        "backend": backend,
        "metric": metric,
        "tool_version": "valgrind-3.22.0",
        "rustc": "rustc 1.99.0",
        "target": "x86_64-unknown-linux-gnu",
        "machine": "m1",
        "base": base if base is not None else [result("stack", 100e6, metric=metric)],
        "head": head if head is not None else [result("stack", 103e6, metric=metric)],
        "rows": rows,
        "analyses": analyses or {},
        "elapsed_seconds": elapsed,
    }


class AnalysisTests(unittest.TestCase):
    """Explaining a slowdown by function."""

    def test_names_lose_the_symbol_hash_the_checkout_and_the_compiler_path(self):
        self.assertEqual(
            perf.same_name(
                "rust_forth_vm::vm::Vm::pop::h0123456789abcdef",
                "/tmp/x/perf-checkouts/head/src/vm.rs",
            ),
            ("rust_forth_vm::vm::Vm::pop", "src/vm.rs"),
        )
        self.assertEqual(
            perf.same_name("core::ptr::drop", "/rustc/0123abcd/library/core/src/ptr/mod.rs")[1],
            "library/core/src/ptr/mod.rs",
        )

    def test_the_largest_differences_come_first_either_way(self):
        base = {("a", "f"): 10, ("b", "f"): 10, ("c", "f"): 10}
        head = {("a", "f"): 11, ("b", "f"): 2, ("c", "f"): 10, ("d", "f"): 5}
        differences = perf.function_differences(base, head, count=2)
        self.assertEqual(
            differences,
            [
                {"function": "b", "file": "f", "base": 10, "head": 2, "difference": -8},
                {"function": "d", "file": "f", "base": 0, "head": 5, "difference": 5},
            ],
        )

    def test_the_two_sides_match_after_renaming(self):
        base = {("f::h0000000000000000", "/t/perf-checkouts/base/src/a.rs"): 10}
        head = {("f::h1111111111111111", "/t/perf-checkouts/head/src/a.rs"): 12}
        differences = perf.function_differences(base, head)
        self.assertEqual([(row["function"], row["difference"]) for row in differences], [("f", 2)])

    def test_only_workloads_from_the_warning_limit_are_analyzed(self):
        with tempfile.TemporaryDirectory() as directory:
            out = perf.Path(directory)
            for side in ("base", "head"):
                (out / side).mkdir()
                for name in ("slow", "same"):
                    (out / side / f"{name}.cachegrind").write_text(CACHEGRIND_OUTPUT)
            rows = [
                perf.row_of("slow", True, 100, 103, perf.WARN_PERCENT, "warn"),
                perf.row_of("same", True, 100, 100, perf.WARN_PERCENT - 0.01, "ok"),
                perf.row_of("new", True, None, 100, None, "new"),
            ]
            analyses = perf.analyze(rows, out / "base", out / "head", out)
        self.assertEqual(analyses, {"slow": []})


class ReportTests(unittest.TestCase):
    """The Markdown report of a CI run."""

    def test_a_comparison_has_a_row_for_each_workload(self):
        rows = [
            perf.row_of("stack", True, 100e6, 103e6, 3.0, "warn"),
            perf.row_of("compile_far", False, 1e6, 2e6, 100.0, "ok"),
        ]
        base = [result("stack", 100e6), result("compile_far", 1e6, gated=False)]
        head = [result("stack", 103e6), result("compile_far", 2e6, gated=False)]
        report = perf.markdown_report(summary(rows, base=base, head=head))
        self.assertIn("| `stack` | Stack | 100.00 M | 103.00 M | +3.00% | - | Warning |", report)
        self.assertIn("| `compile_far` | Stack | 1.00 M | 2.00 M | +100.00% | - | Reported |", report)
        self.assertIn("doesn't fail this check yet", report)

    def test_branch_misses_are_compared_too(self):
        base = [result("stack", 100, metric="ir")]
        base[0]["metrics"]["bim"] = 10
        head = [result("stack", 100, metric="ir")]
        head[0]["metrics"]["bim"] = 12
        rows = perf.compare(base, head, "ir")
        report = perf.markdown_report(summary(rows, base=base, head=head))
        self.assertIn("| +0.00% | +20.00% | Passed |", report)

    def test_branch_misses_need_a_base_other_than_zero(self):
        base = [result("a", 0, metric="bim"), result("b", 1, metric="bim", source="old")]
        head = [result("a", 5, metric="bim"), result("b", 2, metric="bim")]
        self.assertEqual(perf.metric_changes(base, head, "bim"), {})

    def test_other_backends_have_no_branch_misses(self):
        rows = [perf.row_of("stack", True, 100e6, 103e6, 3.0, "warn")]
        report = perf.markdown_report(summary(rows, backend="macos-pmu"))
        self.assertNotIn("Branch misses", report)
        self.assertNotIn("too noisy", report)
        report = perf.markdown_report(summary(rows, backend="cpu-time"))
        self.assertIn("too noisy", report)

    def test_a_changed_source_is_named(self):
        rows = [perf.row_of("stack", True, 100e6, 103e6, None, "changed")]
        report = perf.markdown_report(summary(rows))
        self.assertIn("because their source changed: `stack`.", report)

    def test_an_analysis_lists_its_functions(self):
        rows = [perf.row_of("stack", True, 100e6, 106e6, 6.0, "fail")]
        analysis = {"function": "<a as b>::c|d", "file": "src/vm.rs", "base": 1000, "head": 4000}
        analysis["difference"] = 3000
        report = perf.markdown_report(summary(rows, {"stack": [analysis]}))
        self.assertIn("### Why `stack` is +6.00%", report)
        self.assertIn("| `<a as b>::c\\|d` | `src/vm.rs` | 1,000 | 4,000 | +3,000 |", report)

    def test_without_a_base_it_shows_the_head(self):
        report = perf.markdown_report(summary(None))
        self.assertIn("nothing to compare with", report)
        self.assertIn("| `stack` | Stack | 103.00 M |", report)

    def test_more_time_than_the_budget_is_reported(self):
        rows = [perf.row_of("stack", True, 1, 1, 0.0, "ok")]
        self.assertNotIn("budget", perf.markdown_report(summary(rows)))
        over = summary(rows, elapsed=perf.CI_BUDGET_SECONDS + 1)
        self.assertIn("more than the budget", perf.markdown_report(over))

    def test_annotations_warn_about_slowdowns_and_time(self):
        rows = [
            perf.row_of("stack", True, 1, 1, 6.0, "fail"),
            perf.row_of("output", True, 1, 1, 3.0, "warn"),
            perf.row_of("boot", True, 1, 1, 0.0, "ok"),
        ]
        lines = perf.annotations(summary(rows, elapsed=perf.CI_BUDGET_SECONDS + 1))
        self.assertEqual(len(lines), 3)
        self.assertTrue(all(line.startswith("::warning title=Performance::") for line in lines))
        self.assertIn("stack has +6.00% ir", lines[0])


def entry(
    commit, value, date, source="same", rustc="rustc 1.99.0", tool="valgrind-3.22.0", pr=None
):
    """Returns one line of the history of main, with the workload `stack`."""
    return {
        "schema": perf.HISTORY_SCHEMA,
        "commit": commit,
        "pr": pr,
        "date": date,
        "backend": "cachegrind",
        "rustc": rustc,
        "tool_version": tool,
        "target": "x86_64-unknown-linux-gnu",
        "machine": "m1",
        "workloads": {"stack": {"source_sha256": source, "gated": True, "ir": value}},
    }


def entries(values, **identity):
    """Returns a history with one line per value, oldest first, with commits c0, c1 and so on."""
    return [
        entry(f"c{index}", value, f"2026-11-{index + 1:02d}T00:00:00Z", **identity)
        for index, value in enumerate(values)
    ]


def head_series(history, limit=perf.HISTORY_RUNS, **head):
    """Returns the series of `stack` in a history, for a head like the summary's."""
    head_result = {"name": "stack", "source_sha256": head.get("source", "same"), "gated": True}
    return perf.series(
        history,
        "stack",
        head_result,
        head.get("rustc", "rustc 1.99.0"),
        head.get("tool", "valgrind-3.22.0"),
        limit,
    )


class HistoryTests(unittest.TestCase):
    """The history of main, on the perf-data branch."""

    def test_the_pr_number_comes_from_a_squash_merge_subject(self):
        self.assertEqual(perf.pr_number("Report the performance (#134)"), 134)
        self.assertEqual(perf.pr_number("Fix (#1) of (#2)\n\nBody (#3)"), 2)
        self.assertIsNone(perf.pr_number("Merge branch 'x'"))
        self.assertIsNone(perf.pr_number("See (#12) for why"))
        self.assertIsNone(perf.pr_number(""))

    def test_dates_are_stored_in_utc(self):
        self.assertEqual(perf.utc_date("2026-11-08T17:54:43+01:00"), "2026-11-08T16:54:43Z")
        self.assertEqual(perf.utc_date("2026-11-08T16:54:43Z"), "2026-11-08T16:54:43Z")

    def test_an_entry_has_the_head_of_a_summary(self):
        head = [result("stack", 103e6), result("compile_far", 2e6, gated=False)]
        head[0]["metrics"]["bim"] = 7
        made = perf.history_entry(summary([], head=head), "Speed up (#140)", "2026-11-08T16:54:43Z")
        self.assertEqual(
            made,
            {
                "schema": perf.HISTORY_SCHEMA,
                "commit": "b" * 40,
                "pr": 140,
                "date": "2026-11-08T16:54:43Z",
                "backend": "cachegrind",
                "rustc": "rustc 1.99.0",
                "tool_version": "valgrind-3.22.0",
                "target": "x86_64-unknown-linux-gnu",
                "machine": "m1",
                "workloads": {
                    "stack": {"source_sha256": "same", "gated": True, "ir": 103e6, "bim": 7},
                    "compile_far": {"source_sha256": "same", "gated": False, "ir": 2e6},
                },
            },
        )

    def test_a_history_is_sorted_and_skips_other_schemas_and_bad_lines(self):
        old, new = entries([1, 2])
        other = dict(old, schema=perf.HISTORY_SCHEMA + 1, commit="other")
        with tempfile.TemporaryDirectory() as directory:
            path = perf.Path(directory) / "history.jsonl"
            lines = [json.dumps(new), json.dumps(other), "{not json", "", json.dumps(old)]
            path.write_text("\n".join(lines) + "\n")
            with contextlib.redirect_stderr(io.StringIO()):
                read = perf.read_history(path)
        self.assertEqual([item["commit"] for item in read], ["c0", "c1"])

    def test_a_missing_history_is_empty(self):
        self.assertEqual(perf.read_history("/nonexistent/history.jsonl"), [])

    def test_a_series_is_newest_last_and_limited(self):
        points, broke = head_series(entries([1, 2, 3, 4]), limit=3)
        self.assertEqual([point["value"] for point in points], [2, 3, 4])
        self.assertEqual(points[-1], {"commit": "c3", "pr": None, "value": 4})
        self.assertIsNone(broke)

    def test_a_series_without_a_break_goes_back_to_the_start(self):
        points, broke = head_series(entries([1, 2]))
        self.assertEqual([point["commit"] for point in points], ["c0", "c1"])
        self.assertIsNone(broke)

    def test_a_series_breaks_on_the_source_rustc_and_valgrind(self):
        for change, reason in (
            ({"source": "old"}, "changed workload source"),
            ({"rustc": "rustc 1.98.0"}, "new rustc, rustc 1.98.0 before"),
            ({"tool": "valgrind-3.21.0"}, "new counter version, valgrind-3.21.0 before"),
        ):
            history = entries([1, 2], **change) + [
                entry("c2", 3, "2026-11-03T00:00:00Z"),
                entry("c3", 4, "2026-11-04T00:00:00Z"),
            ]
            points, broke = head_series(history)
            self.assertEqual([point["commit"] for point in points], ["c2", "c3"], change)
            self.assertEqual(broke, {"reasons": [reason], "since": "c2"}, change)

    def test_a_break_at_the_newest_line_starts_with_the_head(self):
        points, broke = head_series(entries([1, 2]), rustc="rustc 2.0.0")
        self.assertEqual(points, [])
        self.assertEqual(broke["since"], None)

    def test_a_workload_missing_from_older_lines_is_new_not_broken(self):
        history = entries([1, 2])
        del history[0]["workloads"]["stack"]
        points, broke = head_series(history)
        self.assertEqual([point["commit"] for point in points], ["c1"])
        self.assertIsNone(broke)

    def test_drift_is_from_the_oldest_of_the_last_points(self):
        points = [{"commit": f"c{n}", "pr": None, "value": n} for n in range(1, 41)]
        self.assertAlmostEqual(perf.drift(points, 22), 100.0)  # The window starts at 11.
        self.assertIsNone(perf.drift([], 5))

    def test_drift_warns_from_its_limit_for_gated_workloads(self):
        def drifting(head_value, gated=True, backend="cachegrind"):
            head = [result("stack", head_value, gated=gated, metric=perf.MAIN_METRIC[backend])]
            made = summary([], head=head, backend=backend)
            return perf.history_of(made, entries([100] * 3))["stack"]["drifting"]

        # 25% is exact in floating point, unlike 110 / 100, so the limit itself is tested.
        with mock.patch.object(perf, "DRIFT_PERCENT", 25.0):
            self.assertFalse(drifting(124.99))
            self.assertTrue(drifting(125))
        self.assertTrue(drifting(100 + perf.DRIFT_PERCENT + 0.01))
        self.assertFalse(drifting(200, gated=False))

    def test_drift_needs_a_backend_that_decides(self):
        history = [entry("c0", 100, "2026-11-01T00:00:00Z")]
        history[0]["workloads"]["stack"]["user_seconds"] = 1.0
        history[0]["tool_version"] = None
        made = summary([], head=[result("stack", 2.0, metric="user_seconds")], backend="cpu-time")
        made["tool_version"] = None
        item = perf.history_of(made, history)["stack"]
        self.assertAlmostEqual(item["drift"], 100.0)
        self.assertFalse(item["drifting"])


class HistoryReportTests(unittest.TestCase):
    """The development in the report of a CI run."""

    def report(self, history, head_value=103e6):
        """Returns a report and its summary with a history, for `stack`."""
        rows = [perf.row_of("stack", True, 100e6, head_value, 3.0, "warn")]
        made = summary(rows, head=[result("stack", head_value)])
        made["history"] = perf.history_of(made, history)
        return perf.markdown_report(made), made

    def test_without_history_there_is_no_column(self):
        rows = [perf.row_of("stack", True, 100e6, 103e6, 3.0, "warn")]
        report = perf.markdown_report(summary(rows))
        self.assertNotIn("merges", report)

    def test_the_column_has_the_merges_and_the_head(self):
        report, _ = self.report(entries([100e6, 101e6, 102e6]))
        self.assertIn("| Status | Last 20 merges |", report)
        self.assertIn("| Warning | ▁▃▆█ |", report)
        self.assertNotIn("Drift", report)
        self.assertNotIn("starts again", report)

    def test_an_empty_history_says_so(self):
        report, _ = self.report([])
        self.assertIn("| Warning | - |", report)
        self.assertIn("No earlier merges", report)

    def test_a_break_is_explained(self):
        history = entries([1, 2], rustc="rustc 1.98.0")
        history.append(entry("d" * 40, 100e6, "2026-11-05T00:00:00Z", pr=140))
        report, _ = self.report(history)
        self.assertIn(
            "The series of `stack` starts again at `dddddddddddd` (#140):"
            " new rustc, rustc 1.98.0 before.",
            report,
        )

    def test_a_break_at_the_head_is_explained(self):
        report, _ = self.report(entries([1, 2], source="old"))
        self.assertIn("starts again with this head: changed workload source.", report)

    def test_drift_is_reported_and_annotated(self):
        report, made = self.report(entries([90e6, 100e6]), head_value=100e6)
        self.assertIn("**Drift:** `stack` is +11.11% above `c0`", report)
        lines = perf.annotations(made)
        self.assertEqual(len(lines), 2)
        self.assertIn("Performance drift::stack is +11.11% ir", lines[1])

    def test_a_report_without_a_base_has_the_column_too(self):
        made = summary(None, head=[result("stack", 103e6)])
        made["history"] = perf.history_of(made, entries([100e6]))
        self.assertIn("| `stack` | Stack | 103.00 M | ▁█ |", perf.markdown_report(made))


class RecordTests(unittest.TestCase):
    """Appending to the history of main."""

    def record(self, directory, made, subject="Speed up (#140)"):
        """Writes a summary, and records it in `<directory>/history.jsonl`."""
        summary_path = perf.Path(directory) / "summary.json"
        if made is not None:
            summary_path.write_text(json.dumps(made))
        args = argparse.Namespace(
            summary=str(summary_path),
            history=str(perf.Path(directory) / "history.jsonl"),
            subject=subject,
            date="2026-11-08T16:54:43Z",
        )
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(perf.command_record(args), 0)
        return perf.read_history(args.history)

    def test_a_summary_appends_one_line(self):
        with tempfile.TemporaryDirectory() as directory:
            history_path = perf.Path(directory) / "history.jsonl"
            history_path.write_text(json.dumps(entry("c0", 1, "2026-11-01T00:00:00Z")) + "\n")
            recorded = self.record(directory, summary([]))
            self.assertEqual(len(history_path.read_text().splitlines()), 2)
        self.assertEqual(
            [(item["commit"], item["pr"]) for item in recorded], [("c0", None), ("b" * 40, 140)]
        )

    def test_a_rerun_records_the_commit_once(self):
        with tempfile.TemporaryDirectory() as directory:
            self.record(directory, summary([]))
            self.assertEqual(len(self.record(directory, summary([]))), 1)

    def test_nothing_measured_records_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(self.record(directory, None), [])
            self.assertEqual(self.record(directory, summary([], head=[])), [])
            self.assertFalse((perf.Path(directory) / "history.jsonl").exists())


class SparklineTests(unittest.TestCase):
    """The small chart."""

    def test_the_lowest_and_highest_values_use_the_ends(self):
        self.assertEqual(perf.sparkline([1, 2, 3]), "▁▅█")

    def test_equal_values_stay_flat(self):
        self.assertEqual(perf.sparkline([5, 5]), "▁▁")


class DatabaseTests(unittest.TestCase):
    """Storing runs, and finding what they can be compared with."""

    def setUp(self):
        self.database = perf.open_database(":memory:")

    def tearDown(self):
        self.database.close()

    def store(self, value, source="same", kind="run", **identity):
        """Stores a run with one workload."""
        results = [result("a", value, source=source)]
        return perf.store_run(self.database, info(**identity), results, kind)

    def test_a_run_reads_back_as_it_was_stored(self):
        run_id = self.store(100)
        self.assertEqual(
            perf.load_results(self.database, run_id),
            [
                {
                    "name": "a",
                    "category": "Stack",
                    "gated": True,
                    "source_sha256": "same",
                    "metrics": {"ir": 100.0},
                }
            ],
        )

    def test_functions_are_stored(self):
        stored = result("a", 100)
        stored["functions"] = [(("next", "vm.rs"), 90)]
        perf.store_run(self.database, info(), [stored])
        rows = self.database.execute("SELECT function, file, ir FROM function_cost").fetchall()
        self.assertEqual(rows, [("next", "vm.rs", 90)])

    def test_the_previous_run_is_the_newest_comparable_one(self):
        first = self.store(100)
        second = self.store(101)
        third = self.store(102)
        self.assertIsNone(perf.previous_run(self.database, first))
        self.assertEqual(perf.previous_run(self.database, third), second)

    def test_other_machines_compilers_backends_and_targets_are_not_comparable(self):
        self.store(100)
        for identity in (
            {"machine": "m2"},
            {"rustc": "rustc 2"},
            {"backend": "macos-pmu"},
            {"target": "arm"},
        ):
            run_id = self.store(100, **identity)
            self.assertIsNone(perf.previous_run(self.database, run_id), identity)

    def test_a_comparison_is_not_the_previous_run(self):
        first = self.store(100)
        self.store(500, kind="compare-base")
        self.store(600, kind="compare-head")
        self.assertEqual(perf.previous_run(self.database, self.store(101)), first)

    def test_the_latest_run_is_not_a_comparison(self):
        first = self.store(100)
        self.store(600, kind="compare-head")
        self.assertEqual(perf.latest_run(self.database), first)

    def test_history_leaves_out_comparisons(self):
        self.store(1)
        self.store(500, kind="compare-base")
        self.store(600, kind="compare-head")
        run_id = self.store(2)
        self.assertEqual(perf.history(self.database, run_id, "ir", 9), {"a": [1.0, 2.0]})

    def test_history_is_oldest_first_and_limited(self):
        for value in (1, 2, 3, 4):
            run_id = self.store(value)
        self.assertEqual(perf.history(self.database, run_id, "ir", 3), {"a": [2.0, 3.0, 4.0]})
        self.assertEqual(perf.history(self.database, run_id, "ir", 9), {"a": [1.0, 2.0, 3.0, 4.0]})

    def test_history_leaves_out_other_machines(self):
        self.store(1)
        self.store(9, machine="m2")
        run_id = self.store(2)
        self.assertEqual(perf.history(self.database, run_id, "ir", 9), {"a": [1.0, 2.0]})

    def test_history_starts_at_the_last_change_of_the_source(self):
        self.store(1, source="old")
        self.store(2, source="new")
        run_id = self.store(3, source="new")
        self.assertEqual(perf.history(self.database, run_id, "ir", 9), {"a": [2.0, 3.0]})

    def test_deleting_a_run_deletes_its_figures(self):
        run_id = self.store(100)
        self.database.execute("DELETE FROM run WHERE id = ?", (run_id,))
        self.assertEqual(self.database.execute("SELECT COUNT(*) FROM measurement").fetchone(), (0,))

    def test_an_unknown_backend_is_refused(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.store(100, backend="wall-clock")

    def test_another_schema_version_stops_the_script(self):
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "results.sqlite")
            database = perf.open_database(path)
            database.execute("UPDATE schema_version SET version = 0")
            database.commit()
            database.close()
            with self.assertRaises(SystemExit):
                perf.open_database(path)


if __name__ == "__main__":
    unittest.main()
