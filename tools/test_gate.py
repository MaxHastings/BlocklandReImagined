"""Regression tests for release-gate outcomes; no content or compiler needed."""
import json
import os
import unittest
from unittest.mock import patch
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace

import gate


class GateScope(unittest.TestCase):
    def test_ordinary_commit_uses_scoped_checks_and_full_is_explicit(self):
        def git(*args):
            return {"rev-parse": "base", "rev-list": "tip", "diff": "tools/check.py"}.get(args[0], "")
        with patch.object(gate, "git", side_effect=git), \
             patch.object(gate.subprocess, "run", return_value=SimpleNamespace(returncode=0)), \
             patch.object(gate, "history_check", return_value=[]), \
             patch.object(gate, "oversized_blobs", return_value=[]), \
             patch.object(gate, "gate_root", return_value=Path("unused")), \
             patch.object(gate, "scoped_gate", return_value=True) as scoped, \
             patch.object(gate, "full_gate", return_value=True) as full:
            with patch.object(gate, "FULL_GATE", False):
                self.assertTrue(gate.gate_commit("tip", False))
                scoped.assert_called_once()
                full.assert_not_called()
            with patch.object(gate, "FULL_GATE", True):
                self.assertTrue(gate.gate_commit("tip", False))
                full.assert_called_once_with("tip", Path("unused"), ["tools/check.py"])


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


