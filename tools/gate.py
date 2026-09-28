"""Push gate for main: nothing lands on origin/main unless it passes.

    python tools/gate.py                 gate HEAD as if pushing it to main
    python tools/gate.py --push          rebase, gate and push HEAD to main (preferred)
    python tools/gate.py --diff-only     only the fast history checks
    python tools/gate.py --install-hook  install the shared pre-push hook
    python tools/gate.py --hook ...      (called by the pre-push hook)
    python tools/gate.py --history-range BASE TIP   (history check only; CI)
    python tools/gate.py --ci-test       content-free tests only (CI)

Checks, cheapest first, on the exact commit being pushed:
  1. the commit already contains the latest origin/main (rebase first)
  2. history sanity: no pushed commit deletes or undoes recent main work,
     and a protocol VERSION change only ever increases it
  3. cargo build --workspace --all-targets --locked
  4. cargo clippy --workspace --all-targets --locked -- -D warnings
  5. bri-client --check against the main checkout's content
  6. cargo test --workspace --locked -- --include-ignored, with failures listed
     in tools/gate-known-failures.toml tolerated

Builds run in one shared gate worktree and target dir next to the main
checkout (default ../.bri-gate), serialized by a lock, so parallel sessions
queue instead of doing 25 cold builds at once.

Allowing an intentional undo: add a trailer line to the commit message,
    Gate-Allow-Undo: path/to/file      (repeatable; "*" allows every path)
Commits made by `git revert` ("Revert ...") are allowed automatically.
"""
import argparse
import concurrent.futures
import contextlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import threading
import tomllib

ZERO = "0" * 40
MAIN_REF = "refs/heads/main"
WINDOW_DAYS = 4
WINDOW_COMMITS = 200
UNDO_MIN_LINES = 10
UNDO_FRACTION = 0.6
# No single file this large belongs in the repository.
MAX_BLOB_BYTES = 25 * 2**20
PROTOCOL_FILE = "crates/net/src/protocol.rs"
PROTOCOL_RE = re.compile(r"^pub const VERSION: u32 = (\d+);", re.M)
LOCK_STALE_SECONDS = 10 * 60
LOCK_HELD = False
TEST_JOBS = 8
# A test binary still running after this long is stopped and fails the run.
BINARY_TIMEOUT = 10 * 60
HEAVY_JOBS = 3
HEAVY_PREFIXES = ("bri-client/", "bri-render/")
DOC_SUFFIXES = (".md",)
# Cargo never deletes superseded artifacts, so the gate's target dir kept
# every old test binary (953 executables for 149 targets, 296 GB). Past this
# much in target/debug/deps the gate starts from an empty target dir, which
# sccache refills quickly.
TARGET_DEPS_CAP = 40 * 2**30


class GateError(Exception):
    pass


def git(*args, cwd=None, check=True):
    result = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
    )
    if check and result.returncode != 0:
        raise GateError(f"git {' '.join(args)} failed:\n{result.stderr.strip()}")
    return result.stdout


def say(message):
    print(f"[gate] {message}", flush=True)


def common_dir():
    return Path(os.path.abspath(git("rev-parse", "--git-common-dir").strip()))


def main_checkout():
    return common_dir().parent


def gate_root():
    return Path(os.environ.get("BRI_GATE_DIR") or main_checkout().parent / ".bri-gate")


# ---------------------------------------------------------------- history


def trailers(sha):
    body = git("log", "-1", "--format=%B", sha)
    allowed = set()
    for line in body.splitlines():
        match = re.match(r"\s*Gate-Allow-Undo:\s*(\S+)", line)
        if match:
            allowed.add(match.group(1).replace("\\", "/"))
    return body, allowed


def meaningful(line):
    text = line.strip()
    return len(text) > 3 and text not in {"}", "{", "});", "},", "]", "],", ")", ");"}


def blob_lines(spec):
    result = subprocess.run(["git", "show", spec], capture_output=True)
    if result.returncode != 0:
        return None
    return result.stdout.decode("utf-8", "replace").splitlines()


