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
