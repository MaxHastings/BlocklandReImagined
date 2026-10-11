#!/usr/bin/env python3
"""Tiered checks: the cheapest one that covers what you changed.

    python tools/check.py quick        formatting + unit tests of the changed crates
    python tools/check.py focused      quick + clippy and non-GPU tests of the changed
                                       crates and what depends on them
    python tools/check.py full         the whole push gate (tools/gate.py), once per
                                       integrated candidate
    python tools/check.py plan [TIER]  print what a tier would run, run nothing

"Changed" is every crate with a file differing from the merge base with
origin/main (or from --base REV), plus uncommitted and untracked files.
A change to a workspace-level file (Cargo.toml, Cargo.lock, the toolchain,
clippy.toml) changes every crate. --crates a,b names crates instead.

Tiers (docs/testing-tiers.md):
  quick    seconds to two minutes on a warm target dir: `cargo fmt --check` and
           `cargo test --lib` (unit tests) for each changed crate.
  focused  a few minutes: `cargo clippy --all-targets -D warnings` and every
           integration test that needs no window or GPU, for each changed crate
           and each crate that depends on one. Crates whose tests render
           (HEAVY_CRATES) run their unit tests only; name the render or app
           test you need with --tests bri-client:large_build_perf.
  full     tools/gate.py: build, clippy, bri-client --check and every test
           with --include-ignored. Nothing is left out; a tier is a way to
           skip work you have not changed, never a way to skip a test.

Long soak and endurance cases (the `soak_`, `endurance_` and `stress_` test
names, see SOAK_PREFIXES) are `#[ignore]`d where they run for minutes. The
gate runs them (it passes --include-ignored); `check.py soak` runs just
those, for the crates it names, when you want them without a full gate.

Each cargo call names one crate so a long list of crates never exceeds the
Windows command-line limit (tools/check_format.py does the same).
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
# Crates whose integration tests open an offscreen renderer or a whole App:
# slow, GPU-bound, and run in full by the gate. `focused` runs only their
# unit tests unless --tests names one.
HEAVY_CRATES = {"bri-client", "bri-render"}
# Files at the workspace level that change every crate.
WORKSPACE_FILES = {
    "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "clippy.toml",
    ".cargo/config.toml",
}
# Tests with these name prefixes are long-running and deliberately labelled.
SOAK_PREFIXES = ("soak_", "endurance_", "stress_")
TIERS = ("quick", "focused", "full")


def run(command, cwd=ROOT, check=True):
    return subprocess.run(command, cwd=cwd, capture_output=True, text=True, check=check)


def metadata():
    data = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked"]).stdout)
    members = set(data["workspace_members"])
    packages = {p["id"]: p for p in data["packages"] if p["id"] in members}
    return packages


def crate_dirs(packages):
    """{crate name: its folder relative to the repository root, with /}."""
    return {
        p["name"]: Path(p["manifest_path"]).parent.resolve().relative_to(ROOT).as_posix()
        for p in packages.values()
    }


def dependents(packages):
    """{crate name: names of the workspace crates that depend on it, directly}."""
    names = {p["id"]: p["name"] for p in packages.values()}
    reverse = {name: set() for name in names.values()}
    for p in packages.values():
        for dep in p["dependencies"]:
            if dep["name"] in reverse and dep.get("kind") != "build":
                reverse[dep["name"]].add(p["name"])
    return reverse


def changed_files(base):
    """Paths that differ from the merge base with `base`, plus working changes."""
    paths = set()
    merge_base = run(["git", "merge-base", "HEAD", base], check=False)
    spec = [merge_base.stdout.strip()] if merge_base.returncode == 0 else []
    if spec:
        paths |= set(run(["git", "diff", "--name-only", spec[0], "HEAD"]).stdout.split())
    paths |= set(run(["git", "diff", "--name-only", "HEAD"]).stdout.split())
    paths |= set(run(["git", "ls-files", "--others", "--exclude-standard"]).stdout.split())
    return sorted(paths)


def changed_crates(files, dirs):
    """The crates `files` belong to; every crate for a workspace-level file."""
    if any(f in WORKSPACE_FILES for f in files):
        return set(dirs)
    # Longest folder first, so a nested crate wins over its parent.
    ordered = sorted(dirs.items(), key=lambda item: -len(item[1]))
    found = set()
    for f in files:
        for name, folder in ordered:
            if f == folder or f.startswith(folder + "/"):
                found.add(name)
                break
    return found


def closure(names, reverse, depth=None):
    """`names` and the workspace crates that depend on them: all of them, or
    only as far as `depth` steps of "depends on" (1 is direct dependents)."""
    seen, level, steps = set(), set(names), 0
    while level and (depth is None or steps <= depth):
        seen |= level
        level = {d for name in level for d in reverse.get(name, ())} - seen
        steps += 1
    return seen


def parse_tests(texts):
    """[(crate, test target)] from --tests crate:target arguments."""
    out = []
    for text in texts:
        crate, _, target = text.partition(":")
        if not target:
            raise SystemExit(f"--tests {text}: write it as crate:target")
        out.append((crate, target))
    return out


def plan(tier, crates, reverse, extra_tests=(), depth=None):
    """The commands a tier runs, as (label, argv) in order."""
    steps = []
    ordered = sorted(crates)
    for name in ordered:
        steps.append((f"fmt {name}", ["cargo", "fmt", "--package", name, "--check"]))
    if tier == "quick":
        for name in ordered:
            steps.append((f"unit {name}", ["cargo", "test", "-p", name, "--lib", "--locked"]))
        return steps
    if tier != "focused":
        raise ValueError(tier)
    affected = sorted(closure(crates, reverse, depth))
    for name in ordered:
        steps.append((f"clippy {name}", [
            "cargo", "clippy", "-p", name, "--all-targets", "--locked", "--", "-D", "warnings"]))
    for name in affected:
        if name in HEAVY_CRATES:
            steps.append((f"unit {name}", ["cargo", "test", "-p", name, "--lib", "--locked"]))
        else:
            steps.append((f"tests {name}", ["cargo", "test", "-p", name, "--locked"]))
    for crate, target in extra_tests:
        steps.append((f"test {crate}:{target}", [
            "cargo", "test", "-p", crate, "--locked", "--test", target]))
    return steps


def soak_plan(crates):
    """The labelled long cases of `crates`: tests with a soak prefix, ignored or not."""
    steps = []
    for name in sorted(crates):
        for prefix in SOAK_PREFIXES:
            steps.append((f"soak {name} {prefix}*", [
                "cargo", "test", "-p", name, "--locked", "--release", "--",
                "--include-ignored", prefix]))
    return steps


def execute(steps):
    """Run steps in order, stopping at the first failure. Returns an exit code."""
    started = time.monotonic()
    timings = []
    for label, argv in steps:
        print(f"[check] {label}: {' '.join(argv)}", flush=True)
        t = time.monotonic()
        code = subprocess.run(argv, cwd=ROOT).returncode
        timings.append((label, time.monotonic() - t))
        if code:
            print(f"[check] FAILED: {label} (exit {code})", flush=True)
            summary(timings, started)
            return code
    summary(timings, started)
    return 0


def summary(timings, started):
    print(f"[check] {len(timings)} step(s) in {time.monotonic() - started:.0f}s", flush=True)
    for label, seconds in sorted(timings, key=lambda t: -t[1])[:5]:
        print(f"[check]   {seconds:6.1f}s  {label}", flush=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("tier", choices=[*TIERS, "soak", "plan"])
    parser.add_argument("what", nargs="?", choices=TIERS, default="focused",
                        help="for `plan`: the tier to print (default focused)")
    parser.add_argument("--base", default="origin/main", help="compare against this revision")
    parser.add_argument("--crates", help="comma-separated crates instead of the changed ones")
    parser.add_argument("--tests", action="append", default=[], metavar="CRATE:TARGET",
                        help="also run this integration test target (focused)")
    parser.add_argument("--depth", type=int, metavar="N",
                        help="focused: test dependents only N steps away (1 = direct), default all")
    parser.add_argument("--push", action="store_true", help="full: rebase, gate and push")
    args = parser.parse_args(argv)
    tier = args.what if args.tier == "plan" else args.tier
    if tier == "full" and args.tier != "plan":
        gate = [sys.executable, str(ROOT / "tools" / "gate.py")]
        return subprocess.run(gate + (["--push"] if args.push else []), cwd=ROOT).returncode
    packages = metadata()
    dirs = crate_dirs(packages)
    if args.crates:
        crates = set(args.crates.split(","))
        unknown = crates - set(dirs)
        if unknown:
            raise SystemExit(f"unknown crate(s): {', '.join(sorted(unknown))}")
    else:
        files = changed_files(args.base)
        crates = changed_crates(files, dirs)
        if not crates:
            print("[check] no crate changed; nothing to run")
            return 0
    print(f"[check] changed crates: {', '.join(sorted(crates))}", flush=True)
    if args.tier == "soak":
        steps = soak_plan(crates)
    elif tier == "full":
        steps = [("gate", [sys.executable, "tools/gate.py"])]
    else:
        steps = plan(tier, crates, dependents(packages), parse_tests(args.tests), args.depth)
    if args.tier == "plan":
        for label, command in steps:
            print(f"{label}: {' '.join(command)}")
        return 0
    return execute(steps)


if __name__ == "__main__":
    sys.exit(main())