def parse_patch(text):
    """Parse -U0 patches into [(header, {path: removed}, {path: added}, {path: status})].

    Commits are separated by lines starting with "@@commit " (the header).
    """
    commits = []
    removed = added = status = None
    path = None
    for line in text.splitlines():
        if line.startswith("@@commit "):
            removed, added, status = {}, {}, {}
            commits.append((line[9:], removed, added, status))
            path = None
        elif line.startswith("diff --git "):
            path = line.split(" b/", 1)[1]
            removed[path], added[path], status[path] = set(), set(), "M"
        elif path is None:
            continue
        elif line.startswith("deleted file mode"):
            status[path] = "D"
        elif line.startswith("new file mode"):
            status[path] = "A"
        elif line.startswith("-") and not line.startswith("---") and meaningful(line[1:]):
            removed[path].add(line[1:].strip())
        elif line.startswith("+") and not line.startswith("+++") and meaningful(line[1:]):
            added[path].add(line[1:].strip())
    return commits


class Blobs:
    """Line sets of path@commit through one `git cat-file --batch` process."""

    def __init__(self):
        self.process = subprocess.Popen(["git", "cat-file", "--batch"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE)
        self.cache = {}

    def lines(self, commit, path):
        key = f"{commit}:{path}"
        if key not in self.cache:
            self.process.stdin.write(key.encode() + b"\n")
            self.process.stdin.flush()
            header = self.process.stdout.readline().split()
            lines = set()
            if header and header[-1] != b"missing" and header[-2:-1] != [b"ambiguous"]:
                data = self.process.stdout.read(int(header[-1]) + 1)
                lines = {line.strip() for line in data.decode("utf-8", "replace").splitlines()}
            self.cache[key] = lines
        return self.cache[key]

    def close(self):
        self.process.stdin.close()
        self.process.wait()


def oversized_blobs(base, tip):
    """Files anywhere in base..tip's new history that do not belong in git:
    any blob over MAX_BLOB_BYTES, or any path inside a target/ folder."""
    objects = git("rev-list", "--objects", f"{base}..{tip}")
    process = subprocess.run(
        ["git", "cat-file", "--batch-check=%(objecttype) %(objectsize) %(objectname)"],
        input="".join(line.split(" ", 1)[0] + "\n" for line in objects.splitlines()),
        capture_output=True, text=True, encoding="utf-8", errors="replace")
    paths = {}
    for line in objects.splitlines():
        name, _, path = line.partition(" ")
        if path:
            paths[name] = path
    found = []
    for line in process.stdout.splitlines():
        kind, size, name = line.split(" ", 2)
        path = paths.get(name, "")
        if kind != "blob":
            continue
        in_target = "/target/" in f"/{path}" and not path.startswith("crates/")
        if int(size) > MAX_BLOB_BYTES or in_target:
            found.append(f"{path or name} ({int(size) / 2**20:.1f} MiB)")
    return found


def history_check(base, tip):
    """Refuse pushed commits that undo most of a recent origin/main commit.

    A commit built from a stale tree removes the lines that newer main commits
    added. For every recent main commit X whose lines a pushed commit C removes,
    compare against all of X's lines that still survive in C's parent: if C
    removes most of them (and does not re-add them elsewhere, as a move would),
    C is undoing X.
    """
    recent = parse_patch(git("log", f"--since={WINDOW_DAYS}.days", f"-n{WINDOW_COMMITS}",
                             "--format=@@commit %h %s", "-p", "-U0", "--no-renames", base))
    pushed = parse_patch(git("log", "--reverse", "--format=@@commit %H %P", "-p", "-U0",
                             "--no-renames", f"{base}..{tip}"))
    blobs = Blobs()
    problems = []
    for header, removed, added, status in pushed:
        sha, *parents = header.split()
        body, allowed = trailers(sha)
        if body.startswith("Revert ") or "*" in allowed or len(parents) != 1:
            continue
        parent = parents[0]
        moved = set().union(*added.values()) if added else set()
        touched = {path for path, lines in removed.items() if lines and path not in allowed}
        for subject, _, footprint, _ in recent:
            if not touched & footprint.keys():
                continue
            surviving_total = undone_total = 0
            files = []
            for path, lines in footprint.items():
                surviving = lines & blobs.lines(parent, path)
                surviving_total += len(surviving)
                if path not in touched:
                    continue
                undone = (surviving & removed[path]) - moved
                if undone:
                    undone_total += len(undone)
                    files.append(("deletes " if status.get(path) == "D" else "") + path)
            if undone_total >= UNDO_MIN_LINES and undone_total >= UNDO_FRACTION * surviving_total:
                problems.append(
                    f"{sha[:9]} removes {undone_total} of the {surviving_total} lines main commit "
                    f"'{subject}' added ({', '.join(sorted(files))})"
                )
    blobs.close()
    # Protocol version may only move forward relative to main.
    base_text = "\n".join(blob_lines(f"{base}:{PROTOCOL_FILE}") or [])
    tip_text = "\n".join(blob_lines(f"{tip}:{PROTOCOL_FILE}") or [])
    base_match, tip_match = PROTOCOL_RE.search(base_text), PROTOCOL_RE.search(tip_text)
    if base_match and tip_match and int(tip_match.group(1)) < int(base_match.group(1)):
        problems.append(
            f"protocol VERSION goes backwards: main has {base_match.group(1)}, push has "
            f"{tip_match.group(1)}. Rebase and bump past main's value."
        )
    return problems


# ---------------------------------------------------------------- build/test


def process_dead(pid):
    """True only when the OS positively reports no such process. Query only."""
    if not pid.isdigit():
        return False
    if os.name == "nt":
        result = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH", "/FO", "CSV"],
                                capture_output=True, text=True, errors="replace")
        return result.returncode == 0 and "No tasks" in result.stdout
    try:
        os.kill(int(pid), 0)
    except ProcessLookupError:
        return True
    except OSError:
        pass
    return False


