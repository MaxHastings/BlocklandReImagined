# Windows core-building playtest package

The packaging script creates a versioned folder under ignored `dist/` only
after root supplies the final release executable and SHA-256. It never builds,
launches or tests the visible game. It refuses an existing release directory,
missing selected content and content paths that escape the content root.

The packager reads the package list the client loads: `content/packages.json`
when present, otherwise `crates/package/base-packages.json`
(`docs/architecture/packages.md`). The package receives that list as
`content/packages.json`. It copies only the listed package directories with
all nested files, plus the Add-Ons every release ships turned on (the
Duplicator and those in `tools/shipped-addons.json`, under `content/addons/`,
and with `-StressLab` the Stress Lab under `content/stresslab/`); it excludes research,
community and unintegrated debris content. Inputs with symbolic links or
junctions are rejected.

Package layout:

```text
BlocklandReImagined-<version>-windows/
  bri-client.exe
  bri-import-addon.exe    imports v20 Add-Ons (Start Game > Add-Ons > Import)
  content/
    packages.json
    <the listed native packages, recursively copied>
    addons/               Add-Ons shipped turned on
    stresslab/            with -StressLab only
  PLAYTEST.md
  KNOWN-ISSUES.md
  TESTER-GUIDE.md         install, playing together, what to send, known limits
  FEATURES.md             what is done, partly done and missing
  PLAYTEST-STRESS-LAB.md  with -StressLab only
  Launch.cmd
  Launch-Playtest.ps1
  MANIFEST.json
  logs/                   timestamped stdout/stderr files from each launch
```

Launch with `Launch.cmd`. It sets the working directory to the package
folder, saves separate timestamped logs under `logs/`, and runs the game with
its per-user state folder (`%LOCALAPPDATA%\BlocklandReImagined`; Mac and
Linux use theirs). Settings, saves and saved host certificates therefore
outlive the release folder, and a newer release's folder finds them. Below,
`<state>` is that folder.

LAN hosts answer discovery broadcasts (UDP 28050) with their listing and public
QUIC certificate; the Join Server list shows them, with saved and favourite
servers from `<state>/servers.json`. A direct join needs only the
game port: it trusts the certificate the host presents the first time (or the
key in a `bri://` invite) and saves it in `<state>/trusted-hosts.json`
(trust on first use). Internet hosting needs nothing forwarded by hand when the
router offers UPnP or NAT-PMP (`docs/architecture/hosting.md`). Hosts keep a
persistent certificate and key in `<state>/host-identity.bin`, so saved
trust survives restarts. Do not share `host-identity.bin`.

`MANIFEST.json` contains ordinally sorted relative paths, byte sizes and SHA-256
hashes for the executable, selected content, normalized config and package
instructions/helpers. It excludes itself and mutable `logs/`
files so normal launches do not invalidate package verification. Verify a
completed folder with:

```powershell
  .\tools\package_playtest.ps1 -VerifyPackage .\dist\BlocklandReImagined-<version>-windows
```

Before assembling the release, inspect the actual source selection and estimate
the copy size without writing anything:

```powershell
.\tools\package_playtest.ps1 -ValidateOnly
```

Final assembly requires root's exact executable hash and an explicit package
version. The generated directory and its content remain ignored; only the
packager, launcher/trust helpers, and this layout documentation are source.

## Zip

Beside the folder the packager writes `BlocklandReImagined-windows.zip`: the
folder under its own versioned name, forward-slash entries. It is the one
Windows download, as the Mac and Linux packagers write
`BlocklandReImagined-macos.zip` and `BlocklandReImagined-linux.zip`. The zip
names carry no version, so
`https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-<os>.zip`
always fetches the newest release.

## Release version and signing

A release build carries its version. Set `BRI_VERSION` to the package version
before building, so the main menu, logs, crash reports and the update check
name it (without it a build calls itself `dev-<commit date>` and never checks
for updates):

```powershell
$env:BRI_VERSION = '2026-09-28-a14'
cargo build --release --locked -p bri-client --bin bri-client
.\tools\package_playtest.ps1 -Version 2026-09-28-a14 -ExpectedExecutableSha256 <hash>
```

The packager runs `bri-client.exe --version` and refuses a build whose version
differs from `-Version`. Publish the zipped folder as a GitHub Release whose tag
is that version; players' games compare against the latest release. Pushing a
version tag does all of this on GitHub Actions
([release-builds.md](release-builds.md)).

Signing is optional and off until there is a code-signing certificate. With
one installed in the Windows certificate store, pass its SHA-1 thumbprint:
`-SignCertificateThumbprint <40 hex digits>` signs and timestamps every `.exe`
in the package with `signtool` (Windows SDK) before the manifest is written.
Unsigned packages trigger SmartScreen's "Windows protected your PC";
`PLAYTEST.md` tells players to click More info, then Run anyway.


## macOS app

`tools/package_mac.sh` builds the same release for Apple Silicon Macs, on a
Mac. It carries the content the Windows zip does, chosen by the same rules:
the packs the package list gives a role, every default Add-On from
`packages/default-addons.json` and, with `--stress-lab`, `packages/stresslab`.

```sh
export BRI_VERSION=2026-09-29-a21
cargo build --release --locked -p bri-client -p bri-addon-import
tools/package_mac.sh --version "$BRI_VERSION" \
    --sha256 "$(shasum -a 256 target/release/bri-client | cut -d' ' -f1)"
tools/package_mac.sh --verify dist/BlocklandReImagined-macos.zip
```

```text
BlocklandReImagined-<version>-macos/
  BlocklandReImagined.app/
    Contents/Info.plist
    Contents/MacOS/bri-client, bri-import-addon
    Contents/Resources/content/     packs, addons/, stresslab/, packages.json
  PLAYTEST.md, PLAYTEST-MAC.md, KNOWN-ISSUES.md, TESTER-GUIDE.md, FEATURES.md
  MANIFEST.json                     every other file, size and SHA-256
```

The app is signed ad-hoc (`codesign --sign -`), or with `--sign-identity`
when there is a Developer ID; the manifest is written after signing and the
folder is zipped with `ditto`. Ad-hoc signed apps are not notarized, so
`PLAYTEST-MAC.md` tells players to use Open Anyway the first time.

The game writes to its content folder, and a signed app must not change, so
on macOS the first launch of each build copies `Contents/Resources/content`
to `~/Library/Application Support/BlocklandReImagined/content/<build>` and
plays from there (`mac_bundle` in `crates/client/src/main.rs`). State and
logs live in `~/Library/Application Support/BlocklandReImagined`.
