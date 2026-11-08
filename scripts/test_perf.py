# Copyright (c) 2026 M.W.R. Hoiting

"""Unit tests for perf.py. Run them with `make perf-test`."""

import os
import sqlite3
import tempfile
import unittest

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
