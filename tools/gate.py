"""Push gate for main: nothing lands on origin/main unless it passes.

    python tools/gate.py                 gate HEAD as if pushing it to main
    python tools/gate.py --push          rebase, gate and push HEAD to main (preferred)
    python tools/gate.py --diff-only     only the fast history checks
    python tools/gate.py --install-hook  install the shared pre-push hook
    python tools/gate.py --hook ...      (called by the pre-push hook)
    python tools/gate.py --history-range BASE TIP   (history check only; CI)
    python tools/gate.py --ci-test       content-free tests only (CI)
    python tools/gate.py --ci-test --shard K/N   the Kth of N even shares of them

Checks, cheapest first, on the exact commit being pushed:
  1. the commit already contains the latest origin/main (rebase first)
  2. history sanity: no pushed commit deletes or undoes recent main work,
     and a protocol VERSION change only ever increases it (no protocol change
     file is removed)
  3. cargo build --workspace --all-targets --locked
  4. cargo clippy --workspace --all-targets --locked -- -D warnings
  5. bri-client --check against the main checkout's content; it also
     refreshes that content's installed default Add-Ons (content/addons)
     from the build, so a stale copy can't fail a test
  6. cargo test --workspace --locked -- --include-ignored, with failures listed
     in tools/gate-known-failures.toml tolerated
  7. the fixed save corpus (crates/client/tests/save-corpus.json), only when
     the change touches a path in SAVE_CORPUS_PATHS

Builds run in one shared gate worktree and target dir next to the main
checkout (default ../.bri-gate), serialized by a lock, so parallel sessions
queue instead of doing 25 cold builds at once.

Allowing an intentional undo: add a trailer line to the commit message,
    Gate-Allow-Undo: path/to/file      (repeatable; "*" allows every path)
Commits made by `git revert` ("Revert ...") are allowed automatically, and so
is reverting such a revert ("Reapply ..."): both undo a commit on purpose.
"""
import argparse
import concurrent.futures
import contextlib
import hashlib
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
PROTOCOL_CHANGES_DIR = "crates/net/protocol-changes"
LOCK_STALE_SECONDS = 10 * 60
LOCK_HELD = False
# Test processes at once: every logical CPU unless --jobs says otherwise.
TEST_JOBS = os.cpu_count() or 8
# Each test binary's last wall time, kept beside the gate's target dir, so
# the next run starts the slowest first (`plan_units`).
TIMINGS_FILE = "gate-timings.json"
# Test time over which the gate prints a warning.
TEST_WARN_SECONDS = 10 * 60
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
# Changes under these paths (prefixes) also host the fixed corpus of tricky
# .bls saves: saving, loading, the .bls converter, brick and print data.
SAVE_CORPUS_PATHS = (
    "crates/bls/",
    "crates/convert/",
    "crates/world/src/build.rs",
    "crates/world/src/model.rs",
    "crates/world/src/packed.rs",
    "crates/world/src/persistence.rs",
    "crates/content/src/brick.rs",
    "crates/content/src/brick_materials.rs",
    "crates/sim/src/session/build_load.rs",
    "crates/sim/src/definitions.rs",
    "crates/client/src/saves.rs",
    "crates/client/src/old_saves.rs",
    "crates/client/src/save_host.rs",
    "crates/client/src/materials.rs",
    "crates/client/src/world_chunks.rs",
    "crates/client/src/world_scene.rs",
    "crates/client/src/bin/saves_host_probe.rs",
    "crates/client/tests/save_corpus.rs",
    "crates/client/tests/save-corpus.json",
)
SAVE_CORPUS_TARGET = "bri-client/save_corpus"
# Every passed tree is recorded under this folder of the gate root, keyed by
# the other inputs a result depends on (`gate_inputs`). A commit that only
# rewords its message, or a rebase that lands on a tree already gated, reuses
# the pass instead of building and testing it again.
PASSED_TREES = "passed-trees"
# How many of the newest recorded passes a tree that differs only in
# documentation is compared against (`reusable_pass`).
REUSE_CANDIDATES = 50
# Top-level content folders the fingerprint leaves out: the content check
# rewrites addons/ from the build itself, and .downloads/ is a cache.
CONTENT_DERIVED = {"addons", ".downloads", "_regeneration"}
SAVE_CORPUS_TEST = "the_fixed_save_corpus_hosts_like_the_game"


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


