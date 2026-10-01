#!/usr/bin/env bash
# macOS counterpart of package_playtest.ps1: BlocklandReImagined.app with the
# release client, the Add-On importer and the same content the Windows zip
# carries (the packs the package list selects, the default Add-Ons from
# packages/default-addons.json, the bundled originals from the Add-On bundle
# (tools/addon_bundle.py) with their CREDITS.md and, with --stress-lab,
# packages/stresslab),
# ad-hoc signed, in a folder with the docs and a checksummed MANIFEST.json,
# zipped with ditto.
#
#   BRI_VERSION=<v> cargo build --release --locked -p bri-client -p bri-addon-import
#   tools/package_mac.sh --version <v> --sha256 <bri-client sha256> [--stress-lab]
#   tools/package_mac.sh --validate-only [--stress-lab]
#   tools/package_mac.sh --verify dist/BlocklandReImagined-<v>-macos.zip
#   [--addon-bundle DIR] [--without-originals]   (the latter only for packaging tests)
#
# Works with macOS's own bash 3.2. Needs python3, codesign and ditto.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
executable="$repo/target/release/bri-client"
importer=""
destination="$repo/dist"
version="" expected="" validate_only=0 verify="" stress_lab=0 skip_version_check=0
identity="-"
addon_bundle="$repo/dist/addon-bundle" without_originals=0
bundle_id="io.github.maxhastings.blocklandreimagined"

die() { echo "package_mac: $*" >&2; exit 1; }
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --sha256) expected="$2"; shift 2 ;;
        --executable) executable="$2"; shift 2 ;;
        --importer) importer="$2"; shift 2 ;;
        --destination) destination="$2"; shift 2 ;;
        --stress-lab) stress_lab=1; shift ;;
        --addon-bundle) addon_bundle="$2"; shift 2 ;;
        --without-originals) without_originals=1; shift ;;
        # A Developer ID identity when there is one; "-" is ad-hoc.
        --sign-identity) identity="$2"; shift 2 ;;
        --validate-only) validate_only=1; shift ;;
        --verify) verify="$2"; shift 2 ;;
        # Packaging tests use a stand-in executable that cannot report a version.
        --skip-version-check) skip_version_check=1; shift ;;
        -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
        *) die "unknown argument $1" ;;
    esac
done
[[ "$(uname -s)" == Darwin ]] || die "run this on macOS (it needs codesign and ditto)"
importer="${importer:-$(dirname "$executable")/bri-import-addon}"

# The release folder in $1: every file listed in MANIFEST.json with its size
# and hash, nothing else, the app's signature intact and every default Add-On
# turned on.
verify_release() {
    local root="$1"
    [[ -f "$root/MANIFEST.json" ]] || die "missing $root/MANIFEST.json"
    [[ -z "$(find "$root" -type l)" ]] || die "release contains symbolic links"
    python3 - "$root" "$repo" "$without_originals" <<'PY'
import hashlib, json, pathlib, sys
root, repo = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
sys.path.insert(0, str(repo / 'tools'))
import addon_bundle
manifest = json.loads((root / 'MANIFEST.json').read_text())
listed = [entry['path'] for entry in manifest['files']]
if manifest.get('schema_version') != 1 or not listed or listed != sorted(set(listed)):
    sys.exit('Invalid or unsorted release manifest')
for entry in manifest['files']:
    path = entry['path']
    if '\\' in path or ':' in path or any(part in ('', '.', '..') for part in path.split('/')):
        sys.exit(f'Unsafe manifest path: {path}')
    data = (root / path).read_bytes()
    if len(data) != entry['bytes'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
        sys.exit(f'Release checksum mismatch: {path}')
actual = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file())
actual = [p for p in actual if p != 'MANIFEST.json']
if actual != listed:
    sys.exit('Release contains unlisted or missing files')
content = root / manifest['content_config']
addon_bundle.verify_defaults(repo, content.parent, root / addon_bundle.CREDITS, sys.argv[3] == '1')
print(f"Verified {len(listed)} files for release version {manifest['version']}.")
PY
    codesign --verify --deep --strict "$root/BlocklandReImagined.app" || die "the app's signature does not verify"
    echo "Verified the app's signature."
}

