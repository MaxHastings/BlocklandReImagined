# Release builds on GitHub

`.github/workflows/release.yml` builds the Windows release on a GitHub runner
and publishes it as a GitHub Release. Players download the standalone
`BlocklandReImagined.exe` (or the zip) from the
[Releases page](https://github.com/MaxHastings/BlocklandReImagined/releases),
and the game's update check (`crates/client/src/updates.rs`) reads the latest
release, so once a release is out, older builds say a newer one exists.

The workflow runs the same recipe as a release built by hand
([playtest-package-layout.md](playtest-package-layout.md)):

1. Fetch the generated v20 content and the Add-On bundle (below). It stops
   here with a clear error when either was never uploaded or lacks a pack or
   an original this commit needs.
2. `cargo build --release --locked` of `bri-client`, `bri-import-addon` and
   `bri-launcher`, with `BRI_VERSION` set to the version.
3. `bri-client --check` against the content.
4. `tools/package_playtest.ps1 -Version <v> -ExpectedExecutableSha256 <hash>`
   (releases leave the Stress Lab test Add-Ons out), then `-VerifyPackage` (the packager also verifies the
   standalone exe it writes).
5. The standalone smoke, `release_smoke`'s
   `standalone_exe_unpacks_per_user_and_starts_the_game`, on the packaged exe.
6. Publish a release tagged with the version, carrying
   `BlocklandReImagined.exe` and `BlocklandReImagined-<v>-windows.zip`.
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
pinned for bundling. Its port is the one
`crates/addon-import/ports/ports.json` lists for that name.

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
Linux workflows already take the base game from that release's Windows zip
(checked against its `MANIFEST.json`); they take the originals and
`CREDITS.md` from it too (`python tools/addon_bundle.py from-release`), so
every platform ships the same credited originals whenever the draft was
last uploaded.

- **Pin an original:** run `find`, check the copy it names is the one to
  credit, and add its sha256 to the entry's `sha256`.
- **Pull one** an author objected to: add `"withdrawn": "<why>"` to its
  entry, upload, and make a release. The game, the bundle and every packager
  leave it out.
- **A new original:** add an entry (its `id` is the importer's name for it,
  `Weapon_Example` becoming `weapon_example`) and its port.

## Making a release

Either push a tag named by the version:

```powershell
git tag 2026-10-02-a19 origin/main
git push origin 2026-10-02-a19
```

or open Actions, **release**, **Run workflow**, and type the version. That form
can also build without publishing (the files are then kept on the run for 30
days) and leave the Stress Lab out.

Tags that start with a date (`YYYY-MM-DD-...`) start the workflow. The version
becomes the release's tag, the dist folder name and what the main menu shows,
exactly as `BRI_VERSION` does for a hand-made build. The workflow refuses a
version that already has a release.