def protocol_change_files(rev):
    """Names of the protocol change files at `rev` (README.md excluded)."""
    result = subprocess.run(["git", "ls-tree", "--name-only", f"{rev}:{PROTOCOL_CHANGES_DIR}"],
                            capture_output=True, text=True, errors="replace")
    if result.returncode != 0:
        return set()
    return {n for n in result.stdout.splitlines() if n.endswith(".md") and n != "README.md"}


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


def undoes_on_purpose(body):
    """Whether a commit message says the commit undoes another on purpose:
    `git revert` ("Revert ...") or a revert of a revert ("Reapply ...")."""
    return body.startswith(("Revert ", "Reapply "))


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
        if undoes_on_purpose(body) or "*" in allowed or len(parents) != 1:
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
    # Protocol version may only move forward relative to main. The version
    # counts the files in PROTOCOL_CHANGES_DIR, so none may disappear.
    gone = sorted(protocol_change_files(base) - protocol_change_files(tip))
    if gone:
        problems.append(
            f"protocol change files removed ({', '.join(gone)}): the protocol version would go "
            f"down. Keep every file in {PROTOCOL_CHANGES_DIR}/; add a new one instead."
        )
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


def header_target(line):
    """The target part of a failure key, from a "Running ..." line, or None."""
    match = re.match(r"\s*Running (?:unittests )?(\S+)", line)
    if not match:
        return None
    target = match.group(1).replace("\\", "/")
    if target.startswith("src/"):
        deps = re.search(r"deps[/\\]([A-Za-z0-9_]+?)-[0-9a-f]+", line)
        target = f"{deps.group(1) if deps else '?'}:{target}"
    return target


def parse_failures(log):
    text = Path(log).read_text(encoding="utf-8", errors="replace")
    section = text[text.rfind("===== test ====="):]
    target = "?"
    failed = set()
    for line in section.splitlines():
        target = header_target(line) or target
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


def gate_env(environ, content):
    """The environment every gate step runs in.

    Content-backed tests find generated v20 content through BRI_CONTENT.
    Without it they panic asking for it, so the gate points it at the main
    checkout's real folder (not the worktree's junction, which the game
    refuses to load packages through) unless the caller already set one.
    """
    env = dict(environ, CARGO_TERM_COLOR="never", CARGO_INCREMENTAL="0")
    env.pop("CARGO_TARGET_DIR", None)
    env.setdefault("BRI_CONTENT", str(content))
    return env


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


def documentation_only(paths):
    """Paths no build or test reads: Markdown outside crates/ (crates/net's
    build script counts its protocol-changes/*.md). List them with
    --no-renames, so a file moved into a .md also shows where it came from."""
    return all(path.endswith(DOC_SUFFIXES) and not path.startswith("crates/") for path in paths)


def toolchain_id(sha):
    """rustc and cargo as the tree at sha pins them (rust-toolchain.toml), or
    None when they cannot be asked."""
    env = dict(os.environ)
    try:
        pinned = tomllib.loads(git("show", f"{sha}:rust-toolchain.toml"))
        env["RUSTUP_TOOLCHAIN"] = pinned["toolchain"]["channel"]
    except (GateError, tomllib.TOMLDecodeError, KeyError):
        pass
    parts = []
    for command in (["rustc", "-vV"], ["cargo", "-V"]):
        try:
            result = subprocess.run(command, capture_output=True, text=True, errors="replace",
                                    env=env, cwd=main_checkout())
        except OSError:
            return None
        if result.returncode:
            return None
        parts.append(result.stdout.strip())
    return "\n".join(parts)


