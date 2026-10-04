"""Regression tests for release-gate outcomes; no content or compiler needed."""
import unittest
from unittest.mock import patch
from pathlib import Path
import sys

import gate


class GateEnvironment(unittest.TestCase):
    def test_content_backed_tests_get_the_main_content(self):
        env = gate_env_for({"PATH": "x", "CARGO_TARGET_DIR": "elsewhere"})
        self.assertEqual(env["BRI_CONTENT"], str(Path("main") / "content"))
        self.assertNotIn("CARGO_TARGET_DIR", env)
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")

    def test_a_chosen_content_folder_is_kept(self):
        env = gate_env_for({"BRI_CONTENT": "mine"})
        self.assertEqual(env["BRI_CONTENT"], "mine")


def gate_env_for(environ):
    return gate.gate_env(environ, Path("main") / "content")


class BinaryDeadline(unittest.TestCase):
    def test_stalled_binary_is_stopped_and_reported_as_failure(self):
        with patch.object(gate, 'BINARY_TIMEOUT', 0.1):
            output, code = gate.run_binary(
                'fixture/stalled', sys.executable,
                ['-c', 'import time; time.sleep(60)'], Path.cwd())
        self.assertNotEqual(code, 0)
        self.assertIn('test fixture/stalled::gate_timeout ... FAILED', output)

    def test_binary_failure_keeps_its_diagnostic(self):
        output, code = gate.run_binary(
            'fixture/error', sys.executable,
            ['-c', 'print("specific failure"); raise SystemExit(42)'], Path.cwd())
        self.assertEqual(code, 42)
        self.assertIn('specific failure', output)


class SaveCorpusResult(unittest.TestCase):
    passed = f"test {gate.SAVE_CORPUS_TEST} ... ok\n"

    def test_skipped_corpus_does_not_claim_verified_saves(self):
        ok, summary = gate.save_corpus_result(
            "skipped: no saves folder; set BRI_SAVES\n" + self.passed, 0)
        self.assertTrue(ok)  # Existing optional-corpus policy stays unchanged.
        self.assertEqual(summary, "SKIPPED: no saves folder; set BRI_SAVES")

    def test_all_checked_saves_are_reported(self):
        self.assertEqual(gate.save_corpus_result(
            "save corpus coverage: 23/23\n" + self.passed, 0),
            (True, "ok (coverage: 23/23 saves)"))

    def test_missing_saves_are_visible_as_partial_coverage(self):
        self.assertEqual(gate.save_corpus_result(
            "missing from the saves folder, not checked: Bedroom/Violin.bls\n"
            "save corpus coverage: 22/23\n" + self.passed, 0),
            (True, "ok (partial coverage: 22/23 saves)"))

    def test_error_takes_priority_over_skip_or_coverage(self):
        for report in ("skipped: no saves folder\n", "save corpus coverage: 23/23\n"):
            with self.subTest(report=report):
                self.assertEqual(gate.save_corpus_result(report + self.passed, 1),
                                 (False, "FAILED"))

    def test_no_named_test_cannot_pass(self):
        self.assertEqual(gate.save_corpus_result("running 0 tests\n", 0),
                         (False, "FAILED"))

    def test_inconsistent_coverage_cannot_pass(self):
        for coverage in ("0/23", "24/23", "0/0"):
            with self.subTest(coverage=coverage):
                self.assertEqual(gate.save_corpus_result(
                    f"save corpus coverage: {coverage}\n" + self.passed, 0),
                    (False, "FAILED (invalid corpus coverage)"))

    def test_an_unreported_count_is_not_called_complete(self):
        self.assertEqual(gate.save_corpus_result(self.passed, 0),
                         (True, "ok (coverage not reported)"))


if __name__ == "__main__":
    unittest.main()
