"""Index stock package names and literal event registrations; never execute scripts.

This is provenance/coverage metadata, not a copied script or a runtime package.
Activation order, dynamic declarations and engine-provided behavior need review.
"""
import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

EVENT = re.compile(
    r'^\s*register(Input|Output)Event\(\s*"([^"\n]+)"\s*,\s*"([^"\n]+)"',
    re.MULTILINE,
)


def events(source, origin):
    return [dict(kind=m[1].lower(), target_class=m[2], name=m[3], source=origin,
                 line=source.count('\n', 0, m.start()) + 1, status='pending')
            for m in EVENT.finditer(source)]


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('v20_root', type=Path)
    parser.add_argument('--defaults', type=Path, default=Path('.research/bl-decompiled/v20/server/defaultAddOnList.cs'))
    parser.add_argument('--core', type=Path, default=Path('.research/v20-dso/server/scripts/allGameScripts-Vanilla.cs'))
    parser.add_argument('--out', type=Path, default=Path('docs/vanilla-inventory.json'))
    args = parser.parse_args()
    defaults = args.defaults.read_text(encoding='utf8')
    core = args.core.read_text(encoding='utf8')
    packages, missing = [], []
    registrations = events(core, 'core/allGameScripts-Vanilla.cs')
    core_counts = {kind: sum(e['kind'] == kind for e in registrations) for kind in ['input', 'output']}
    names = set()
    for line, text in enumerate(defaults.splitlines(), 1):
        match = re.fullmatch(r'\s*\$AddOn__([A-Za-z0-9_]+)\s*=\s*1\s*;\s*', text)
        if not match:
            continue
        name = match[1]
        if name.lower() in names:
            raise ValueError(f'Duplicate package: {name}')
        names.add(name.lower())
        path = args.v20_root / 'Add-Ons' / (name + '.zip')
        package = dict(name=name, family=name.split('_', 1)[0].lower(), default_list_line=line,
                       status='pending', present=path.is_file())
        if path.is_file():
            package['archive_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
            with zipfile.ZipFile(path) as archive:
                for member in archive.infolist():
                    if member.filename.lower().endswith('.cs'):
                        if member.file_size > 16 * 1024 * 1024:
                            raise ValueError(f'Oversized script: {name}/{member.filename}')
                        raw = archive.read(member)
                        try:
                            script = raw.decode('utf8')
                        except UnicodeDecodeError:
                            script = raw.decode('latin1')
                        script = script.replace('\r\n', '\n').replace('\r', '\n')
                        registrations.extend(events(script, f'Add-Ons/{name}/{member.filename}'))
        else:
            missing.append(name)
        packages.append(package)
    result = dict(schema_version=1, scope='Stock default-enabled package baseline and literal core/add-on event registration candidates. Not a complete vanilla inventory or proof of activation/behavior.',
                  default_list_sha256=hashlib.sha256(args.defaults.read_bytes()).hexdigest(),
                  core_script_sha256=hashlib.sha256(args.core.read_bytes()).hexdigest(),
                  packages=packages, event_registrations=registrations,
                  missing_packages=missing, core_event_counts=core_counts,
                  unindexed=['shipped-but-disabled stock packages', 'core built-in content and engine behavior',
                             'dynamic registration', 'maps and Tutorial availability', 'full UI/workflow coverage'])
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + '\n', encoding='utf8')
    print(json.dumps(dict(packages=len(packages), missing=missing, core_events=core_counts,
                          addon_registration_candidates=len(registrations)-sum(core_counts.values()))))
    if missing:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