def content_fingerprint(content):
    """Names, sizes and modification times of the content packs and of what
    sits directly in each, plus every regeneration stamp. A regenerated or
    copied-in pack changes it; the installed Add-Ons, which the content check
    rewrites from each build, do not. Each entry is stat'ed by its path: on
    Windows a scandir entry's times are the copy in the parent folder's
    listing, which NTFS updates lazily, so two reads of an unchanged folder
    could differ."""
    def line(entry, indent=""):
        stat = os.stat(entry.path)
        size = stat.st_size if entry.is_file() else "-"
        return f"{indent}{entry.name} {size} {stat.st_mtime_ns}"

    lines = []
    try:
        for entry in sorted(os.scandir(content), key=lambda entry: entry.name):
            if entry.name in CONTENT_DERIVED:
                continue
            lines.append(line(entry))
            if entry.is_dir():
                children = sorted(os.scandir(entry.path), key=lambda child: child.name)
                lines.extend(line(child, "  ") for child in children)
        stamps = Path(content) / "_regeneration" / "stamps"
        for stamp in sorted(stamps.glob("*.json")):
            lines.append(f"{stamp.name} {stamp.read_text(encoding='utf-8', errors='replace')}")
    except OSError:
        return None
    return "\n".join(lines)


def gate_inputs(sha, content):
    """A digest of what a gate result depends on besides the tree: the
    platform, the pinned toolchain and the content the tests load. None when
    one of them cannot be read, which never matches a recorded pass."""
    toolchain = toolchain_id(sha)
    fingerprint = content_fingerprint(content)
    if toolchain is None or fingerprint is None:
        return None
    text = "\n".join([sys.platform, os.environ.get("BRI_CONTENT", ""), toolchain, fingerprint])
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


def tree_of(sha):
    return git("rev-parse", f"{sha}^{{tree}}").strip()


def record_pass(root, sha, inputs, corpus):
    """Remember that sha's tree passed with these inputs (`gate_inputs`);
    `corpus` says whether the fixed save corpus ran too."""
    if inputs is None:
        return
    folder = root / PASSED_TREES
    folder.mkdir(exist_ok=True)
    record = {"commit": sha, "corpus": corpus, "at": time.strftime("%Y-%m-%d %H:%M:%S")}
    (folder / f"{inputs}-{tree_of(sha)}.json").write_text(json.dumps(record), encoding="utf-8")


def reusable_pass(root, sha, inputs, needs_corpus):
    """(commit, same tree) of an earlier pass that covers sha, or None.

    It covers sha when it ran with the same inputs on the same tree, or on a
    tree that differs only in documentation (`documentation_only`), and also
    hosted the save corpus whenever this change needs it."""
    if inputs is None:
        return None
    folder = root / PASSED_TREES
    tree = tree_of(sha)
    try:
        candidates = sorted(folder.glob(f"{inputs}-*.json"),
                            key=lambda path: path.stat().st_mtime, reverse=True)
    except OSError:
        return None
    exact = folder / f"{inputs}-{tree}.json"
    if exact in candidates:
        candidates.remove(exact)
        candidates.insert(0, exact)
    for path in candidates[:REUSE_CANDIDATES]:
        try:
            record = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if needs_corpus and not record.get("corpus"):
            continue
        other = path.stem.split("-", 1)[1]
        if other != tree:
            result = subprocess.run(["git", "diff", "--name-only", "--no-renames", other, tree],
                                    capture_output=True, text=True, errors="replace")
            if result.returncode or not documentation_only(result.stdout.split()):
                continue
        return record.get("commit", "?"), other == tree
    return None


def touches_saves(changed):
    return any(path.startswith(SAVE_CORPUS_PATHS) for path in changed)


def save_corpus_result(output, code):
    """Keep libtest success separate from actual corpus coverage."""
    if code != 0 or f"test {SAVE_CORPUS_TEST} ... ok" not in output:
        return False, "FAILED"
    skipped = next((line.strip() for line in output.splitlines()
                    if line.strip().startswith("skipped:")), None)
    if skipped:
        return True, "SKIPPED: " + skipped.removeprefix("skipped:").strip()
    coverage = re.search(r"^save corpus coverage: (\d+)/(\d+)$", output, re.M)
    if coverage:
        checked, total = map(int, coverage.groups())
        if not 0 < checked <= total:
            return False, "FAILED (invalid corpus coverage)"
        kind = "coverage" if checked == total else "partial coverage"
        return True, f"ok ({kind}: {checked}/{total} saves)"
    return True, "ok (coverage not reported)"


