# Windows core-building playtest package

The packaging script creates a versioned folder under ignored `dist/` only
after root supplies the final release executable and SHA-256. It never builds,
launches or tests the visible game. It refuses an existing release directory,
missing selected content and content paths that escape the content root.

The packager reads the package list the client loads: `content/packages.json`
when present, otherwise `crates/package/base-packages.json`
(`docs/architecture/packages.md`). The package receives that list as
`content/packages.json`. It copies only those 14
selected package directories with all nested files; it excludes research,
community and unintegrated debris content. Inputs with symbolic links or
junctions are rejected.

Package layout:

```text
BlocklandReImagined-alpha-<version>/
  bri-client.exe
  content/
    packages.json
    <15 selected native packages, recursively copied>
  PLAYTEST.md
  KNOWN-ISSUES.md
  Launch.cmd
  Launch-Playtest.ps1
  MANIFEST.json
  user-state/             created on first launch; never copied into a release
  logs/                   timestamped stdout/stderr files from each launch
```

Launch with `Launch.cmd`. It sets the working directory to the package
folder, uses package-local `user-state/`, and saves separate timestamped logs
under `logs/`. State and saved host certificates therefore remain beside the
playtest build.

LAN hosts answer discovery broadcasts (UDP 28050) with their listing and public
QUIC certificate; the Join Server list shows them, with saved and favourite
servers from `user-state/servers.json`. A direct join needs only the
game port: it trusts the certificate the host presents the first time (or the
key in a `bri://` invite) and saves it in `user-state/trusted-hosts.json`
(trust on first use). Internet hosting needs nothing forwarded by hand when the
router offers UPnP or NAT-PMP (`docs/architecture/hosting.md`). Hosts keep a persistent certificate in `user-state/`
(`host-certificate.der`, `host-key.der`), so saved trust survives restarts. Do
not share `host-key.der`.

`MANIFEST.json` contains ordinally sorted relative paths, byte sizes and SHA-256
hashes for the executable, selected content, normalized config and package
instructions/helpers. It excludes itself and mutable `logs/` and `user-state/`
files so normal launches do not invalidate package verification. Verify a
completed folder with:

```powershell
  .\tools\package_playtest.ps1 -VerifyPackage .\dist\BlocklandReImagined-building-playtest-<version>
```

Before assembling the release, inspect the actual source selection and estimate
the copy size without writing anything:

```powershell
.\tools\package_playtest.ps1 -ValidateOnly
```

Final assembly requires root's exact executable hash and an explicit package
version. The generated directory and its content remain ignored; only the
packager, launcher/trust helpers, and this layout documentation are source.

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
is that version; players' games compare against the latest release.

Signing is optional and off until there is a code-signing certificate. With
one installed in the Windows certificate store, pass its SHA-1 thumbprint:
`-SignCertificateThumbprint <40 hex digits>` signs and timestamps every `.exe`
in the package with `signtool` (Windows SDK) before the manifest is written.
Unsigned packages trigger SmartScreen's "Windows protected your PC";
`PLAYTEST.md` tells players to click More info, then Run anyway.

