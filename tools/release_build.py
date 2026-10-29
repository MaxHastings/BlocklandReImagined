#!/usr/bin/env python3
"""Build the release with unchanged optimization and retain timing/resource evidence.

The Cargo target set and release optimization flags match the original workflows.
Measurements are diagnostic only and never change compiler or host settings.
"""
import argparse
import ctypes
import datetime
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import threading
import time

TARGETS = [('bri-client', 'bri-client'), ('bri-addon-import', 'bri-import-addon'),
           ('bri-net', 'bri-server'), ('bri-package-runtime', 'bri-addon-check'),
           ('bri-sim', 'bri-addon-run')]


def command(system):
    targets = [t for t in TARGETS if system != 'macos' or t[0] != 'bri-net']
    base = ['cargo', 'build', '--release', '--locked', '--timings']
    binaries = [arg for package, binary in targets for arg in ['-p', package, '--bin', binary]]
    return base + binaries


def output(command):
    return subprocess.check_output(command, text=True, timeout=5).strip()


def memory():
    """Host evidence only; no compiler or operating-system settings are changed."""
    system = platform.system()
    if system == 'Windows':
        class Status(ctypes.Structure):
            _fields_ = [('length', ctypes.c_ulong), ('load', ctypes.c_ulong)] + [
                (name, ctypes.c_ulonglong) for name in
                ['total', 'available', 'commit_limit', 'commit_available',
                 'virtual_total', 'virtual_available', 'extended']]
        status = Status()
        status.length = ctypes.sizeof(status)
        if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
            raise OSError('GlobalMemoryStatusEx failed')
        return {'physical_bytes': status.total, 'available_bytes': status.available,
                'commit_bytes': status.commit_limit - status.commit_available}
    if system == 'Darwin':
        vm = output(['vm_stat'])
        page = int(re.search(r'page size of (\d+)', vm)[1])
        counts = {name: int(value) for name, value in re.findall(r'^([^:\n]+):\s+(\d+)\.', vm, re.M)}
        return {'physical_bytes': int(output(['sysctl', '-n', 'hw.memsize'])),
                'free_bytes': counts.get('Pages free', 0) * page,
                'compressed_bytes': counts.get('Pages occupied by compressor', 0) * page,
                'swap': output(['sysctl', '-n', 'vm.swapusage'])}
    info = dict(re.findall(r'^(\w+):\s+(\d+) kB', Path('/proc/meminfo').read_text(), re.M))
    return {key: int(info[name]) * 1024 for key, name in
            [('physical_bytes', 'MemTotal'), ('available_bytes', 'MemAvailable'),
             ('swap_total_bytes', 'SwapTotal'), ('swap_free_bytes', 'SwapFree')]}


def stats():
    try:
        return json.loads(output(['sccache', '--show-stats', '--stats-format=json']))
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        return {'unavailable': str(error)}


class Monitor:
    def __init__(self, directory):
        self.directory = directory
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.sample, daemon=True)

    def sample(self):
        start = time.monotonic()
        with (self.directory / 'host-memory.jsonl').open('w', encoding='utf-8') as stream:
            while not self.stop.is_set():
                try:
                    row = memory()
                except (OSError, subprocess.SubprocessError, ValueError, TypeError, KeyError, IndexError) as error:
                    row = {'unavailable': str(error)}
                row['elapsed_seconds'] = round(time.monotonic() - start, 2)
                stream.write(json.dumps(row) + '\n')
                stream.flush()
                self.stop.wait(10)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(timeout=16)


def build(system, directory, *, run=subprocess.call):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    report = {'schema_version': 1, 'system': system,
              'cpu_count': os.cpu_count(), 'machine': platform.machine(),
              'platform': platform.platform(), 'version': os.environ.get('BRI_VERSION'),
              'started_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'steps': []}
    with Monitor(directory):
        try:
            invocation = command(system)
            print(f'Release build: {" ".join(invocation)}', flush=True)
            start = time.monotonic()
            try:
                code = run(invocation)
            except OSError as error:
                report['launch_error'] = str(error)
                code = 1
            report['steps'].append({'command': invocation, 'exit_code': code,
                                    'elapsed_seconds': round(time.monotonic() - start, 3)})
            timing = Path('target/cargo-timings/cargo-timing.html')
            if timing.is_file():
                shutil.copyfile(timing, directory / 'binaries-timing.html')
            return code
        finally:
            report['sccache'] = stats()
            (directory / 'summary.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--system', choices=['windows', 'macos', 'linux'], required=True)
    parser.add_argument('--output', type=Path, default=Path('artifacts/release-build'))
    args = parser.parse_args()
    raise SystemExit(build(args.system, args.output))


if __name__ == '__main__':
    main()