def save_corpus(binaries, log, env=None):
    """Host the fixed save corpus; True when it passed or found no saves."""
    owners = [b for b in binaries if b[0] == SAVE_CORPUS_TARGET]
    if not owners:
        say(f"save corpus: no {SAVE_CORPUS_TARGET} test binary")
        return False
    say("save corpus: the change touches saving, loading or brick data")
    started = time.time()
    label, executable, cwd, header = owners[0]
    output, code = run_binary(label, executable,
                              ["--include-ignored", "--exact", SAVE_CORPUS_TEST, "--nocapture"],
                              cwd, env)
    with open(log, "a", encoding="utf-8", errors="replace") as handle:
        handle.write(f"\n===== save corpus =====\n{header}\n{output}\n")
    ok, summary = save_corpus_result(output, code)
    say(f"save corpus: {summary} in {time.time() - started:.0f}s")
    if not ok:
        print(tail(log, "===== save corpus ====="))
    return ok


def already_passed(root, sha, inputs, needs_corpus):
    if (root / "passed" / sha).exists():
        say(f"{sha[:9]} already passed the gate")
        return True
    earlier = reusable_pass(root, sha, inputs, needs_corpus)
    if earlier:
        commit, same_tree = earlier
        say(f"{sha[:9]} has {'the same tree as' if same_tree else 'only documentation changes over'}"
            f" {commit[:9]}, which passed the gate with the same toolchain and content")
        return True
    return False


