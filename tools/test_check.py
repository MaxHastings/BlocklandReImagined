"""Tests for tools/check.py: which crates a change touches and what each
tier runs for them. No compiler or git history needed."""
import unittest

import check

DIRS = {
    "bri-world": "crates/world",
    "bri-sim": "crates/sim",
    "bri-net": "crates/net",
    "bri-client": "crates/client",
    "bri-client-sandbox": "crates/client-sandbox",
    "bri-docs": "crates/docs",
}
# world <- sim <- net <- client; the sandbox depends on the client only.
REVERSE = {
    "bri-world": {"bri-sim"},
    "bri-sim": {"bri-net"},
    "bri-net": {"bri-client"},
    "bri-client": {"bri-client-sandbox"},
    "bri-client-sandbox": set(),
    "bri-docs": set(),
}


class Changed(unittest.TestCase):
    def test_files_belong_to_the_crate_whose_folder_holds_them(self):
        files = ["crates/sim/src/lib.rs", "crates/net/tests/x.rs", "docs/STATUS.md"]
        self.assertEqual(check.changed_crates(files, DIRS), {"bri-sim", "bri-net"})

    def test_a_nested_crate_wins_over_the_folder_name_it_shares(self):
        # crates/client-sandbox starts with crates/client but is not inside it.
        files = ["crates/client-sandbox/src/lib.rs"]
        self.assertEqual(check.changed_crates(files, DIRS), {"bri-client-sandbox"})
        self.assertEqual(check.changed_crates(["crates/client/x"], DIRS), {"bri-client"})

    def test_a_workspace_file_changes_every_crate(self):
        self.assertEqual(check.changed_crates(["Cargo.lock"], DIRS), set(DIRS))
        self.assertEqual(check.changed_crates(["rust-toolchain.toml"], DIRS), set(DIRS))

    def test_documentation_alone_changes_no_crate(self):
        self.assertEqual(check.changed_crates(["docs/a.md", "tools/gate.py"], DIRS), set())


class Dependents(unittest.TestCase):
    def test_closure_follows_dependents_all_the_way_or_to_a_depth(self):
        self.assertEqual(
            check.closure({"bri-world"}, REVERSE),
            {"bri-world", "bri-sim", "bri-net", "bri-client", "bri-client-sandbox"})
        self.assertEqual(check.closure({"bri-world"}, REVERSE, 1), {"bri-world", "bri-sim"})
        self.assertEqual(check.closure({"bri-world"}, REVERSE, 0), {"bri-world"})
        self.assertEqual(check.closure({"bri-docs"}, REVERSE), {"bri-docs"})


class Plans(unittest.TestCase):
    def labels(self, tier, crates, **kw):
        return [label for label, _ in check.plan(tier, crates, REVERSE, **kw)]

    def test_quick_is_formatting_and_unit_tests_of_the_changed_crates_only(self):
        self.assertEqual(
            self.labels("quick", {"bri-sim"}), ["fmt bri-sim", "unit bri-sim"])

    def test_focused_adds_clippy_and_the_dependents_tests(self):
        steps = dict(check.plan("focused", {"bri-world"}, REVERSE, depth=1))
        self.assertIn("clippy bri-world", steps)
        self.assertNotIn("clippy bri-sim", steps)
        self.assertEqual(steps["tests bri-sim"], ["cargo", "test", "-p", "bri-sim", "--locked"])
        self.assertNotIn("tests bri-net", steps)

    def test_render_and_app_crates_run_unit_tests_only_unless_named(self):
        steps = dict(check.plan("focused", {"bri-net"}, REVERSE))
        self.assertEqual(
            steps["unit bri-client"], ["cargo", "test", "-p", "bri-client", "--lib", "--locked"])
        self.assertNotIn("tests bri-client", steps)
        named = dict(check.plan("focused", {"bri-net"}, REVERSE,
                                extra_tests=[("bri-client", "large_build_perf")]))
        self.assertEqual(
            named["test bri-client:large_build_perf"],
            ["cargo", "test", "-p", "bri-client", "--locked", "--test", "large_build_perf"])

    def test_every_cargo_call_names_one_crate_so_windows_limits_hold(self):
        for _, argv in check.plan("focused", set(DIRS), REVERSE):
            self.assertLessEqual(argv.count("-p") + argv.count("--package"), 1, argv)
            self.assertLess(len(" ".join(argv)), 200)

    def test_soak_selects_only_the_labelled_long_cases(self):
        for _, argv in check.soak_plan({"bri-net"}):
            self.assertIn("--include-ignored", argv)
            self.assertTrue(argv[-1].endswith("_"), argv)

    def test_tests_argument_needs_crate_and_target(self):
        self.assertEqual(check.parse_tests(["bri-net:soak"]), [("bri-net", "soak")])
        with self.assertRaises(SystemExit):
            check.parse_tests(["bri-net"])


if __name__ == "__main__":
    unittest.main()