class Lock:
    """An exclusive file lock. The holder refreshes the file's mtime every
    30 s; a lock whose heartbeat stopped for LOCK_STALE_SECONDS, or whose
    holder the OS reports gone, is reclaimed. Only the owner removes it."""

    def __init__(self, path, label):
        self.path = path
        self.label = label
        self.token = f"{os.getpid()} {time.time():.0f} {label}"
        self.stop = threading.Event()

    def owned(self):
        try:
            return self.path.read_text(encoding="utf-8") == self.token
        except OSError:
            return False

    def heartbeat(self):
        while not self.stop.wait(30):
            if self.owned():
                os.utime(self.path)

    def __enter__(self):
        announced = 0.0
        while True:
            try:
                fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                os.write(fd, self.token.encode("utf-8"))
                os.close(fd)
                threading.Thread(target=self.heartbeat, daemon=True).start()
                return self
            except FileExistsError:
                pass
            try:
                holder = self.path.read_text(encoding="utf-8")
                idle = time.time() - self.path.stat().st_mtime
            except OSError:
                time.sleep(1)
                continue
            parts = holder.split(" ", 2)
            started = float(parts[1]) if len(parts) > 1 and parts[1].isdigit() else time.time()
            if idle > LOCK_STALE_SECONDS or (idle > 60 and process_dead(parts[0])):
                say(f"reclaiming stale gate lock ({holder}; no heartbeat for {idle:.0f}s)")
                try:
                    if self.path.read_text(encoding="utf-8") == holder:
                        remove(self.path)
                except OSError:
                    pass
                continue
            if time.time() - announced >= 300:
                say(f"waiting for the gate lock, held for {(time.time() - started) / 60:.0f} min "
                    f"by: {holder}")
                announced = time.time()
            time.sleep(5)

    def __exit__(self, *exc):
        self.stop.set()
        if self.owned():
            remove(self.path)


def remove(path):
    """Delete a file, retrying while a waiter briefly has it open (Windows)."""
    for _ in range(100):
        try:
            path.unlink(missing_ok=True)
            return
        except PermissionError:
            time.sleep(0.1)
    path.unlink(missing_ok=True)


def run_step(name, command, cwd, log, env=None):
    say(f"{name}: {' '.join(str(part) for part in command)}")
    started = time.time()
    with open(log, "a", encoding="utf-8", errors="replace") as handle:
        handle.write(f"\n===== {name} =====\n")
        handle.flush()
        result = subprocess.run(command, cwd=cwd, stdout=handle, stderr=subprocess.STDOUT, env=env)
    say(f"{name}: {'ok' if result.returncode == 0 else 'FAILED'} in {time.time() - started:.0f}s")
    return result.returncode == 0


