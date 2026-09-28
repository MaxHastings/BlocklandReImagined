#!/usr/bin/env bash
# Linux counterpart of package_playtest.ps1: copy the release client and the
# content packs the package list selects into dist/, with a checksummed manifest.
#
#   tools/package_playtest.sh --version a8 --sha256 <release-client-sha256>
#   tools/package_playtest.sh --validate-only
#   tools/package_playtest.sh --verify dist/BlocklandReImagined-alpha-a8-linux
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
executable="$repo/target/release/bri-client"
destination="$repo/dist"
version="" expected="" validate_only=0 verify=""
fields=(map_bundle brick_catalog geometry effects worlds ui_pack brick_materials avatar
        effects_runtime audio weather foliage weapons item_presentation weapon_debris
        vehicles events tutorial)

die() { echo "package_playtest: $*" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --sha256) expected="$2"; shift 2 ;;
        --executable) executable="$2"; shift 2 ;;
        --importer) importer="$2"; shift 2 ;;
        --destination) destination="$2"; shift 2 ;;
        --validate-only) validate_only=1; shift ;;
        --verify) verify="$2"; shift 2 ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *) die "unknown argument $1" ;;
    esac
done

# Every packaged file except MANIFEST.json, logs/ and user-state/, sorted.
list_files() {
    (cd "$1" && find . -type f ! -path './MANIFEST.json' ! -path './logs/*' ! -path './user-state/*' \
        | sed 's|^\./||' | LC_ALL=C sort)
}

verify_package() {
    local root="$1" manifest="$1/MANIFEST.json"
    [[ -f "$manifest" ]] || die "missing $manifest"
    [[ -z "$(find "$root" -type l)" ]] || die "package contains symbolic links"
    python3 - "$root" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
manifest = json.loads((root / 'MANIFEST.json').read_text())
listed = [entry['path'] for entry in manifest['files']]
if manifest.get('schema_version') != 1 or not listed or listed != sorted(set(listed)):
    sys.exit('Invalid or unsorted package manifest')
for entry in manifest['files']:
    path = entry['path']
    if '\\' in path or ':' in path or any(part in ('', '.', '..') for part in path.split('/')):
        sys.exit(f'Unsafe manifest path: {path}')
    data = (root / path).read_bytes()
    if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
        sys.exit(f'Package checksum mismatch: {path}')
actual = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file())
actual = [p for p in actual if p != 'MANIFEST.json' and not p.startswith(('logs/', 'user-state/'))]
if actual != listed:
    sys.exit('Package contains unlisted or missing files')
print(f"Verified {len(listed)} files for package version {manifest['version']}.")
PY
}

if [[ -n "$verify" ]]; then verify_package "$verify"; exit 0; fi

# The base package list, or content/packages.json when present: role=dir.
declare -A packs
list="$repo/crates/package/base-packages.json"
[[ -f "$repo/content/packages.json" ]] && list="$repo/content/packages.json"
while IFS='=' read -r field name; do
    packs[$field]="$name"
done < <(python3 -c 'import json,sys; [print(p["role"] + "=" + p["dir"]) for p in json.load(open(sys.argv[1]))["packages"] if p.get("role")]' "$list")
for field in "${fields[@]}"; do
    [[ -n "${packs[$field]+x}" ]] || die "$list has no package for the $field role"
done

[[ -f "$executable" && ! -L "$executable" && -s "$executable" ]] || die "release client missing; build it first: $executable"
# The Add-Ons screen's Import runs bri-import-addon from beside the client.
importer="${importer:-$(dirname "$executable")/bri-import-addon}"
[[ -f "$importer" && ! -L "$importer" && -s "$importer" ]] || die "Add-On importer missing; build it first (cargo build --release --locked -p bri-addon-import): $importer"
sha="$(sha256sum "$executable" | cut -d' ' -f1)"
total_files=0 total_bytes=0
for field in "${fields[@]}"; do
    name="${packs[$field]}"
    [[ "$name" != *..* && "$name" != /* && "$name" != *\\* ]] || die "unsafe package name for $field: $name"
    dir="$repo/content/$name"
    [[ -d "$dir" && ! -L "$dir" ]] || die "selected $field package is missing or a link: $dir"
    [[ -z "$(find "$dir" -type l)" ]] || die "$dir contains symbolic links"
    files=$(find "$dir" -type f | wc -l)
    [[ "$files" -gt 0 ]] || die "selected $field package is empty: $dir"
    total_files=$((total_files + files))
    total_bytes=$((total_bytes + $(du -sb "$dir" | cut -f1)))
done
for doc in docs/PLAYTEST.md docs/KNOWN-ISSUES.md docs/TESTER-GUIDE.md docs/FEATURES.md tools/launch_playtest.sh; do
    [[ -f "$repo/$doc" ]] || die "required package file missing: $doc"
done
if [[ "$validate_only" -eq 1 ]]; then
    echo "packages: ${#fields[@]}, content files: $total_files, content bytes: $total_bytes, client sha256: $sha"
    exit 0
fi

[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ ]] || die "supply --version with 1-64 letters, digits, dot, underscore or dash"
[[ "$expected" =~ ^[0-9A-Fa-f]{64}$ ]] || die "supply --sha256 with the release client's expected SHA-256"
[[ "$sha" == "${expected,,}" ]] || die "release client hash differs from the expected build: $sha"
release="$destination/BlocklandReImagined-alpha-$version-linux"
[[ ! -e "$release" ]] || die "refusing to overwrite $release"
mkdir -p "$release/content"
trap 'rm -rf "$release"' ERR
install -m 755 "$executable" "$release/bri-client"
install -m 755 "$importer" "$release/bri-import-addon"
cp "$repo/docs/PLAYTEST.md" "$repo/docs/KNOWN-ISSUES.md" "$repo/docs/TESTER-GUIDE.md" "$repo/docs/FEATURES.md" "$release/"
install -m 755 "$repo/tools/launch_playtest.sh" "$release/launch.sh"
cp "$list" "$release/content/packages.json"
for field in "${fields[@]}"; do
    cp -r "$repo/content/${packs[$field]}" "$release/content/"
done
python3 - "$release" "$version" <<'PY'
import hashlib, json, pathlib, sys
root, version = pathlib.Path(sys.argv[1]), sys.argv[2]
files = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file())
entries = []
for path in files:
    data = (root / path).read_bytes()
    entries.append({'path': path, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
manifest = {'schema_version': 1, 'version': version, 'executable': 'bri-client',
            'content_config': 'content/packages.json', 'files': entries}
(root / 'MANIFEST.json').write_text(json.dumps(manifest, indent=2) + '\n')
PY
trap - ERR
echo "Created $release ($total_files content files, $total_bytes bytes)."
echo "Verify with: tools/package_playtest.sh --verify \"$release\""