def full_gate(sha, root, changed=()):
    root.mkdir(parents=True, exist_ok=True)
    passed = root / "passed" / sha
    content = main_checkout() / "content"
    needs_corpus = touches_saves(changed)
    inputs = gate_inputs(sha, content)
    if already_passed(root, sha, inputs, needs_corpus):
        return True
    (root / "logs").mkdir(exist_ok=True)
    log = root / "logs" / f"{sha[:12]}.log"
    log.write_text("", encoding="utf-8")
    label = f"{git('rev-parse', '--show-toplevel').strip()} {sha[:9]}"
    with contextlib.nullcontext() if LOCK_HELD else Lock(root / "gate.lock", label):
        # Another run may have passed this tree while this one waited.
        if already_passed(root, sha, inputs, needs_corpus):
            return True
        worktree = prepare_worktree(root, sha)
        trim_target(root / "target")
        # Every run builds a new commit once, so incremental state only costs
        # disk writes, and it stops sccache caching the workspace crates.
        # The target dir goes on the command line, never in CARGO_TARGET_DIR:
        # sccache hashes every CARGO_* variable, so that variable alone made
        # every gate compile miss the cache the lanes fill.
        env = gate_env(os.environ, content)
        target = ["--target-dir", str(root / "target")]
        started = time.time()
        phases = {}
        steps = [
            ("tool-tests", [sys.executable, "-m", "unittest", "discover", "-s", str(worktree / "tools"), "-p", "test_*.py"]),
            ("build", ["cargo", "build", "--workspace", "--all-targets", "--locked", *target]),
        ]
        for name, command in steps:
            if not tree_intact(worktree, sha):
                return False
            step_started = time.time()
            if not run_step(name, command, worktree, log, env):
                print(tail(log, f"===== {name} ====="))
                say(f"full log: {log}")
                return False
            phases[name] = time.time() - step_started
        state = root / "check-state"
        shutil.rmtree(state, ignore_errors=True)
        exe = root / "target" / "debug" / ("bri-client.exe" if os.name == "nt" else "bri-client")
        # The check also refreshes the default Add-Ons' installed copies with
        # the binary just built, so a stale copy of an older build's data
        # never fails a content test.
        check_env = dict(env, BRI_INSTALL_DEFAULT_ADD_ONS="1")
        check_started = time.time()
        if not run_step("content-check", [exe, "--check", worktree / "content", state], worktree, log,
                        check_env):
            print(tail(log, "===== content-check ====="))
            return False
        phases["content-check"] = time.time() - check_started
        known, skips = known_failures(worktree)
        skip_args = [arg for name in skips for arg in ("--skip", name)]
        test_started = time.time()
        with open(log, "a", encoding="utf-8") as handle:
            handle.write("\n===== test =====\n")
        binaries = test_binaries(worktree, env, target)
        if binaries is None:
            print(tail(log, "===== test ====="))
            say("a test target failed to compile")
            return False
        # Clippy needs no test process and test processes need no cargo lock,
        # so it runs alongside the test pass, into a log of its own.
        clippy_log = root / "logs" / f"{sha[:12]}-clippy.log"
        clippy_log.write_text("", encoding="utf-8")
        clippy = {}

        def run_clippy():
            started_clippy = time.time()
            clippy["ok"] = run_step("clippy", ["cargo", "clippy", "--workspace", "--all-targets",
                                               "--locked", *target, "--", "-D", "warnings"],
                                    worktree, clippy_log, env)
            clippy["seconds"] = time.time() - started_clippy

        clippy_thread = threading.Thread(target=run_clippy)
        clippy_thread.start()
        # Beside the gate's target dir, not in it: target/ gets wiped.
        timings = root / TIMINGS_FILE
        units = plan_units(binaries, ["--include-ignored", *skip_args], load_timings(timings))
        say(f"test: {len(binaries)} test binaries, {TEST_JOBS} at a time, slowest first, "
            "--include-ignored")
        seconds = run_binaries(units, log, TEST_JOBS, port_bound_targets(worktree), env)
        save_timings(timings, seconds)
        phases["tests"] = time.time() - test_started
        say(f"test: ran {len(binaries)} binaries in {phases['tests']:.0f}s")
        clippy_thread.join()
        phases["clippy"] = clippy.get("seconds", 0.0)
        with open(log, "a", encoding="utf-8", errors="replace") as handle:
            handle.write(Path(clippy_log).read_text(encoding="utf-8", errors="replace"))
        if not clippy.get("ok"):
            print(tail(clippy_log, "===== clippy ====="))
            say(f"full log: {log}")
            return False
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
        retries_started = time.time()
        for key in list(unexpected):
            name = key.split("::", 1)[1]
            if whole_process_failure(name):
                # Not a test name: the whole process hung past BINARY_TIMEOUT
                # or crashed. A name-filtered rerun would match nothing.
                say(f"not retried: {name}")
                continue
            retry = root / "logs" / f"{sha[:12]}-retry.log"
            retry.write_text("", encoding="utf-8")
            # Rerun only the binary the failure came from (every binary with
            # that target name), not the whole workspace behind a name filter.
            owners = [b for b in binaries if header_target(b[3]) == key.split("::", 1)[0]]
            say(f"retry {name}: in {', '.join(label for label, _, _, _ in owners) or 'no binary'}")
            started_retry = time.time()
            with open(retry, "a", encoding="utf-8", errors="replace") as handle:
                for label, executable, cwd, header in owners:
                    output, _ = run_binary(label, executable,
                                           ["--include-ignored", "--exact", name], cwd, env)
                    handle.write(f"{header}\n{output}\n")
            say(f"retry {name}: done in {time.time() - started_retry:.0f}s")
            text = retry.read_text(encoding="utf-8", errors="replace")
            if f"test {name} ... ok" in text:
                say(f"flaky: {key} failed, then passed alone")
                unexpected.remove(key)
            elif f"test {name} ..." not in text:
                say(f"retry of {key} ran no test named {name}; it still counts as failed")
        phases["retries"] = time.time() - retries_started
        summary(phases, time.time() - started, log)
        if unexpected:
            say("new test failures:")
            for key in unexpected:
                print(f"    {key}")
            say(f"full log: {log}")
            return False
        if needs_corpus and not save_corpus(binaries, log, env):
            say(f"full log: {log}")
            return False
        if not tree_intact(worktree, sha):
            return False
        passed.parent.mkdir(exist_ok=True)
        passed.write_text(time.strftime("%Y-%m-%d %H:%M:%S"), encoding="utf-8")
        # Content regenerated during the run leaves the tree unrecorded.
        if gate_inputs(sha, content) == inputs:
            record_pass(root, sha, inputs, needs_corpus)
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
    # --no-renames lists both sides of a move: code renamed into a .md file
    # is a deleted source file, not documentation.
    changed = git("diff", "--name-only", "--no-renames", base, sha).split()
    if changed and documentation_only(changed):
        say(f"only documentation changed ({len(changed)} files); skipping build and tests")
        return True
    return full_gate(sha, gate_root(), changed)


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