def tail(log, marker, lines=60):
    text = Path(log).read_text(encoding="utf-8", errors="replace")
    section = text[text.rfind(marker):] if marker in text else text
    return "\n".join(section.splitlines()[-lines:])


def known_failures(worktree):
    path = worktree / "tools" / "gate-known-failures.toml"
    if not path.exists():
        return {}, []
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    failures = {entry["test"]: entry for entry in data.get("failure", [])}
    skips = [entry["test"] for entry in data.get("skip", []) + data.get("nightly", [])]
    return failures, skips


def port_bound_targets(worktree):
    """Test targets that host on fixed ports ([[port_bound]] entries)."""
    path = worktree / "tools" / "gate-known-failures.toml"
    if not path.exists():
        return set()
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return {entry["target"] for entry in data.get("port_bound", [])}


def parse_failures(log):
    text = Path(log).read_text(encoding="utf-8", errors="replace")
    section = text[text.rfind("===== test ====="):]
    target = "?"
    failed = set()
    for line in section.splitlines():
        match = re.match(r"\s*Running (?:unittests )?(\S+)", line)
        if match:
            target = match.group(1).replace("\\", "/")
            if target.startswith("src/"):
                deps = re.search(r"deps[/\\]([A-Za-z0-9_]+?)-[0-9a-f]+", line)
                target = f"{deps.group(1) if deps else '?'}:{target}"
        match = re.match(r"test (\S+) \.\.\. FAILED", line)
        if match:
            failed.add(f"{target}::{match.group(1)}")
    compile_error = "error: could not compile" in section or "error[E" in section
    return failed, compile_error


def matches(key, known):
    return next((name for name in known
                 if key == name or key.endswith("::" + name) or key.endswith("/" + name)), None)


def prepare_worktree(root, sha):
    worktree = root / "worktree"
    if not (worktree / ".git").exists():
        if worktree.exists():
            raise GateError(f"{worktree} exists but is not a git worktree; remove it by hand")
        git("worktree", "prune")
        git("worktree", "add", "--detach", str(worktree), sha)
    else:
        git("checkout", "--detach", "--force", sha, cwd=worktree)
        git("reset", "-q", "--hard", sha, cwd=worktree)
        # -x also drops ignored leftovers, keeping the content junction,
        # test report folders and any stray in-tree target/.
        git("clean", "-fdxq", "-e", "/content", "-e", "/artifacts", "-e", "/target",
            cwd=worktree)
    content = worktree / "content"
    if not content.exists():
        source = main_checkout() / "content"
        if os.name == "nt":
            subprocess.run(["cmd", "/c", "mklink", "/J", str(content), str(source)],
                           check=True, capture_output=True)
        else:
            content.symlink_to(source, target_is_directory=True)
    # Ignored tests write reports under artifacts/<name>/ and expect the
    # directory to exist, as it does in the main checkout.
    for directory in (main_checkout() / "artifacts").glob("*/"):
        (worktree / "artifacts" / directory.name).mkdir(parents=True, exist_ok=True)
    return worktree


def tree_intact(worktree, sha):
    """The gate worktree still holds exactly sha, with no tracked changes."""
    head = git("rev-parse", "HEAD", cwd=worktree).strip()
    dirty = git("status", "--porcelain", "--untracked-files=no", cwd=worktree).strip()
    if head != sha or dirty:
        say(f"the gate worktree changed during this run (HEAD {head[:9]}, "
            f"{'dirty' if dirty else 'clean'}); another process touched it. Rerun the gate.")
        return False
    return True


def trim_target(target):
    """Drop incremental state, and the whole target dir once stale builds pile up."""
    shutil.rmtree(target / "debug" / "incremental", ignore_errors=True)
    deps = target / "debug" / "deps"
    if not deps.is_dir():
        return
    size = sum(entry.stat().st_size for entry in os.scandir(deps) if entry.is_file())
    if size > TARGET_DEPS_CAP:
        say(f"{size / 2**30:.0f} GB of old builds in {deps}; starting from an empty target dir")
        shutil.rmtree(target, ignore_errors=True)


