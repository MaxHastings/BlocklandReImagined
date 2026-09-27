"""Read-only source comparison, including decompressed ZIP member hashes.

Only base/, Add-Ons/ and saves/ are read. No scripts are executed, archives
extracted, user settings read, or original files written. Hash metadata can be
retained without copying original game content into the repository.
"""
import argparse
import hashlib
import json
import re
import zipfile
from collections import Counter
from pathlib import Path


def digest(stream):
    result = hashlib.sha256()
    while chunk := stream.read(1024 * 1024):
        result.update(chunk)
    return result.hexdigest()


def inventory(root):
    physical, assets, maps, errors = {}, {}, [], []

    def record(path, source, stream, size):
        key = path.replace('\\', '/').casefold()
        row = dict(path=path, source=source, bytes=size, sha256=digest(stream))
        assets.setdefault(key, []).append(row)
        return row

    for folder in ('base', 'Add-Ons', 'saves'):
        for path in sorted((root / folder).rglob('*')):
            if not path.is_file():
                continue
            if not path.resolve().is_relative_to(root):
                raise ValueError(f'Source escapes root: {path}')
            rel = path.relative_to(root).as_posix()
            with path.open('rb') as stream:
                physical[rel.casefold()] = dict(path=rel, bytes=path.stat().st_size,
                                               sha256=digest(stream))
            if path.suffix.casefold() != '.zip':
                with path.open('rb') as stream:
                    record(rel, rel, stream, path.stat().st_size)
                continue
            try:
                with zipfile.ZipFile(path) as archive:
                    for member in archive.infolist():
                        if member.is_dir():
                            continue
                        if member.file_size > 128 * 1024 * 1024:
                            raise ValueError(f'Oversize member: {member.filename}')
                        virtual = f'{path.with_suffix("").relative_to(root).as_posix()}/{member.filename}'
                        with archive.open(member) as stream:
                            row = record(virtual, rel, stream, member.file_size)
                        if path.stem.startswith('Map_') and member.filename.casefold().endswith('.mis'):
                            if member.file_size > 2 * 1024 * 1024:
                                raise ValueError('Oversize mission')
                            mission = archive.read(member).decode('utf8', errors='replace')
                            fields = {}
                            for field in ('name', 'saveName', 'materialList', 'fogDistance', 'visibleDistance'):
                                match = re.search(r'\b' + field + r'\s*=\s*"([^"\r\n]*)"', mission, re.I)
                                fields[field] = match[1] if match else None
                            maps.append(dict(package=path.stem, mission=virtual,
                                             sha256=row['sha256'], fields=fields))
            except (ValueError, OSError, RuntimeError, zipfile.BadZipFile) as exc:
                errors.append(dict(source=rel, error=str(exc)))
    return dict(root=str(root), physical=physical, assets=assets, maps=maps,
                errors=errors,
                duplicates={k: v for k, v in assets.items() if len(v) > 1})


def compare(old, new):
    same, changed, added, ambiguous = [], [], [], []
    for path, rows in new['assets'].items():
        prior = old['assets'].get(path, [])
        if len(rows) != 1 or len(prior) > 1:
            ambiguous.append(path)
        elif not prior:
            added.append(path)
        elif rows[0]['sha256'] == prior[0]['sha256']:
            same.append(path)
        else:
            changed.append(path)
    return dict(identical=same, changed=changed, new_only=added, ambiguous=ambiguous,
                old_only=sorted(set(old['assets']) - set(new['assets'])))


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('old', type=Path)
    parser.add_argument('reference', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    roots = [args.old.resolve(strict=True), args.reference.resolve(strict=True)]
    output = args.output.resolve()
    if any(output.is_relative_to(root) for root in roots):
        raise ValueError('Output must be outside both source installations')
    old, new = [inventory(root) for root in roots]
    delta = compare(old, new)
    packages = []
    for key, row in new['physical'].items():
        if key.startswith('add-ons/') and key.endswith('.zip'):
            prefix = key[:-4] + '/'
            members = {kind: sum(p.startswith(prefix) for p in paths)
                       for kind, paths in delta.items() if kind != 'old_only'}
            prior = old['physical'].get(key)
            packages.append(dict(**row, archive_identical=bool(prior and prior['sha256'] == row['sha256']),
                                 members=members,
                                 old_only_members=sum(p.startswith(prefix) for p in delta['old_only'])))
    summary = dict(old_assets=len(old['assets']), reference_assets=len(new['assets']),
                   comparison={k: len(v) for k, v in delta.items()},
                   reference_packages=len(packages), reference_missions=len(new['maps']),
                   package_families=dict(Counter(Path(p['path']).stem.split('_')[0] for p in packages)),
                   old_errors=len(old['errors']), reference_errors=len(new['errors']))
    report = dict(schema_version=1, scope=['base', 'Add-Ons', 'saves'],
                  note='User-designated baseline; presence alone does not establish historical official distribution. Settings, launcher modules, cache, logs and screenshots excluded.',
                  summary=summary, old=old, reference=new, comparison=delta, packages=packages)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    print(json.dumps(summary, indent=2))
    if new['errors'] or new['duplicates']:
        raise SystemExit('Reference has unreadable or ambiguous assets; inspect report')


if __name__ == '__main__':
    main()
