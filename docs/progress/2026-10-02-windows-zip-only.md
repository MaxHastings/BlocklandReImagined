# 2026-10-02 Windows ships as just a zip

Max asked whether Windows needs both an exe and a zip, and decided on just
the zip, like Mac and Linux.

What changed:

- `crates/launcher` (the self-unpacking `BlocklandReImagined.exe`) is gone,
  with its packager code (`-NoStandalone`, `-VerifyStandalone`, the
  `BRISFX01` footer), its release smoke and its workflow steps. Nothing else
  used it: the update check only opens the release page.
- The three downloads keep one name across versions:
  `BlocklandReImagined-windows.zip`, `-macos.zip`, `-linux.zip`, so
  `releases/latest/download/BlocklandReImagined-<os>.zip` always fetches the
  newest. The folder inside still carries the version.
- `Launch.cmd` and `launch.sh` no longer pass a package-local `user-state`
  folder: the game uses its per-user state folder
  (`%LOCALAPPDATA%\BlocklandReImagined`, as the Mac app already did), so a
  newer release's folder finds the same settings, saves and identity.
- `release.yml` replaces the exe smoke with a zip smoke: unzip, check the one
  versioned folder, `-VerifyPackage` it and run its `bri-client --check`.
- README, TESTER-GUIDE, release-builds, playtest-package-layout,
  native-client, DEDICATED-SERVER, modding README and the Stress Lab page
  describe the zip-only install.

Known gap: dropped and imported Add-Ons still live in the release folder's
`content`, so an update means copying `content\Add-Ons` and `content\addons`
across (TESTER-GUIDE says so). The exe used to carry them over. Moving
player Add-Ons into the state folder is the follow-up.

Checks: `cargo check --workspace --tests`; workflow YAML parses; `bash -n`
on the shell scripts. The PowerShell packaging tests run on the PC gate.