class PruneStaleArtifacts(unittest.TestCase):
    def test_only_the_newest_generation_of_each_test_binary_stays(self):
        with tempfile.TemporaryDirectory() as folder:
            deps = Path(folder)
            files = {
                "item_ghost-aaaa.exe": 1, "item_ghost-bbbb.exe": 2,
                "item_ghost-aaaa.pdb": 1, "item_ghost-bbbb.pdb": 2,
                "libbri_world-cccc.rlib": 1, "libbri_world-dddd.rlib": 2,
                "mirror_render-eeee.exe": 5,
            }
            for name, mtime in files.items():
                path = deps / name
                path.write_bytes(b"x")
                os.utime(path, (mtime, mtime))
            removed = gate.prune_stale_artifacts(deps)
            self.assertEqual(removed, 2)
            self.assertEqual(
                sorted(entry.name for entry in os.scandir(deps)),
                ["item_ghost-bbbb.exe", "item_ghost-bbbb.pdb",
                 "libbri_world-cccc.rlib", "libbri_world-dddd.rlib", "mirror_render-eeee.exe"])


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
    def test_compile_groups_select_exactly_the_shards_packages_and_targets(self):
        # --workspace --lib used to compile all three libraries on every shard.
        kinds = {"a/lib": "lib", "b/lib": "lib", "c/lib": "lib",
                 "a/flow": "test", "b/flow": "test", "c/other": "test"}
        mine = {"a/lib", "b/lib", "a/flow"}
        groups = gate.ci_build_groups(mine, kinds)
        built = []
        for packages, filters in groups:
            for label, kind in kinds.items():
                package, name = label.split("/")
                selected = "--lib" in filters if kind == "lib" else any(
                    filters[i:i + 2] == [f"--{kind}", name] for i in range(len(filters) - 1))
                if package in packages and selected:
                    built.append(label)
        self.assertCountEqual(built, mine)
        self.assertFalse(any("c" in packages for packages, _ in groups))

    def test_library_only_packages_share_one_build_graph(self):
        kinds = {"a/lib": "lib", "b/lib": "lib", "c/lib": "lib"}
        self.assertEqual(gate.ci_build_groups({"a/lib", "b/lib"}, kinds),
                         [(["a", "b"], ["--lib"])])

    def test_the_shards_run_each_target_exactly_once(self):
        labels = [f"crate{i}/t" for i in range(23)]
        parts = [gate.shard(labels, (k, 4)) for k in range(4)]
        self.assertEqual(sorted(sum(parts, [])), sorted(labels))
        self.assertTrue(all(5 <= len(p) <= 6 for p in parts))

    def test_the_pr_tier_leaves_the_nightly_targets_out_and_full_runs_them(self):
        data = {"ci_skip_target": [{"target": "a/content"}],
                "ci_nightly_target": [{"target": "b/render"}]}
        self.assertEqual(gate.ci_left_out(data, "pr"), {"a/content", "b/render"})
        self.assertEqual(gate.ci_left_out(data, "full"), {"a/content"})

    def test_the_pr_tier_also_leaves_out_the_slow_packages_except_the_kept_targets(self):
        data = {"ci_nightly_target": [{"target": "bri-render/gpu"}],
                "ci_quick": {"slow_packages": ["bri-client", "bri-render"],
                             "keep": ["bri-client/lib"]}}
        labels = ["bri-client/lib", "bri-client/flow", "bri-render/gpu", "bri-render/x", "bri-sim/lib"]
        self.assertEqual(gate.ci_left_out(data, "pr", labels),
                         {"bri-client/flow", "bri-render/gpu", "bri-render/x"})
        self.assertEqual(gate.ci_left_out(data, "main", labels), {"bri-render/gpu"})

    def test_the_pr_tier_leaves_out_the_later_targets_and_main_runs_them(self):
        data = {"ci_quick": {"later": ["bri-addon-import/slayer"]}}
        labels = ["bri-addon-import/slayer", "bri-addon-import/lib"]
        self.assertEqual(gate.ci_left_out(data, "pr", labels), {"bri-addon-import/slayer"})
        self.assertEqual(gate.ci_left_out(data, "main", labels), set())

    def test_every_nightly_target_is_named_once_with_a_reason(self):
        import tomllib
        path = Path(__file__).with_name("gate-known-failures.toml")
        nightly = tomllib.loads(path.read_text(encoding="utf-8")).get("ci_nightly_target", [])
        self.assertTrue(nightly)
        targets = [entry["target"] for entry in nightly]
        self.assertEqual(len(targets), len(set(targets)))
        self.assertTrue(all(entry.get("reason") for entry in nightly))

    def test_the_test_targets_come_from_cargo_metadata_labelled_like_the_build(self):
        def target(name, *kind, test=True):
            return {"name": name, "kind": list(kind), "test": test}
        metadata = {
            "workspace_members": ["a-id", "b-id"],
            "packages": [
                {"id": "a-id", "name": "bri-a", "targets": [
                    target("bri_a", "lib"), target("bri-a", "bin"), target("flow", "test"),
                    target("build-script-build", "custom-build", test=False),
                    target("probe", "example", test=False)]},
                {"id": "b-id", "name": "bri-b", "targets": [
                    target("bri_b", "rlib", "cdylib"), target("flow", "test")]},
                {"id": "dep-id", "name": "serde", "targets": [target("serde", "lib")]},
            ],
        }
        labels = gate.labels_from_metadata(metadata)
        self.assertEqual(labels, {"bri-a/lib": "lib", "bri-a/bri-a": "bin", "bri-a/flow": "test",
                                  "bri-b/lib": "lib", "bri-b/flow": "test"})
        self.assertEqual(gate.target_filters({"bri-a/flow", "bri-b/flow", "bri-a/bri-a"}, labels),
                         ["--bin", "bri-a", "--test", "flow"])
        self.assertEqual(gate.target_filters({"bri-b/lib"}, labels), ["--lib"])

    def test_weighted_shards_balance_the_heavy_targets(self):
        weights = {"a/slow1": 100.0, "a/slow2": 90.0, "a/slow3": 80.0, "a/slow4": 70.0}
        labels = sorted(weights) + [f"b/t{i}" for i in range(8)]
        weights.update({label: 1.0 for label in labels if label.startswith("b/")})
        parts = [gate.shard(labels, (k, 4), weights) for k in range(4)]
        self.assertEqual(sorted(sum(parts, [])), sorted(labels))
        self.assertTrue(all(sum(label.startswith("a/") for label in p) == 1 for p in parts))

    def test_a_cost_per_binary_spreads_instant_targets_evenly(self):
        labels = [f"a/t{i}" for i in range(9)] + ["a/slow"]
        weights = {"a/slow": 30.0, **{label: 0.0 for label in labels if label != "a/slow"}}
        parts = [gate.shard(labels, (k, 2), weights, per_binary=10.0) for k in range(2)]
        self.assertEqual(sorted(len(p) for p in parts), [4, 6])
        bare = [gate.shard(labels, (k, 2), weights) for k in range(2)]
        self.assertEqual(sorted(len(p) for p in bare), [1, 9])

    def test_the_committed_weights_name_only_real_shapes(self):
        path = Path(__file__).with_name("ci-timings.json")
        weights = json.loads(path.read_text(encoding="utf-8"))
        self.assertTrue(weights)
        self.assertTrue(all(isinstance(v, float) and v >= 0 for v in weights.values()))

    def test_one_shard_is_all_of_them(self):
        self.assertEqual(gate.shard(["b", "a"], (0, 1)), ["a", "b"])

    def test_a_shard_past_the_count_is_refused(self):
        with self.assertRaises(Exception):
            gate.parse_shard("4/4")


