"""Regression tests for release-gate outcomes; no content or compiler needed."""
import os
import unittest
from unittest.mock import patch
from pathlib import Path
import sys
import tempfile

import gate


class ProcessExit(unittest.TestCase):
    def test_a_crash_with_no_failed_test_is_a_failure(self):
        output = "running 3 tests\ntest a ... ok\nthread 'b' has overflowed its stack\n"
        line = gate.exit_failure("crate/t", output, 0xC00000FD)
        self.assertIn("test crate/t::process_exit ... FAILED", line)
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / "gate.log"
            log.write_text("===== test =====\n     Running tests/t.rs\n" + output + line,
                           encoding="utf-8")
            failed, _ = gate.parse_failures(log)
        self.assertEqual(failed, {"tests/t.rs::crate/t::process_exit"})
        self.assertTrue(gate.whole_process_failure("crate/t::process_exit"))

    def test_named_failures_and_clean_exits_add_nothing(self):
        self.assertEqual(gate.exit_failure("c/t", "test a ... FAILED\n", 101), "")
        self.assertEqual(gate.exit_failure("c/t", "test a ... ok\n", 0), "")


class TestPlanning(unittest.TestCase):
    binaries = [("a/x", "x.exe", ".", "Running x"), ("b/y", "y.exe", ".", "Running y"),
                ("c/z", "z.exe", ".", "Running z")]

    def test_the_slowest_and_the_unmeasured_start_first(self):
        units = gate.plan_units(self.binaries, ["--include-ignored"], {"a/x": 5.0, "b/y": 9.0})
        self.assertEqual([u[0] for u in units], ["c/z", "b/y", "a/x"])
        self.assertTrue(all(u[4] == ["--include-ignored"] for u in units))

    def test_timings_keep_earlier_binaries_they_did_not_run(self):
        path = Path(os.environ.get("TEMP", ".")) / f"gate-timings-test-{os.getpid()}.json"
        try:
            gate.save_timings(path, {"a/x": 3.0})
            gate.save_timings(path, {"b/y": 4.0})
            self.assertEqual(gate.load_timings(path), {"a/x": 3.0, "b/y": 4.0})
        finally:
            path.unlink(missing_ok=True)
        self.assertEqual(gate.load_timings(path), {})


class CiShards(unittest.TestCase):
    def test_the_shards_run_each_target_exactly_once(self):
        labels = [f"crate{i}/t" for i in range(23)]
        parts = [gate.shard(labels, (k, 4)) for k in range(4)]
        self.assertEqual(sorted(sum(parts, [])), sorted(labels))
        self.assertTrue(all(5 <= len(p) <= 6 for p in parts))

    def test_one_shard_is_all_of_them(self):
        self.assertEqual(gate.shard(["b", "a"], (0, 1)), ["a", "b"])

    def test_a_shard_past_the_count_is_refused(self):
        with self.assertRaises(Exception):
            gate.parse_shard("4/4")


class GateEnvironment(unittest.TestCase):
    def test_content_backed_tests_get_the_main_content(self):
        env = gate_env_for({"PATH": "x", "CARGO_TARGET_DIR": "elsewhere"})
        self.assertEqual(env["BRI_CONTENT"], str(Path("main") / "content"))
        self.assertNotIn("CARGO_TARGET_DIR", env)
        self.assertEqual(env["CARGO_INCREMENTAL"], "0")

    def test_a_chosen_content_folder_is_kept(self):
        env = gate_env_for({"BRI_CONTENT": "mine"})
        self.assertEqual(env["BRI_CONTENT"], "mine")


class BinaryEnvironment(unittest.TestCase):
    def test_a_test_binary_runs_in_the_gate_environment(self):
        env = gate_env_for({"PATH": os.environ.get("PATH", "")})
        script = "import os; print(os.environ.get('BRI_CONTENT', 'unset'))"
        output, code = gate.run_binary("probe", sys.executable, ["-c", script], ".", env)
        self.assertEqual(code, 0)
        self.assertEqual(output.strip(), str(Path("main") / "content"))


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


class History(unittest.TestCase):
    def test_a_revert_and_a_reapply_undo_on_purpose(self):
        self.assertTrue(gate.undoes_on_purpose('Revert "WIP: bots drive"'))
        self.assertTrue(gate.undoes_on_purpose('Reapply "WIP: bots drive"'))

    def test_other_commits_do_not(self):
        self.assertFalse(gate.undoes_on_purpose("Reapplying the old route costs"))
        self.assertFalse(gate.undoes_on_purpose("Merge teammate damage"))