def full_gate(sha, root):
    root.mkdir(parents=True, exist_ok=True)
    passed = root / "passed" / sha
    if passed.exists():
        say(f"{sha[:9]} already passed the gate")
        return True
    (root / "logs").mkdir(exist_ok=True)
    log = root / "logs" / f"{sha[:12]}.log"
    log.write_text("", encoding="utf-8")
    label = f"{git('rev-parse', '--show-toplevel').strip()} {sha[:9]}"
    with contextlib.nullcontext() if LOCK_HELD else Lock(root / "gate.lock", label):
        if passed.exists():
            say(f"{sha[:9]} already passed the gate")
            return True
        worktree = prepare_worktree(root, sha)
        trim_target(root / "target")
        # Every run builds a new commit once, so incremental state only costs
        # disk writes, and it stops sccache caching the workspace crates.
        env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"), CARGO_TERM_COLOR="never",
                   CARGO_INCREMENTAL="0")
        started = time.time()
        steps = [
            ("build", ["cargo", "build", "--workspace", "--all-targets", "--locked"]),
            ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked",
                        "--", "-D", "warnings"]),
        ]
        for name, command in steps:
            if not tree_intact(worktree, sha):
                return False
            if not run_step(name, command, worktree, log, env):
                print(tail(log, f"===== {name} ====="))
                say(f"full log: {log}")
                return False
        state = root / "check-state"
        shutil.rmtree(state, ignore_errors=True)
        exe = root / "target" / "debug" / ("bri-client.exe" if os.name == "nt" else "bri-client")
        if not run_step("content-check", [exe, "--check", worktree / "content", state], worktree, log, env):
            print(tail(log, "===== content-check ====="))
            return False
        known, skips = known_failures(worktree)
        skip_args = [arg for name in skips for arg in ("--skip", name)]
        say(f"test: {TEST_JOBS} test binaries at a time, --include-ignored")
        test_started = time.time()
        with open(log, "a", encoding="utf-8") as handle:
            handle.write("\n===== test =====\n")
        binaries = test_binaries(worktree, env)
        if binaries is None:
            print(tail(log, "===== test ====="))
            say("a test target failed to compile")
            return False
        run_binaries(binaries, ["--include-ignored", *skip_args], log, TEST_JOBS,
                     port_bound_targets(worktree))
        say(f"test: ran {len(binaries)} binaries in {time.time() - test_started:.0f}s")
        if not tree_intact(worktree, sha):
            return False
        failed, compile_error = parse_failures(log)
        if compile_error:
            print(tail(log, "===== test ====="))
            say("a test target failed to compile")
            return False
        unexpected = sorted(key for key in failed if not matches(key, known))
        tolerated = sorted(key for key in failed if matches(key, known))
        fixed = sorted(name for name in known if not any(matches(key, {name: 1}) for key in failed))
        for key in tolerated:
            entry = known[matches(key, known)]
            say(f"known failure (owner: {entry.get('owner', '?')}): {key}")
        for name in fixed:
            say(f"known failure now passes; remove it from tools/gate-known-failures.toml: {name}")
        if "test result: FAILED" in Path(log).read_text(encoding="utf-8", errors="replace") and not failed:
            print(tail(log, "===== test ====="))
            say("tests failed but no failing test name could be parsed")
            return False
        # Offscreen app tests wait on wall-clock timeouts, which a machine busy
        # with other sessions' builds can miss. Retry each new failure alone once.
        for key in list(unexpected):
            name = key.split("::", 1)[1]
            retry = root / "logs" / f"{sha[:12]}-retry.log"
            retry.write_text("", encoding="utf-8")
            run_step(f"retry {name}", ["cargo", "test", "--workspace", "--locked", "--",
                                       "--include-ignored", "--exact", name], worktree, retry, env)
            if f"test {name} ... ok" in retry.read_text(encoding="utf-8", errors="replace"):
                say(f"flaky: {key} failed, then passed alone")
                unexpected.remove(key)
        if unexpected:
            say("new test failures:")
            for key in unexpected:
                print(f"    {key}")
            say(f"full log: {log}")
            return False
        if not tree_intact(worktree, sha):
            return False
        passed.parent.mkdir(exist_ok=True)
        passed.write_text(time.strftime("%Y-%m-%d %H:%M:%S"), encoding="utf-8")
        say(f"PASSED {sha[:9]} in {time.time() - started:.0f}s")
        return True


