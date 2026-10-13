# Release builds on GitHub

`.github/workflows/release.yml` builds the Windows, macOS and Linux releases
side by side on GitHub runners and publishes all three as one GitHub Release. Players download
`BlocklandReImagined-windows.zip` from the
[Releases page](https://github.com/MaxHastings/BlocklandReImagined/releases),
and the game's update check (`crates/client/src/updates.rs`) reads the latest
release, so once a release is out, older builds say a newer one exists.

The workflow runs the same recipe as a release built by hand
([playtest-package-layout.md](playtest-package-layout.md)):

1. Fetch the generated v20 content and the Add-On bundle (below). It stops
   here with a clear error when either was never uploaded or lacks a pack or
   an original this commit needs.
2. `cargo build --release --locked` of `bri-client`, `bri-import-addon` and
   `bri-server`, with `BRI_VERSION` set to the version.
3. `bri-client --check` against the content.
4. `tools/package_playtest.ps1 -Version <v> -ExpectedExecutableSha256 <hash>`
   (releases leave the Stress Lab test Add-Ons out), writing
   `dist/BlocklandReImagined-windows.zip`.
5. The zip smoke: unzip it, check it holds exactly the versioned release
   folder, `-VerifyPackage` that folder and run its `bri-client --check`.
6. Publish a release tagged with the version, carrying
   `BlocklandReImagined-windows.zip`, `BlocklandReImagined-macos.zip`,
   `BlocklandReImagined-linux.zip` and `SHA256SUMS` (`tools/publish_release.py`,
   which first checks every zip names the version and that the base content
   packs are byte-identical on all three). The Mac and Linux builds
   (`mac-release-asset.yml`, `linux-release-asset.yml`) fetch the same content
   and Add-On bundle as Windows, so they run alongside it. The
   names carry no version, so
   `releases/latest/download/BlocklandReImagined-<os>.zip` always fetches the
   newest; the folder inside is `BlocklandReImagined-<v>-<os>`.
   The `.pdb` debug symbols are kept as a workflow artifact
   (`...-symbols`, 90 days), not shipped to players.

The loopback-join release smoke needs an original v20 Add-On archive and a GPU,
so it stays on the PC. Tag a commit that passed `tools/gate.py`, which already
ran every content test on it.

### Old saves before a release

Releases no longer load a random sample of Maxwell's saves by hand. A fixed
corpus of 23 known-tricky `.bls` saves (`crates/client/tests/save-corpus.json`:
relative path, reason, expected bricks placed or expected refusal) is hosted
the way the game hosts a dropped save. The gate runs it on its own whenever a
change touches a path in `SAVE_CORPUS_PATHS` (`tools/gate.py`): saving,
loading, the `.bls` reader and converter, brick and print data. About 100 s on
the PC. The saves themselves are Maxwell's and never enter the repository.

Before tagging, if the gate did not run it (no save paths changed since the
last release), run it once by hand in the main checkout:

```powershell
cargo test -p bri-client --test save_corpus -- --ignored --nocapture
```

It reads `BRI_SAVES` (default `%LOCALAPPDATA%\BlocklandReImagined\saves`) and
`BRI_CONTENT` (default `content/`) and passes with a "skipped:" line when
either is missing. For a sweep of every save, `saves_host_probe <content>
<saves-dir> <report.json>` still hosts a whole folder (about 40 minutes for
700 saves).

## One-time setup (Max, on the PC)

The content is generated from the v20 install and is never committed. The
workflow gets it from a **draft** release of this repository named
`ci-content`. Drafts are visible only to people who can push and to the
workflow itself, so it needs no secret or second repository. It is also marked
a prerelease, so even if it were published by mistake the update check would
ignore it.

1. Install the GitHub CLI and sign in, once:
   ```powershell
   winget install GitHub.cli
   gh auth login
   ```
2. In the main checkout (whose `content/` is current, as for any release):
   ```powershell
   python tools/ci_content.py upload
   ```
   It zips the packs the game loads into `dist/ci-content.zip` (it
   refuses anything over GitHub's 2 GiB asset limit), creates the draft
   release if needed and replaces its zip.

Without `gh`, `python tools/ci_content.py pack` writes the same zip; then on
GitHub, Releases, Draft a new release, tag `ci-content` (don't create the tag),
tick "Set as a pre-release", attach `dist/ci-content.zip` and click **Save
draft**, never Publish.

Upload again whenever the content changes: after a bootstrap run that rebuilt
packs, or when `crates/package/base-packages.json` names a new pack. The
default Add-Ons never come from this zip: our own are committed under
`packages/`, and the bundled originals have their own draft release (next
section). A release
run built with content older than its commit fails at step 1 and says so.

## Bundled original Add-Ons

Releases carry classic community Add-Ons (the Duplicator, the Stunt Plane,
the Sniper Rifle and others) as their authors' originals, credited to them,
with our ports adding the behaviour their scripts had. The public repository
never holds their files or anything converted from them. It holds only
`packages/default-addons.json`: per original, its Add-On name, title,
credited authors, version, on or off at start, and the sha256 of each copy
pinned for bundling. Its port is the folder whose
`crates/addon-import/ports/<port>/entry.json` names it.

`tools/addon_bundle.py` does the rest on the PC:

```powershell
python tools/addon_bundle.py find      # where each original is, its sha256, whether its port applies
python tools/addon_bundle.py upload    # build the bundle, then put it on the addon-bundle draft release
```

`build` (which `upload` runs) searches Steam's
`S:\SteamLibrary\steamapps\common\Blockland\Add-Ons`, the v20 install's
`Add-Ons`, Maxwell's archive and any `--search` folder; imports the pinned
copy of each original with `bri-import-addon` against the v20 install
and, as a player's Import does, the game's generated content
(`--installed`; `--content-root`, default `content/`);
refuses one whose listed port does not apply; writes the credited authors
into its `package.json` (the Add-Ons screen shows them); and packs
`addons/<id>/`, `bundle.json` and `CREDITS.md` into `dist/addon-bundle.zip`.
A port with host rules (the Hookshot's pull, Trench's digging) writes them
beside the import at `addons/<id>-rules/`, as Import does for a player, and
names them in the import's `companions`. Every step carries them with the
original (`sources`, `install`, `fetch`, `from-release`), the packagers turn
them on right after an original that starts on, and `verify-release`
refuses a release without them. A bundle built before they shipped (schema
1) is refused: rebuild and upload it.
`upload` refuses while an original has no pinned copy, since the release
would lack it (`--allow-unpinned` releases without it).

The draft release is `addon-bundle`, like `ci-content`: visible only to people
who can push and to the workflows. The Windows release workflow runs
`python tools/addon_bundle.py fetch`, and its packager copies the originals
into `content/addons/<id>` and `CREDITS.md` beside the docs. The Mac and
Linux workflows fetch the same draft (and the same `ci-content`), so every
platform ships the same credited originals whenever the draft was last
uploaded; `tools/publish_release.py` refuses a release whose three zips carry
different base content. (`addon_bundle.py from-release` still takes the
bundle back out of an unpacked release.)

- **Pin an original:** run `find`, check the copy it names is the one to
  credit, and add its sha256 to the entry's `sha256`.
- **Pull one** an author objected to: add `"withdrawn": "<why>"` to its
  entry, upload, and make a release. The game, the bundle and every packager
  leave it out.
- **A new original:** add an entry (its `id` is the importer's name for it,
  `Weapon_Example` becoming `weapon_example`) and its port.

## The DirectX Shader Compiler (Windows)

The Windows release carries `dxcompiler.dll` from Microsoft's open-source
DirectX Shader Compiler beside `bri-client.exe`, with its LLVM and MIT
licences under `licenses/DirectXShaderCompiler/`. wgpu's default compiler
choice loads it, and the world's shaders compile in about 3 s instead of
about 20 s with the system's FXC (minutes on a busy CPU). The session log
says which one the game used. Without the file the game still runs, on FXC.
`dxil.dll`, under Microsoft's own licence terms, does not ship: DXC hashes
shaders itself without it.

`tools/shader-compiler.json` pins the release by URL and sha256, and each
shipped file by sha256. `tools/package_playtest.ps1` downloads it into
`dist/shader-compiler/` when missing (or takes `-ShaderCompilerArchive`),
refuses anything that is not the pinned file, and `-VerifyPackage` refuses a
release without them. To move to a newer DXC, change the pin and check the
game on the PC.

## Making a release

From a gate-ready branch on origin (it contains origin/main), one command on
the PC does everything, and the next release needs one go-ahead from Max:

```powershell
python tools/release.py v0.2.11 --branch claude/v0211-integration-ab12cd
```

1. **Content refresh first.** The main checkout checks out the branch head
   (detached) and runs `tools/bootstrap.py`, which rebuilds only packs whose
   importer inputs changed; `ci_content.py upload` runs when a pack changed,
   `addon_bundle.py upload` when the content, `packages/default-addons.json`,
   the importer or a port changed since the previous `v*` tag. The gate then
   tests the content that ships. (`--skip-refresh` when it already matches,
   `--upload` to upload regardless.)
2. **Release builds start**: a build-only `release.yml` run at the branch,
   Windows, macOS and Linux in parallel, while the gate runs.
3. **The gate** (`tools/gate.py`) on that commit. It goes first: any queued
   perf or benchmark runs on the PC wait until it finishes.
4. **Push and tag.** The exact gated commit is pushed to main (the push hook
   reuses the gate's pass, so it takes seconds) and tagged with the version.
   There is no merge pull request, so nothing waits on the GitHub Windows
   check, which still runs on main afterwards as the record. An open pull
   request for the branch shows as merged.
5. **Publish.** The tag starts `publish-release.yml`, which waits for the
   build-only run if it is still going, takes its three zips and publishes
   them with `docs/releases/<version>.md` (when the commit has it) above the
   download instructions. With no build at that commit it starts a
   publishing `release.yml` run at the tag instead.

It stops at the first failure and says what failed; nothing is pushed until
the gate and the builds pass. `--check-only` does steps 1 to 3 and publishes
nothing. Each step prints its minutes. The binaries carry the tagged commit's
hash, because the tag names the commit they were built from.

Other ways in, for a release built without the PC:

- Push a date tag (`git tag 2026-10-02-a19 origin/main` then
  `git push origin 2026-10-02-a19`): `release.yml` builds and publishes all
  three platforms.
- Push a `v*` tag yourself: `publish-release.yml` publishes a build-only run
  at that commit if there is one, otherwise builds.
- Actions, **release**, **Run workflow**, with a version, and publish on or
  off. Off keeps the three zips on the run for 30 days.

The workflows refuse a version that already has a published release. A draft
left by a failed publish is reused.

Re-attaching one platform to an existing release: run `mac-release-asset.yml`
or `linux-release-asset.yml` with the release tag, the release commit and
publish on. They never replace an asset.
