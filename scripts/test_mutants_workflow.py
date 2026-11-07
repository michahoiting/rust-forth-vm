# Copyright (c) 2026 M.W.R. Hoiting

"""Tests for the incremental mutation target and privileged comment formatter."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent


class MutantsTargetTests(unittest.TestCase):
    """Make must pass the diff as one argument and preserve the cargo exit code."""

    def test_target_passes_diff_and_records_cargo_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            cargo = temp / "cargo"
            cargo.write_text(
                '#!/bin/sh\n'
                'if [ "$2" = "--version" ]; then exit 0; fi\n'
                'printf "%s\\n" "$@" > "$ARGS_FILE"\n'
                'exit 3\n',
                encoding="utf-8",
            )
            cargo.chmod(0o755)
            diff = temp / "a diff"
            diff.write_text("", encoding="utf-8")
            status = temp / "status"
            args = temp / "args"
            env = dict(os.environ, PATH=f"{temp}:{os.environ['PATH']}", ARGS_FILE=str(args))
            result = subprocess.run(
                ["make", "mutants-diff", f"DIFF={diff}", f"MUTANTS_STATUS_FILE={status}"],
                cwd=ROOT, env=env, capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(status.read_text(encoding="utf-8"), "3\n")
            self.assertEqual(args.read_text(encoding="utf-8").splitlines(),
                             ["mutants", "--in-diff", str(diff)])

    def test_target_rejects_missing_diff(self):
        result = subprocess.run(
            ["make", "mutants-diff", "DIFF=/nonexistent/mutants.diff"],
            cwd=ROOT, capture_output=True, text=True, check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("readable diff file", result.stdout)


class CommentFormatterTests(unittest.TestCase):
    """Exercise the actual inline reporter without granting it a token."""

    @classmethod
    def setUpClass(cls):
        workflow = (ROOT / ".github/workflows/mutants-pr.yml").read_text(encoding="utf-8")
        cls.script = textwrap.dedent(
            workflow.split("python3 - <<'PY' > \"$RUNNER_TEMP/comment.json\"\n", 1)[1]
            .split("\n          PY\n", 1)[0]
        )

    def comment(self, **overrides):
        env = dict(
            os.environ,
            MUTANTS_OUTCOME="success", MUTANTS_EXIT_CODE="0", MUTANTS_DURATION="12",
            MUTANTS_COUNTS="2,2,0,0,0", JOB_RESULT="success", TESTED_SHA="a" * 40,
            GITHUB_SERVER_URL="https://github.com", GITHUB_REPOSITORY="owner/repo",
            GITHUB_RUN_ID="123",
        )
        env.update(overrides)
        result = subprocess.run(
            ["python3", "-c", self.script], env=env, capture_output=True, text=True, check=True,
        )
        return json.loads(result.stdout)["body"]

    def test_success_and_empty_results(self):
        self.assertIn("caught: 2", self.comment())
        self.assertIn("12 seconds", self.comment())
        self.assertIn("No eligible mutants", self.comment(MUTANTS_COUNTS="0,0,0,0,0"))

    def test_failure_and_missing_results(self):
        result = self.comment(
            MUTANTS_OUTCOME="failure", MUTANTS_EXIT_CODE="4",
            MUTANTS_COUNTS="", MUTANTS_DURATION="", JOB_RESULT="failure",
        )
        self.assertIn("Baseline tests failed", result)
        self.assertIn("Counts:** unavailable", result)
        self.assertNotIn("Completed", result)

    def test_untrusted_fields_are_not_interpolated(self):
        result = self.comment(
            MUTANTS_COUNTS="1,2,3,4,5\n@everyone", MUTANTS_DURATION="1\n@everyone",
            TESTED_SHA="not-a-sha\n@everyone", MUTANTS_OUTCOME="failure",
        )
        self.assertNotIn("@everyone", result)
        self.assertIn("unavailable", result)


if __name__ == "__main__":
    unittest.main()