# ---------------------------------------------------------------- entry


def gate_commit(sha, diff_only):
    say("fetching origin/main")
    git("fetch", "-q", "origin", "main")
    base = git("rev-parse", "refs/remotes/origin/main").strip()
    if sha == base:
        say("nothing new to push")
        return True
    if subprocess.run(["git", "merge-base", "--is-ancestor", base, sha]).returncode != 0:
        say(f"{sha[:9]} does not contain the latest origin/main ({base[:9]}).")
        say("Rebase onto origin/main first (git fetch origin main && git rebase origin/main), "
            "then push again.")
        return False
    count = len(git("rev-list", f"{base}..{sha}").split())
    say(f"checking {count} commit(s) {base[:9]}..{sha[:9]}")
    problems = history_check(base, sha)
    if problems:
        say("history check FAILED; these commits look built from a stale tree:")
        for problem in problems:
            print(f"    {problem}")
        say("Rebuild the commit on origin/main. If the undo is intentional, add "
            "'Gate-Allow-Undo: <path>' to that commit's message.")
        return False
    say("history check ok")
    heavy = oversized_blobs(base, sha)
    if heavy:
        say("refusing: the pushed commits carry build output or very large files:")
        for line in heavy[:20]:
            print(f"    {line}")
        say("Remove them from the commits themselves (not just a later commit); "
            "history keeps every blob it was given.")
        return False
    if diff_only:
        return True
    changed = git("diff", "--name-only", base, sha).split()
    if changed and all(path.endswith(DOC_SUFFIXES) for path in changed):
        say(f"only documentation changed ({len(changed)} files); skipping build and tests")
        return True
    return full_gate(sha, gate_root())


def push_main():
    """Rebase HEAD onto origin/main, gate it and push it, all under the gate
    lock, so no other gated push can move main between the gate and the push."""
    global LOCK_HELD
    root = gate_root()
    root.mkdir(parents=True, exist_ok=True)
    label = f"{git('rev-parse', '--show-toplevel').strip()} --push"
    with Lock(root / "gate.lock", label):
        LOCK_HELD = True
        for _attempt in range(3):
            git("fetch", "-q", "origin", "main")
            if subprocess.run(["git", "merge-base", "--is-ancestor", "refs/remotes/origin/main",
                                "HEAD"]).returncode != 0:
                if git("status", "--porcelain", "--untracked-files=no").strip():
                    raise GateError("commit or discard your changes first; --push rebases HEAD")
                say("rebasing onto origin/main")
                if subprocess.run(["git", "rebase", "-q", "refs/remotes/origin/main"]).returncode:
                    subprocess.run(["git", "rebase", "--abort"])
                    raise GateError("rebase onto origin/main conflicts; resolve it by hand")
            sha = git("rev-parse", "HEAD").strip()
            if not gate_commit(sha, diff_only=False):
                return False
            # Push the commit that passed, not whatever HEAD became meanwhile.
            if subprocess.run(["git", "push", "origin", f"{sha}:refs/heads/main"]).returncode == 0:
                say(f"pushed {sha[:9]} to main")
                return True
            say("main moved during the gate (an ungated push?); rebasing and retrying")
    return False