def test_binaries(top, env=None, extra=()):
    """Build every test target and list (label, executable, cwd, header).

    label is "<package>/<target>" ("lib" for unit tests); header mimics the
    "Running ..." line cargo test prints, which parse_failures reads."""
    build = subprocess.run(["cargo", "test", "--workspace", "--locked", "--no-run", *extra,
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


def run_binary(label, executable, args, cwd, env=None):
    """Run one test binary to completion; returns (output, exit code).

    `env` is the gate's step environment (`gate_env`); without it the binary
    inherits this process's, which lacks BRI_CONTENT."""
    process = subprocess.Popen([executable, *args], cwd=cwd, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, errors="replace",
                               env=env)
    try:
        output, _ = process.communicate(timeout=BINARY_TIMEOUT)
        return output + exit_failure(label, output, process.returncode), process.returncode
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
        return output, 1


def exit_failure(label, output, code):
    """A failure line for a test process that exited non-zero without naming
    a failed test (a stack overflow, a driver crash, an out-of-memory kill),
    so the crash fails the gate; "" otherwise."""
    if code == 0 or re.search(r"^test \S+ \.\.\. FAILED", output, re.MULTILINE):
        return ""
    return (f"\n[gate] {label} exited with code {code} and no failed test\n"
            f"test {label}::process_exit ... FAILED\n")


def whole_process_failure(name):
    """A failure key's name part that stands for a whole test process (it
    hung or crashed), not a test a name-filtered rerun could match."""
    return name.endswith(("::gate_timeout", "::process_exit"))


def load_timings(path):
    """Each test binary's last wall time in seconds, by label; {} when none."""
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return {label: float(seconds) for label, seconds in data.items()
            if isinstance(seconds, (int, float))}


def save_timings(path, seconds):
    """Keep this run's times, and earlier ones for binaries it didn't run."""
    merged = dict(load_timings(path), **seconds)
    try:
        Path(path).write_text(json.dumps(merged, indent=1, sort_keys=True), encoding="utf-8")
    except OSError:
        pass


def plan_units(binaries, args, timings):
    """The test processes to run, one per binary: (label, executable, cwd,
    header, args, expected seconds), slowest first, with a binary never
    measured first so it gets measured."""
    units = [(label, executable, cwd, header, list(args),
              timings.get(label, float("inf")))
             for label, executable, cwd, header in binaries]
    order = {entry[0]: i for i, entry in enumerate(binaries)}
    return sorted(units, key=lambda u: (-u[5], order[u[0]]))


def summary(phases, total, log=None):
    """One line of where the gate's time went, also kept in the log, and a
    warning past the limit."""
    parts = [f"{name} {phases[name]:.0f}s" for name in
             ("tool-tests", "build", "content-check", "clippy", "tests", "retries")
             if name in phases]
    line = (f"phases: {', '.join(parts)}, total {total:.0f}s, {TEST_JOBS} test jobs "
            "(clippy ran alongside tests)")
    say(line)
    if log:
        with open(log, "a", encoding="utf-8", errors="replace") as handle:
            handle.write(f"[gate] {line}\n")
    if phases.get("tests", 0) > TEST_WARN_SECONDS:
        say(f"warning: tests took {phases['tests'] / 60:.1f} minutes, over "
            f"{TEST_WARN_SECONDS // 60}")


def run_binaries(units, log, jobs, exclusive=(), env=None):
    """Run test processes (`plan_units`) in parallel, in the order given,
    appending each one's output to log in listing order. Returns each binary's
    wall time (its processes added up), by label.

    Whole-app and GPU test binaries wait on wall-clock timeouts, so at most
    HEAVY_JOBS of them run at once, in their own pool; the rest share the
    remaining jobs. Targets in `exclusive` bind fixed ports (a hosted game's
    UDP 28000/28050), so they run one at a time in a pool of their own."""
    def one(entry):
        label, executable, cwd, header, args, _ = entry
        started = time.time()
        output, code = run_binary(label, executable, args, cwd, env)
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
        futures = [pool_for(u[0]).submit(one, u) for u in units]
        results = [future.result() for future in futures]
    seconds = {}
    for _, _, _, secs, label in results:
        seconds[label] = seconds.get(label, 0.0) + secs
    listing = sorted(range(len(units)), key=lambda i: (units[i][3], i))
    with open(log, "a", encoding="utf-8", errors="replace") as handle:
        for i in listing:
            header, output, _, _, _ = results[i]
            handle.write(f"{header}\n{output}\n")
    slowest = sorted(((secs, label) for label, secs in seconds.items()), reverse=True)
    with open(log, "a", encoding="utf-8", errors="replace") as handle:
        handle.write("[gate] seconds per test binary, slowest first:\n")
        handle.writelines(f"[gate] {secs:7.1f} {label}\n" for secs, label in slowest)
    say("slowest test binaries: " + ", ".join(f"{label} {secs:.0f}s" for secs, label in slowest[:3]))
    return seconds


def shard(labels, part):
    """The labels of shard `part` = (k, n): every nth label from the kth,
    in sorted order, so the n shards together run each label exactly once."""
    k, n = part
    return [label for i, label in enumerate(sorted(labels)) if i % n == k]


def parse_shard(text):
    """`K/N` (shards counted from 0) as (k, n)."""
    k, _, n = text.partition("/")
    k, n = int(k), int(n)
    if not 0 <= k < n:
        raise argparse.ArgumentTypeError(f"shard {text}: need 0 <= K < N")
    return k, n


def ci_test(part=(0, 1)):
    """Run every test binary except targets that need generated v20 content,
    or one even share of them (`part`, see `shard`).

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
    known = {entry["test"]: entry for entry in data.get("failure", [])}
    skip_args = [arg for entry in data.get("skip", []) + data.get("nightly", [])
                 for arg in ("--skip", entry["test"])]
    failed = []
    mine = set(shard([label for label, _, _, _ in binaries if label not in skipped], part))
    say(f"shard {part[0]}/{part[1]}: {len(mine)} of {len(binaries) - len(skipped)} test targets")
    for label, executable, cwd, _ in sorted(binaries):
        if label in skipped:
            say(f"skipping {label} (needs generated content)")
            continue
        if label not in mine:
            continue
        say(f"running {label}")
        output, code = run_binary(label, executable, skip_args, cwd)
        print(output, end="", flush=True)
        if code and not ci_tolerated(label, output, known):
            failed.append((label, output))
    if failed:
        # The CI log viewer keeps only the end of a long log, so repeat each
        # failing target's libtest failure report here.
        for label, output in failed:
            lines = output.splitlines()
            if "failures:" in lines:
                lines = lines[lines.index("failures:") :]
            else:
                lines = lines[-200:]
            say(f"failures in {label}:")
            print("\n".join(lines[:200]), flush=True)
        say(f"failing test targets: {', '.join(label for label, _ in failed)}")
        return False
    say("all content-free test targets passed")
    return True


def ci_tolerated(label, output, known):
    """True when every test that failed in one binary's output is a
    [[failure]] entry, as the local gate tolerates them. A hang or crash
    (no failing test named) is never tolerated."""
    failed = {f"{label}::{name}" for name in re.findall(r"^test (\S+) \.\.\. FAILED", output, re.M)}
    if not failed or any(whole_process_failure(key.split("::", 1)[1]) for key in failed):
        return False
    unexpected = sorted(key for key in failed if not matches(key, known))
    for key in sorted(failed - set(unexpected)):
        say(f"known failure (owner: {known[matches(key, known)].get('owner', '?')}): {key}")
    return not unexpected


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
    global TEST_JOBS
    forget_hook_repository()
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--hook", nargs="*", help=argparse.SUPPRESS)
    parser.add_argument("--install-hook", action="store_true")
    parser.add_argument("--diff-only", action="store_true")
    parser.add_argument("--ci-test", action="store_true",
                        help="run the tests that need no generated content (used by CI)")
    parser.add_argument("--shard", type=parse_shard, default=(0, 1), metavar="K/N",
                        help="with --ci-test, run only the Kth of N even shares (from 0)")
    parser.add_argument("--push", action="store_true",
                        help="rebase onto origin/main, gate and push to main under the lock")
    parser.add_argument("--history-range", nargs=2, metavar=("BASE", "TIP"),
                        help="only run the history check on BASE..TIP (used by CI)")
    parser.add_argument("--jobs", type=int, default=None, metavar="N",
                        help=f"test processes at once (default: every logical CPU, {TEST_JOBS})")
    parser.add_argument("commit", nargs="?", default="HEAD")
    args = parser.parse_args()
    if args.jobs:
        TEST_JOBS = max(HEAVY_JOBS + 1, args.jobs)
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
            return 0 if ci_test(args.shard) else 1
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
