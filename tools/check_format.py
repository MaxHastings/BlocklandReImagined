#!/usr/bin/env python3
"""Check every workspace package without exceeding Windows command limits."""
import json
import pathlib
import subprocess
import sys


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    metadata = subprocess.run(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked'],
        cwd=root, capture_output=True, text=True, check=True,
    )
    data = json.loads(metadata.stdout)
    members = set(data['workspace_members'])
    for package in sorted(data['packages'], key=lambda p: p['name']):
        if package['id'] not in members:
            continue
        print('[fmt]', package['name'], flush=True)
        result = subprocess.run(
            ['cargo', 'fmt', '--package', package['name'], '--check'], cwd=root,
        )
        if result.returncode:
            return result.returncode
    return 0


if __name__ == '__main__':
    sys.exit(main())
