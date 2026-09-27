#!/usr/bin/env python3
"""Free disk by deleting the cargo `target/` folders of finished worktrees.

    python tools/clean_targets.py                      # dry run: what would go, and how much
    python tools/clean_targets.py --apply              # delete
    python tools/clean_targets.py --keep NAME --apply  # also spare a worktree (folder name; repeatable)

Only `<worktree>/target` folders are deleted; source, branches and
uncommitted changes are never touched. Build output regenerates on the next
cargo build. The main checkout (whose target/ packaging uses) and the
worktree this runs from are always kept, and so is any target folder that a
cargo build has locked or that changed in the last --recent minutes.
"""
import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import time

REPO = pathlib.Path(__file__).resolve().parents[1]


def worktrees():
    """(path, is_main) for every worktree of this repository."""
    out = subprocess.run(['git', 'worktree', 'list', '--porcelain'], cwd=REPO,
                         capture_output=True, text=True, check=True).stdout
    paths = [pathlib.Path(line[9:]).resolve() for line in out.splitlines() if line.startswith('worktree ')]
    return [(path, index == 0) for index, path in enumerate(paths)]


def size(path):
    total = 0
    for root, _, files in os.walk(path):
        for name in files:
            try:
                total += os.lstat(os.path.join(root, name)).st_size
            except OSError:
                pass
    return total


def locked(target):
    """Whether a cargo build holds a profile's .cargo-lock."""
    for lock in [*target.glob('*/.cargo-lock'), *target.glob('*/*/.cargo-lock')]:
        try:
            with open(lock, 'r+b') as handle:
                if os.name == 'nt':
                    import msvcrt
                    msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                    handle.seek(0)
                    msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    fcntl.flock(handle, fcntl.LOCK_UN)
        except OSError:
            return True
    return False


def recently_changed(target, seconds):
    """Whether the target folder or its first two levels changed recently."""
    now = time.time()
    entries = [target]
    for child in target.iterdir():
        entries.append(child)
        if child.is_dir():
            entries.extend(child.iterdir())
    for entry in entries:
        try:
            if now - entry.stat().st_mtime < seconds:
                return True
        except OSError:
            pass
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--apply', action='store_true', help='delete (default: dry run)')
    parser.add_argument('--keep', action='append', default=[], help='worktree folder name to keep')
    parser.add_argument('--recent', type=float, default=30, help='minutes of activity that keep a folder')
    args = parser.parse_args()
    keep = {name.lower() for name in args.keep}
    here = REPO.resolve()
    total = 0
    for path, is_main in worktrees():
        target = path / 'target'
        if not target.is_dir():
            continue
        reason = ('main checkout' if is_main else 'this worktree' if path == here
                  else 'kept' if path.name.lower() in keep
                  else 'build running' if locked(target)
                  else f'changed in the last {args.recent:g} min' if recently_changed(target, args.recent * 60)
                  else None)
        if reason:
            print(f'keep    {path.name:<40} ({reason})')
            continue
        amount = size(target)
        if args.apply:
            shutil.rmtree(target, ignore_errors=True)
            left = size(target) if target.exists() else 0
            amount -= left
            print(f'deleted {path.name:<40} {amount / 2**30:7.1f} GB'
                  + (f'  ({left / 2**30:.1f} GB in use, left)' if left else ''), flush=True)
        else:
            print(f'delete  {path.name:<40} {amount / 2**30:7.1f} GB', flush=True)
        total += amount
    verb = 'Freed' if args.apply else 'Would free'
    print(f'{verb} {total / 2**30:.1f} GB.' + ('' if args.apply else ' Rerun with --apply to delete.'))


if __name__ == '__main__':
    sys.exit(main())
