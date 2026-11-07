# Copyright (c) 2026 M.W.R. Hoiting

"""Tests for incremental mutation reporting."""

import json
import tempfile
import unittest
from pathlib import Path

import mutants_report


class MutantsReportTests(unittest.TestCase):
    """Counts and outcomes must not turn errors into success."""

    def test_valid_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcomes.json"
            path.write_text(
                json.dumps(dict(zip(mutants_report.COUNT_FIELDS, (5, 3, 1, 1, 0)))),
                encoding="utf-8",
            )
            self.assertEqual(mutants_report.read_counts(path), (5, 3, 1, 1, 0))

    def test_invalid_or_missing_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "outcomes.json"
            self.assertIsNone(mutants_report.read_counts(path))
            self.assertEqual(mutants_report.read_counts(path, empty_success=True), (0, 0, 0, 0, 0))
            path.write_text('{"total_mutants": true}', encoding="utf-8")
            self.assertIsNone(mutants_report.read_counts(path))
            path.write_text("{", encoding="utf-8")
            self.assertIsNone(mutants_report.read_counts(path))

    def test_success_without_mutants(self):
        result = mutants_report.describe("success", "0", "7", (0, 0, 0, 0, 0))
        self.assertIn("No eligible mutants", result)
        self.assertIn("7 seconds", result)

    def test_failures_and_missing_values(self):
        for code, text in (
            ("2", "Surviving mutants"),
            ("3", "timed out"),
            ("4", "Baseline tests failed"),
            ("1", "command failed"),
            ("", "command failed"),
        ):
            result = mutants_report.describe("failure", code, "", None)
            self.assertIn(text, result)
            self.assertIn("unavailable", result)
            self.assertNotIn("Completed", result)
        self.assertIn("did not run", mutants_report.describe("skipped", "", "", None))


if __name__ == "__main__":
    unittest.main()