class PassedTrees(unittest.TestCase):
    """A tree that passed is not gated again: not after a reworded commit
    message, and not after a change that only touches documentation."""

    def setUp(self):
        # Git leaves its objects read-only, which Windows refuses to delete.
        self.folder = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(self.folder.cleanup)
        self.repo = Path(self.folder.name) / "repo"
        self.repo.mkdir()
        self.root = Path(self.folder.name) / "gate"
        self.root.mkdir()
        previous = os.getcwd()
        os.chdir(self.repo)
        self.addCleanup(os.chdir, previous)
        for args in (["init", "-q"], ["config", "user.email", "t@example.com"],
                     ["config", "user.name", "t"], ["config", "commit.gpgsign", "false"]):
            gate.git(*args)
        self.write("src/lib.rs", "fn a() {}\n")
        self.passed = self.commit("first")

    def write(self, path, text):
        (self.repo / path).parent.mkdir(parents=True, exist_ok=True)
        (self.repo / path).write_text(text, encoding="utf-8")

    def commit(self, message):
        gate.git("add", "-A")
        gate.git("commit", "-q", "--allow-empty", "-m", message)
        return gate.git("rev-parse", "HEAD").strip()

    def test_a_reworded_commit_reuses_the_pass(self):
        gate.record_pass(self.root, self.passed, "inputs", corpus=False)
        gate.git("commit", "-q", "--amend", "-m", "first\n\nGate-Allow-Undo: src/lib.rs")
        reworded = gate.git("rev-parse", "HEAD").strip()
        self.assertNotEqual(reworded, self.passed)
        self.assertEqual(gate.reusable_pass(self.root, reworded, "inputs", False),
                         (self.passed, True))

    def test_documentation_changes_reuse_the_pass(self):
        gate.record_pass(self.root, self.passed, "inputs", corpus=False)
        self.write("docs/progress/entry.md", "notes\n")
        self.write("README.md", "readme\n")
        docs = self.commit("notes")
        self.assertEqual(gate.reusable_pass(self.root, docs, "inputs", False),
                         (self.passed, False))

    def test_code_and_protocol_changes_do_not(self):
        gate.record_pass(self.root, self.passed, "inputs", corpus=False)
        self.write("crates/net/protocol-changes/0042-new.md", "a wire change\n")
        protocol = self.commit("wire")
        self.assertIsNone(gate.reusable_pass(self.root, protocol, "inputs", False))
        gate.git("reset", "-q", "--hard", self.passed)
        self.write("src/lib.rs", "fn b() {}\n")
        code = self.commit("code")
        self.assertIsNone(gate.reusable_pass(self.root, code, "inputs", False))

    def test_code_moved_into_a_markdown_file_does_not(self):
        gate.record_pass(self.root, self.passed, "inputs", corpus=False)
        (self.repo / "docs").mkdir()
        gate.git("mv", "src/lib.rs", "docs/lib.md")
        moved = self.commit("move")
        self.assertIn("src/lib.rs", gate.git("diff", "--name-only", "--no-renames", self.passed, moved))
        self.assertIsNone(gate.reusable_pass(self.root, moved, "inputs", False))

    def test_other_inputs_or_a_missing_corpus_do_not(self):
        gate.record_pass(self.root, self.passed, "inputs", corpus=False)
        self.assertIsNone(gate.reusable_pass(self.root, self.passed, "other", False))
        self.assertIsNone(gate.reusable_pass(self.root, self.passed, None, False))
        self.assertIsNone(gate.reusable_pass(self.root, self.passed, "inputs", True))
        gate.record_pass(self.root, self.passed, "inputs", corpus=True)
        self.assertEqual(gate.reusable_pass(self.root, self.passed, "inputs", True),
                         (self.passed, True))

    def test_unreadable_inputs_record_nothing(self):
        gate.record_pass(self.root, self.passed, None, corpus=True)
        self.assertFalse((self.root / gate.PASSED_TREES).exists())


class ContentFingerprint(unittest.TestCase):
    def test_packs_and_stamps_change_it_but_installed_add_ons_do_not(self):
        with tempfile.TemporaryDirectory() as folder:
            content = Path(folder)
            (content / "bricks-pass-001").mkdir()
            (content / "bricks-pass-001" / "manifest.json").write_text("{}", encoding="utf-8")
            (content / "_regeneration" / "stamps").mkdir(parents=True)
            stamp = content / "_regeneration" / "stamps" / "worlds-pass-006.json"
            stamp.write_text('{"inputs": "a"}', encoding="utf-8")
            first = gate.content_fingerprint(content)
            (content / "addons" / "Tool_Duplicator").mkdir(parents=True)
            self.assertEqual(gate.content_fingerprint(content), first)
            stamp.write_text('{"inputs": "b"}', encoding="utf-8")
            restamped = gate.content_fingerprint(content)
            self.assertNotEqual(restamped, first)
            (content / "worlds-pass-007").mkdir()
            self.assertNotEqual(gate.content_fingerprint(content), restamped)
        self.assertIsNone(gate.content_fingerprint(Path(folder) / "gone"))


if __name__ == "__main__":
    unittest.main()
