#!/usr/bin/env python3
"""Set up a fresh checkout: toolchain check, content, client build, --check.

    python tools/bootstrap.py --v20 "/path/to/Blockland v20"

The v20 folder is the one holding base/, Add-Ons/ and saves/. It is only read.
The path is remembered, so later runs need no arguments. Every step is
idempotent: a rerun only rebuilds content that is missing or out of date
(see tools/regenerate_content.py), then rebuilds the client and checks it.

When it finishes, start the game with the command it prints.
"""
import argparse
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys

if sys.version_info < (3, 9):
    sys.exit('Python 3.9 or newer is needed (this is %d.%d).' % sys.version_info[:2])

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import regenerate_content as regen  # noqa: E402

REPO = regen.REPO


def distro():
    """Linux distribution family from /etc/os-release: debian, arch, fedora or ''."""
    try:
        text = pathlib.Path('/etc/os-release').read_text()
    except OSError:
        return ''
    ids = ' '.join(re.findall(r'^(?:ID|ID_LIKE)=(.*)$', text, re.M)).replace('"', '').split()
    for family, names in (('debian', {'debian', 'ubuntu'}), ('arch', {'arch', 'cachyos', 'manjaro'}),
                          ('fedora', {'fedora', 'rhel'})):
        if names.intersection(ids):
            return family
    return ''


def linux_packages(debian, arch, fedora):
    return {'debian': f'sudo apt install {debian}', 'arch': f'sudo pacman -S --needed {arch}',
            'fedora': f'sudo dnf install {fedora}'}.get(
        distro(), f'install your distribution\'s packages for: {debian} (Debian names)')


def output(*command):
    try:
        result = subprocess.run(command, capture_output=True, text=True)
    except OSError:
        return None
    return result.stdout if result.returncode == 0 else None


def check_prerequisites(need_dotnet):
    """Every missing tool with the exact command that installs it."""
    problems = []
    if not shutil.which('git'):
        problems.append(('git', {'darwin': 'xcode-select --install',
                                 'win32': 'winget install --id Git.Git -e'}.get(
            sys.platform, linux_packages('git', 'git', 'git'))))
    wanted = re.search(r'^rust-version\s*=\s*"([\d.]+)"',
                       (REPO / 'Cargo.toml').read_text(encoding='utf-8'), re.M).group(1)
    rustc = output('rustc', '--version') if shutil.which('cargo') else None
    if not rustc:
        problems.append(('Rust (cargo and rustc)', 'install rustup from https://rustup.rs, then open a new terminal'
                         if sys.platform != 'win32' else 'winget install --id Rustlang.Rustup -e, then open a new terminal'))
    else:
        have = tuple(int(n) for n in re.search(r'(\d+)\.(\d+)', rustc).groups())
        if have < tuple(int(n) for n in wanted.split('.')[:2]):
            problems.append((f'Rust {wanted} or newer (found {rustc.split()[1]})', 'rustup update stable'))
    try:
        import PIL  # noqa: F401
    except ImportError:
        problems.append(('Pillow for Python', f'"{sys.executable}" -m pip install pillow'))
    if sys.platform.startswith('linux'):
        if not (shutil.which('cc') or shutil.which('gcc') or shutil.which('clang')):
            problems.append(('a C compiler', linux_packages('build-essential', 'base-devel', 'gcc')))
        if not shutil.which('pkg-config') and not shutil.which('pkgconf'):
            problems.append(('pkg-config', linux_packages('pkg-config', 'pkgconf', 'pkgconf-pkg-config')))
        else:
            if output('pkg-config', '--exists', 'alsa') is None:
                problems.append(('ALSA headers (audio)', linux_packages('libasound2-dev', 'alsa-lib', 'alsa-lib-devel')))
            if output('pkg-config', '--exists', 'libudev') is None:
                problems.append(('udev headers (gamepads)', linux_packages('libudev-dev', 'systemd-libs', 'systemd-devel')))
    if sys.platform == 'darwin' and not output('xcode-select', '-p'):
        problems.append(('Xcode command line tools', 'xcode-select --install'))
    if need_dotnet and os.name != 'nt':
        dotnet = shutil.which('dotnet')
        if not dotnet or regen.dotnet_sdk_major(dotnet) < 8:
            problems.append(('.NET 8 or newer SDK (decompiles the v20 scripts once)', regen.DOTNET_HINT))
    return problems


def configure_build_cache():
    """Compile through sccache when it is installed, so checkouts on one machine
    share compiled dependencies instead of each building them cold."""
    if os.environ.get('RUSTC_WRAPPER'):
        print(f'Build cache: RUSTC_WRAPPER={os.environ["RUSTC_WRAPPER"]} (already set).')
        return
    sccache = shutil.which('sccache')
    if not sccache:
        print('Build cache: none. Optional: `cargo install sccache --locked` shares compiled '
              'dependencies between checkouts (see AGENTS.md, "Builds and disk").')
        return
    os.environ['RUSTC_WRAPPER'] = sccache
    os.environ.setdefault('SCCACHE_CACHE_SIZE', '40G')
    print(f'Build cache: sccache, capped at {os.environ["SCCACHE_CACHE_SIZE"]}.')


def remembered_v20(content):
    path = content / '_regeneration' / 'v20-path.txt'
    try:
        return pathlib.Path(path.read_text(encoding='utf-8').strip())
    except OSError:
        return None


def remember_v20(content, v20):
    folder = content / '_regeneration'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'v20-path.txt').write_text(str(v20) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--v20', type=pathlib.Path,
                        help='Blockland v20 folder (default: $BRI_V20, then the last one used)')
    parser.add_argument('--content', type=pathlib.Path, default=REPO / 'content',
                        help='content directory to fill (default: <repo>/content)')
    parser.add_argument('--keep-stale', action='store_true', help='keep packs whose inputs changed')
    parser.add_argument('--rebuild', choices=regen.PACK_STEPS, action='append', default=[],
                        help='rebuild this pack even if it is up to date')
    parser.add_argument('--prerequisites', action='store_true', help='only check the toolchain')
    args = parser.parse_args()
    content = args.content.resolve()

    v20 = args.v20 or (pathlib.Path(os.environ['BRI_V20']) if os.environ.get('BRI_V20') else None) \
        or remembered_v20(content)
    need_dotnet = not (regen.CORE.exists() and regen.DAMAGE_TYPES.exists())
    print(f'Blockland ReImagined setup on {platform.system()} {platform.machine()}, Python {platform.python_version()}')
    problems = check_prerequisites(need_dotnet)
    if problems:
        print('\nInstall these first, then rerun this command:')
        for name, hint in problems:
            print(f'  - {name}:\n      {hint}')
        sys.exit(1)
    print('Toolchain OK.')
    configure_build_cache()
    if args.prerequisites:
        return
    if v20 is None:
        regen.fail('Pass the Blockland v20 folder once: python tools/bootstrap.py --v20 "/path/to/Blockland v20"\n'
                   'It is the folder with base/, Add-Ons/ and saves/ (for example the B4v21 '
                   'launcher\'s versions/Blockland v20).')
    v20 = v20.expanduser().resolve()
    if not regen.valid_v20(v20):
        regen.fail(f'{v20} is not a Blockland v20 folder: it needs base/, Add-Ons/ and saves/.')
    remember_v20(content, v20)
    regen.regenerate(v20, content, regen.STEPS, keep_stale=args.keep_stale, force=args.rebuild)
    client = REPO / 'target' / 'release' / ('bri-client' + regen.EXE)
    print('\nSetup complete. Start the game with:')
    print(f'  "{client}" --run "{content}"')


if __name__ == '__main__':
    main()