def test_binaries(top, env=None):
    """Build every test target and list (label, executable, cwd, header).

    label is "<package>/<target>" ("lib" for unit tests); header mimics the
    "Running ..." line cargo test prints, which parse_failures reads."""
    build = subprocess.run(["cargo", "test", "--workspace", "--locked", "--no-run",
                            "--message-format=json-render-diagnostics"],
                           cwd=top, stdout=subprocess.PIPE, text=True, errors="replace", env=env)
    if build.returncode:
        return None
    binaries = []
    for line in build.stdout.splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        if not message["profile"].get("test"):
            continue
        package_id = message["package_id"]
        package = (package_id.rsplit("#", 1)[1].split("@")[0] if "#" in package_id
                   else package_id.split()[0])
        target = message["target"]
        cwd = Path(message["manifest_path"]).parent
        try:
            source = Path(target["src_path"]).relative_to(cwd).as_posix()
        except ValueError:
            source = target["src_path"]
        unit = "lib" in target["kind"] or "bin" in target["kind"]
        header = f"     Running {'unittests ' if unit else ''}{source} ({message['executable']})"
        label = f"{package}/{'lib' if 'lib' in target['kind'] else target['name']}"
        binaries.append((label, message["executable"], cwd, header))
    return binaries


def stop_tree(process):
    """End a test binary the gate started, with every process it started."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(process.pid)],
                       capture_output=True)
    else:
        process.kill()


def run_binaries(binaries, args, log, jobs, exclusive=()):
    """Run test binaries in parallel, appending each one's output to log in
    listing order. Returns True when every binary passed.

    Whole-app and GPU test binaries wait on wall-clock timeouts, so at most
    HEAVY_JOBS of them run at once, in their own pool; the rest share the
    remaining jobs. Targets in `exclusive` bind fixed ports (a hosted game's
    UDP 28000/28050), so they run one at a time in a pool of their own."""
    def one(entry):
        label, executable, cwd, header = entry
        started = time.time()
        process = subprocess.Popen([executable, *args], cwd=cwd, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, errors="replace")
        try:
            output, _ = process.communicate(timeout=BINARY_TIMEOUT)
            code = process.returncode
        except subprocess.TimeoutExpired:
            # A hung test (a stuck child process) must fail the run, not hold
            # the gate lock forever. End this test binary and its children.
            stop_tree(process)
            try:
                output, _ = process.communicate(timeout=30)
            except subprocess.TimeoutExpired:
                output = ""
            output += (f"\n[gate] {label} ran past {BINARY_TIMEOUT}s and was stopped\n"
                       f"test {label}::gate_timeout ... FAILED\n")
            code = 1
        return header, output, code, time.time() - started, label

    def is_heavy(label):
        return label.startswith(HEAVY_PREFIXES) and not label.endswith("/lib")

    def pool_for(label):
        if label in exclusive:
            return alone
        return heavy if is_heavy(label) else light

    with concurrent.futures.ThreadPoolExecutor(1) as alone, \
            concurrent.futures.ThreadPoolExecutor(HEAVY_JOBS) as heavy, \
            concurrent.futures.ThreadPoolExecutor(max(1, jobs - HEAVY_JOBS)) as light:
        futures = [pool_for(b[0]).submit(one, b) for b in binaries]
        results = [future.result() for future in futures]
    ok = True
    with open(log, "a", encoding="utf-8", errors="replace") as handle:
        for header, output, code, _, _ in results:
            handle.write(f"{header}\n{output}\n")
            ok = ok and code == 0
    slowest = sorted(((secs, label) for _, _, _, secs, label in results), reverse=True)
    say("slowest test binaries: " + ", ".join(f"{label} {secs:.0f}s" for secs, label in slowest[:3]))
    return ok


def ci_test():
    """Run every test binary except targets that need generated v20 content.

    GitHub runners have no v20 content. Those targets are listed as
    [[ci_skip_target]] in tools/gate-known-failures.toml; the local gate still
    runs them all.
    """
    top = Path(git("rev-parse", "--show-toplevel").strip())
    data = tomllib.loads((top / "tools" / "gate-known-failures.toml").read_text(encoding="utf-8"))
    skipped = {entry["target"] for entry in data.get("ci_skip_target", [])}
    binaries = test_binaries(top)
    if binaries is None:
        return False
    unknown = skipped - {label for label, _, _, _ in binaries}
    if unknown:
        say(f"ci_skip_target entries match no test target: {', '.join(sorted(unknown))}")
        return False
    failed = []
    for label, executable, cwd, _ in sorted(binaries):
        if label in skipped:
            say(f"skipping {label} (needs generated content)")
            continue
        say(f"running {label}")
        if subprocess.run([executable], cwd=cwd).returncode:
            failed.append(label)
    if failed:
        say(f"failing test targets: {', '.join(failed)}")
        return False
    say("all content-free test targets passed")
    return True


def hook(stdin):
    ok = True
    for line in stdin.read().splitlines():
        parts = line.split()
        if len(parts) != 4:
            continue
        _local_ref, local_sha, remote_ref, _remote_sha = parts
        if remote_ref != MAIN_REF:
            continue
        if local_sha == ZERO:
            say("refusing to delete main")
            ok = False
            continue
        ok = gate_commit(local_sha, diff_only=False) and ok
    return ok


HOOK = """#!/bin/sh
# Blockland ReImagined push gate. Installed by: python tools/gate.py --install-hook
# Runs tools/gate.py from the latest origin/main for pushes to main.
# Never bypass it with --no-verify.
common=$(git rev-parse --git-common-dir)
git fetch -q origin main 2>/dev/null
script="$common/bri-gate-current.py"
if ! git show refs/remotes/origin/main:tools/gate.py > "$script" 2>/dev/null; then
    top=$(git rev-parse --show-toplevel)
    [ -f "$top/tools/gate.py" ] || exit 0
    cp "$top/tools/gate.py" "$script"