if [[ -n "$verify" ]]; then
    if [[ "$verify" == *.zip ]]; then
        unpacked="$(mktemp -d)"
        trap 'rm -rf "$unpacked"' EXIT
        ditto -x -k "$verify" "$unpacked"
        tops=("$unpacked"/*)
        [[ ${#tops[@]} -eq 1 && -d "${tops[0]}" ]] || die "the zip must hold exactly one release folder"
        verify_release "${tops[0]}"
    else
        verify_release "$verify"
    fi
    exit 0
fi

# What goes in the app, as package_playtest.ps1 chooses it: the packs the
# package list gives a role (content/packages.json when present, else the
# base list), then every default Add-On and, with --stress-lab, the Stress
# Lab ones, each at content/<prefix>/<id>. Printed as JSON.
plan_file="$(mktemp)"
python3 - "$repo" "$stress_lab" "$addon_bundle" "$without_originals" > "$plan_file" <<'PY'
import json, pathlib, sys
repo, stress_lab = pathlib.Path(sys.argv[1]), sys.argv[2] == '1'
sys.path.insert(0, str(repo / 'tools'))
import addon_bundle
import content_packs
fields = ['map_bundle', 'brick_catalog', 'geometry', 'effects', 'worlds', 'ui_pack',
          'brick_materials', 'avatar', 'effects_runtime', 'audio', 'weather', 'foliage',
          'weapons', 'item_presentation', 'weapon_debris', 'vehicles', 'events', 'tutorial']
def fail(message):
    sys.exit(f'package_mac: {message}')
source, listing, _ = content_packs.package_list(repo / 'content', repo)
if listing.get('schema_version') != 1:
    fail(f'unsupported package list schema in {source}')
# Only the base packs: the Add-Ons a checkout's game installed into its own
# list come from packages/ below, as in every release.
packs = [p for p in listing['packages'] if p.get('role')]
for pack in packs:
    name = pack['dir']
    if not name or '\\' in name or ':' in name or any(part in ('', '.', '..') for part in name.split('/')):
        fail(f"unsafe package directory for {pack['id']}: {name}")
missing = [f for f in fields if f not in {p['role'] for p in packs}]
if missing:
    fail(f"{source} has no package for the {', '.join(missing)} role")

def side(manifest):
    # Keep in step with bri_package::library's side_for_kinds (and the
    # Windows packager's New-ModPackage).
    kinds = [p['kind'] for p in manifest.get('provides') or [] if p]
    # Client code follows the host (shared) unless it is personal.
    code = manifest.get('client')
    personal = isinstance(code, dict) and code.get('personal') is True
    if kinds and all(k in ('behaviour', 'script', 'world', 'entity', 'mode', 'archetype') for k in kinds):
        return 'server'
    if all(k in ('model', 'hud') for k in kinds) and (personal or (code is None and kinds)):
        return 'client'
    return 'shared'

def mod(directory, prefix):
    manifest = json.loads((directory / 'package.json').read_text())
    return {'id': manifest['id'], 'version': manifest['version'], 'side': side(manifest),
            'path': str(directory), 'dir': f"{prefix}/{manifest['id']}"}

# Our own from packages/, the bundled originals from the Add-On bundle.
defaults, credits = addon_bundle.default_sources(repo, pathlib.Path(sys.argv[3]).resolve(), sys.argv[4] == '1')
mods = [dict(mod(pathlib.Path(a['path']), 'addons'), enabled=a['enabled']) for a in defaults]
if stress_lab:
    found = sorted(d for d in (repo / 'packages/stresslab').iterdir() if (d / 'package.json').is_file())
    if not found:
        fail('no Add-Ons found in packages/stresslab')
    mods += [mod(d, 'stresslab') for d in found]
print(json.dumps({'source': str(source), 'schema_version': listing['schema_version'],
                  'packs': packs, 'mods': mods, 'credits': str(credits) if credits else ''}))
PY
plan="$(cat "$plan_file")"
rm -f "$plan_file"
field() { python3 -c 'import json,sys; plan=json.loads(sys.argv[1]); exec(sys.argv[2])' "$plan" "$1"; }

total_files=0 total_bytes=0
while IFS= read -r name; do
    dir="$repo/content/$name"
    [[ -d "$dir" && ! -L "$dir" ]] || die "selected package is missing or a link: $dir"
    [[ -z "$(find "$dir" -type l)" ]] || die "$dir contains symbolic links"
    files=$(find "$dir" -type f | wc -l | tr -d ' ')
    [[ "$files" -gt 0 ]] || die "selected package is empty: $dir"
    total_files=$((total_files + files))
    total_bytes=$((total_bytes + $(find "$dir" -type f -exec stat -f %z {} + | awk '{s+=$1} END {print s+0}')))
done < <(field 'print("\n".join(p["dir"] for p in plan["packs"]))')
while IFS= read -r path; do
    [[ -z "$(find "$path" -type l)" ]] || die "$path contains symbolic links"
done < <(field 'print("\n".join(m["path"] for m in plan["mods"]))')

docs=(docs/PLAYTEST.md docs/KNOWN-ISSUES.md docs/TESTER-GUIDE.md docs/FEATURES.md docs/PLAYTEST-MAC.md)
[[ "$stress_lab" -eq 1 ]] && docs+=(docs/stress-lab/PLAYTEST-STRESS-LAB.md)
for doc in "${docs[@]}"; do
    [[ -f "$repo/$doc" ]] || die "required release file missing: $doc"
done
[[ -f "$executable" && ! -L "$executable" && -s "$executable" ]] || die "release client missing; build it first: $executable"
[[ -f "$importer" && ! -L "$importer" && -s "$importer" ]] || die "Add-On importer missing; build it first (cargo build --release --locked -p bri-addon-import): $importer"
sha="$(shasum -a 256 "$executable" | cut -d' ' -f1)"
mods="$(field 'print(", ".join(m["id"] for m in plan["mods"]))')"
if [[ "$validate_only" -eq 1 ]]; then
    echo "packages: $(field 'print(len(plan["packs"]))'), content files: $total_files, content bytes: $total_bytes"
    echo "Add-Ons: $mods"
    echo "client sha256: $sha"
    exit 0
fi

[[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ ]] || die "supply --version with 1-64 letters, digits, dot, underscore or dash"
[[ "$expected" =~ ^[0-9A-Fa-f]{64}$ ]] || die "supply --sha256 with the release client's expected SHA-256"
[[ "$sha" == "$(echo "$expected" | tr 'A-F' 'a-f')" ]] || die "release client hash differs from the expected build: $sha"
if [[ "$skip_version_check" -eq 0 ]]; then
    # The version the build carries must be the release's, or the main menu,
    # logs and update check would name another build.
    built="$("$executable" --version | awk '{print $1}')"
    [[ "$built" == "$version" ]] || die "the client reports version '$built', not '$version'; rebuild with BRI_VERSION=$version"
fi

release="$destination/BlocklandReImagined-$version-macos"
zip="$release.zip"
[[ ! -e "$release" && ! -e "$zip" ]] || die "refusing to overwrite $release or its zip"
mkdir -p "$destination"
trap 'rm -rf "$release" "$zip"' ERR
app="$release/BlocklandReImagined.app"
content="$app/Contents/Resources/content"
mkdir -p "$app/Contents/MacOS" "$content"
install -m 755 "$executable" "$app/Contents/MacOS/bri-client"
# The Add-Ons screen's Import runs bri-import-addon from beside the client.
install -m 755 "$importer" "$app/Contents/MacOS/bri-import-addon"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Blockland ReImagined</string>
    <key>CFBundleDisplayName</key><string>Blockland ReImagined</string>
    <key>CFBundleIdentifier</key><string>$bundle_id</string>
    <key>CFBundleExecutable</key><string>bri-client</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.games</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSLocalNetworkUsageDescription</key><string>Blockland ReImagined finds and hosts LAN games on your local network.</string>
</dict>
</plist>
PLIST
while IFS= read -r name; do
    mkdir -p "$content/$(dirname "$name")"
    cp -R "$repo/content/$name" "$content/$name"
done < <(field 'print("\n".join(p["dir"] for p in plan["packs"]))')
while IFS=$'\t' read -r path dir; do
    mkdir -p "$content/$(dirname "$dir")"
    cp -R "$path" "$content/$dir"
done < <(field 'print("\n".join(m["path"] + "\t" + m["dir"] for m in plan["mods"]))')
python3 - "$plan" "$content/packages.json" <<'PY'
import json, sys
plan = json.loads(sys.argv[1])
packages = plan['packs'] + [{k: m[k] for k in ('id', 'version', 'side', 'dir')} for m in plan['mods']
                            if m.get('enabled', True)]
with open(sys.argv[2], 'w') as out:
    out.write(json.dumps({'schema_version': plan['schema_version'], 'packages': packages}, indent=2) + '\n')
PY
for doc in "${docs[@]}"; do
    cp "$repo/$doc" "$release/"
done
credits="$(field 'print(plan["credits"])')"
[[ -z "$credits" ]] || cp "$credits" "$release/CREDITS.md"
# Finder metadata would change the sealed bundle after signing.
find "$release" -name .DS_Store -delete
codesign --force --deep --sign "$identity" "$app"
codesign --verify --deep --strict "$app"
python3 - "$release" "$version" <<'PY'
import hashlib, json, pathlib, sys
root, version = pathlib.Path(sys.argv[1]), sys.argv[2]
files = sorted(p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file())
entries = []
for path in files:
    data = (root / path).read_bytes()
    entries.append({'path': path, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
manifest = {'schema_version': 1, 'version': version,
            'executable': 'BlocklandReImagined.app/Contents/MacOS/bri-client',
            'content_config': 'BlocklandReImagined.app/Contents/Resources/content/packages.json',
            'files': entries}
(root / 'MANIFEST.json').write_text(json.dumps(manifest, indent=2) + '\n')
PY
(cd "$destination" && ditto -c -k --keepParent "$(basename "$release")" "$(basename "$zip")")
trap - ERR
verify_release "$release"
echo "Created $release ($total_files content files, $total_bytes bytes; Add-Ons: $mods)."
echo "Created $zip ($(stat -f %z "$zip") bytes)."
echo "Verify with: tools/package_mac.sh --verify \"$zip\""