class CiKnownFailures(unittest.TestCase):
    known = {"capture_the_flag": {"owner": "someone"}}

    def test_a_listed_failure_is_tolerated_as_the_gate_does(self):
        output = "test capture_the_flag ... FAILED\ntest other ... ok\n"
        self.assertTrue(gate.ci_tolerated("bri-chaos/bot_gauntlet", output, self.known))

    def test_an_unlisted_failure_beside_a_listed_one_fails(self):
        output = "test capture_the_flag ... FAILED\ntest other ... FAILED\n"
        self.assertFalse(gate.ci_tolerated("bri-chaos/bot_gauntlet", output, self.known))

    def test_a_hang_or_crash_is_never_tolerated(self):
        for name in ("gate_timeout", "process_exit"):
            output = f"test bri-chaos/bot_gauntlet::{name} ... FAILED\n"
            self.assertFalse(gate.ci_tolerated("bri-chaos/bot_gauntlet", output, self.known))
        self.assertFalse(gate.ci_tolerated("bri-chaos/bot_gauntlet", "", self.known))


class CiFlakyRetries(unittest.TestCase):
    binaries = [("bri-client/avatar_editor_preview", "avatar-preview.exe", ".", "Running preview")]

    def test_a_named_failure_that_passes_alone_is_reported_as_flaky(self):
        label = self.binaries[0][0]
        failed = [(label, "test portrait::synthetic ... FAILED\n")]
        with patch.object(gate, "run_binary", return_value=(
                "test portrait::synthetic ... ok\n", 0)) as retry:
            remaining = gate.retry_ci_failures(failed, self.binaries)
        self.assertEqual(remaining, [])
        retry.assert_called_once_with(label, "avatar-preview.exe", ["--exact", "portrait::synthetic"], ".")

    def test_a_failure_that_repeats_alone_still_blocks_ci(self):
        label = self.binaries[0][0]
        failed = [(label, "test portrait::synthetic ... FAILED\n")]
        with patch.object(gate, "run_binary", return_value=(
                "test portrait::synthetic ... FAILED\n", 101)):
            remaining = gate.retry_ci_failures(failed, self.binaries)
        self.assertEqual(len(remaining), 1)
        self.assertIn("test portrait::synthetic ... FAILED", remaining[0][1])

    def test_an_unnamed_process_failure_is_not_retried(self):
        label = self.binaries[0][0]
        failed = [(label, "process exited without a test report\n")]
        with patch.object(gate, "run_binary") as retry:
            remaining = gate.retry_ci_failures(failed, self.binaries)
        self.assertEqual(remaining, failed)
        retry.assert_not_called()


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


