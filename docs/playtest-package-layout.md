# Windows core-building playtest package

The packaging script creates a versioned folder under ignored `dist/` only
after root supplies the final release executable and SHA-256. It never builds,
launches or tests the visible game. It refuses an existing release directory,
missing selected content and content paths that escape the content root.

The packager parses `ContentConfig::default` from
`crates/client/src/content.rs`, then applies the same optional
`content/client-content.json` override that `ClientContent::load` honors. The
package receives a normalized `content/client-content.json` containing every
effective package selection and the terrain region. It copies only those 14
selected package directories with all nested files; it excludes research,
community and unintegrated debris content. Inputs with symbolic links or
junctions are rejected.

Package layout:

```text
BlocklandReImagined-alpha-<version>/
  bri-client.exe
  content/
    client-content.json
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
QUIC certificate; the Join Server list shows them. A direct-IP join asks the
address once for its certificate and saves it in `user-state/trusted-hosts.json`
(trust on first use). Hosts keep a persistent certificate in `user-state/`
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