fi
for py in python python3 py; do
    if command -v $py >/dev/null 2>&1; then exec $py "$script" --hook "$@"; fi
done
echo "[gate] no python found; cannot run the push gate" >&2
exit 1
"""


def install_hook():
    hooks = common_dir() / "hooks"
    hooks.mkdir(exist_ok=True)
    target = hooks / "pre-push"
    if target.exists() and "Blockland ReImagined push gate" not in target.read_text(errors="replace"):
        raise GateError(f"{target} exists and is not the gate hook; merge it by hand")
    target.write_text(HOOK, encoding="utf-8", newline="\n")
    target.chmod(0o755)
    say(f"installed {target} (shared by every worktree)")


def forget_hook_repository():
    """Git runs hooks with GIT_DIR and friends pointing at the pushing
    worktree. Left in the environment, they redirect every git call, even
    one with cwd set to the gate worktree, back to the pusher: a checkout
    for the gate would move the pusher's HEAD while cargo built a stale
    gate tree. Hooks start in the pusher's top level, so plain discovery
    from the working directory finds the same repository."""
    for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_PREFIX",
                 "GIT_COMMON_DIR", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                 "GIT_QUARANTINE_PATH"):
        os.environ.pop(name, None)


def main():
    forget_hook_repository()
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--hook", nargs="*", help=argparse.SUPPRESS)
    parser.add_argument("--install-hook", action="store_true")
    parser.add_argument("--diff-only", action="store_true")
    parser.add_argument("--ci-test", action="store_true",
                        help="run the tests that need no generated content (used by CI)")
    parser.add_argument("--push", action="store_true",
                        help="rebase onto origin/main, gate and push to main under the lock")
    parser.add_argument("--history-range", nargs=2, metavar=("BASE", "TIP"),
                        help="only run the history check on BASE..TIP (used by CI)")
    parser.add_argument("commit", nargs="?", default="HEAD")
    args = parser.parse_args()
    try:
        if args.install_hook:
            install_hook()
            return 0
        if args.history_range:
            base, tip = args.history_range
            if subprocess.run(["git", "cat-file", "-e", f"{base}^{{commit}}"],
                              capture_output=True).returncode:
                # A force push replaced the previous tip; judge against main.
                base = git("merge-base", "refs/remotes/origin/main", tip).strip()
                say(f"previous tip is gone; checking from merge base {base[:9]}")
            problems = history_check(base, tip)
            for problem in problems:
                print(f"    {problem}")
            say("history check " + ("FAILED" if problems else "ok"))
            return 1 if problems else 0
        if args.ci_test:
            return 0 if ci_test() else 1
        if args.push:
            return 0 if push_main() else 1
        if args.hook is not None:
            return 0 if hook(sys.stdin) else 1
        sha = git("rev-parse", args.commit).strip()
        return 0 if gate_commit(sha, args.diff_only) else 1
    except GateError as error:
        say(str(error))
        return 1


if __name__ == "__main__":
    sys.exit(main())