class PerfBudget(unittest.TestCase):
    output = ("world=\"Collage\" map=x bricks=5\n"
              "BRI_STRESS_SUMMARY {\"world\": \"Collage\", \"bots\": 32, \"ticks\": 3600, "
              "\"build\": \"release\", \"step_mean_ms\": 6.25, \"step_p99_ms\": 14.0}\n"
              "test bots_fight_a_free_for_all_on_real_content ... ok\n")

    def test_the_summary_line_is_read(self):
        summary = gate.parse_stress_summary(self.output)
        self.assertEqual(summary["step_mean_ms"], 6.25)
        self.assertIsNone(gate.parse_stress_summary("step_ms mean=6.0\n"))

    def test_a_mean_over_budget_fails_and_under_passes(self):
        self.assertTrue(gate.perf_verdict("collage", 7.0, self.output, 0)[0])
        ok, message = gate.perf_verdict("collage", 6.0, self.output, 0)
        self.assertFalse(ok)
        self.assertIn("OVER BUDGET", message)

    def test_a_debug_build_a_crash_or_a_missing_line_fails(self):
        debug = self.output.replace('"release"', '"debug"')
        self.assertFalse(gate.perf_verdict("collage", 9.0, debug, 0)[0])
        self.assertFalse(gate.perf_verdict("collage", 9.0, self.output, 101)[0])
        self.assertFalse(gate.perf_verdict("collage", 9.0, "test x ... ok\n", 0)[0])

    def test_a_harness_skip_is_a_skip(self):
        ok, message = gate.perf_verdict("afghan", 5.5, "skipped: content not found (no save)\n", 0)
        self.assertTrue(ok)
        self.assertIn("SKIPPED", message)

    def test_budgets_default_and_env_overrides(self):
        self.assertEqual(gate.perf_budget_ms("collage", {}), 7.0)
        self.assertEqual(gate.perf_budget_ms("afghan", {}), 5.5)
        self.assertEqual(gate.perf_budget_ms("afghan", {"BRI_PERF_BUDGET_AFGHAN_MS": "6.5"}), 6.5)
        with self.assertRaises(gate.GateError):
            gate.perf_budget_ms("afghan", {"BRI_PERF_BUDGET_AFGHAN_MS": "fast"})

    def test_saves_are_found_by_name_and_missing_ones_skip_or_fall_back(self):
        with tempfile.TemporaryDirectory() as folder:
            saves = Path(folder) / "saves"
            (saves / "Bedroom").mkdir(parents=True)
            (saves / "Bedroom" / "Crazy Collage Madness.world.json").write_text("{}")
            self.assertEqual(gate.find_save("Crazy Collage Madness ", [saves]).name,
                             "Crazy Collage Madness.world.json")
            content = Path(folder) / "content"
            content.mkdir()
            plan = {name: (env, what) for name, _, env, what in
                    gate.perf_plan({"BRI_CONTENT": str(content), "BRI_PERF_SAVES": str(saves)})}
            self.assertTrue(plan["collage"][0]["BRI_STRESS_WORLD"].endswith(
                "Crazy Collage Madness.world.json"))
            self.assertEqual(plan["collage"][0]["BRI_STRESS_BOTS"], "32")
            self.assertEqual(plan["afghan"][0]["BRI_STRESS_WORLD_NAME"], "Afghanistan DM")
            self.assertEqual(plan["afghan"][0]["BRI_STRESS_TEAMS"], "2")
            empty = Path(folder) / "none"
            plan = {name: env for name, _, env, _ in
                    gate.perf_plan({"BRI_CONTENT": str(content), "BRI_PERF_SAVES": str(empty)})}
            self.assertIsNone(plan["collage"])
            self.assertTrue(all(env is None for _, _, env, _ in
                                gate.perf_plan({"BRI_CONTENT": str(empty)})))


if __name__ == "__main__":
    unittest.main()
