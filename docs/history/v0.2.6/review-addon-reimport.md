> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-addon-reimport.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: Add-On re-import after a schema change

Branch origin/claude/project-thread-uoh3ap-addon-versions = **f23c1b12** (327ff3ab + f23c1b12 on 1418df5b).

## Verdict: PASS with one small must-fix (the notice). Merge the sha that carries it.

Ran in the cloud on f23c1b12: `bri-weapons --test schema` (1/1), `bri-package` classic unit tests (6/6), `bri-package --test addon_folder_safety` (6/6), `bri-client --lib add_ons::` (9/9). All pass.

### 327ff3ab, plain message ✅
- `Pack::validate` returns typed `OtherSchema { found }`; Display is the player line (older → "reinstall it or get an updated copy", newer → "update the game"). `developer_of` walks the anyhow chain, so context wrapping doesn't lose it.
- `content_identity::read_weapons` eprintln!s the developer line with the manifest path; players never see bootstrap.
- Test checks the shown text has no "bootstrap", the developer line has the command, newer reads "newer version", and an unrelated error gives None. Good.

### f23c1b12, import plan ✅
- `Record.importer` (serde default, skipped when None, so old state files still load). `plan()` treats a record as current only if stamp AND importer match; None counts as stale. Matched/failed records under another key re-import through the same `Step::Import { replaces }` path a changed zip takes. A failed conversion retries under a new game. Good.
- Import writes `importer: Some(reads())` on success and failure. On failure the old conversion is kept (existing behaviour); it then shows the plain "older version" line. Acceptable.

## 🔴 Must-fix (small): the notice says "It starts off" for Add-Ons that stay on
`sync` reports every Import as "Converted X. It starts off." / "Converted N classic Add-Ons: ... They start off." (add_ons.rs ~617-621). A re-conversion keeps the Add-On enabled; the new test itself asserts that notice and then `gun.enabled == true`. This wording bug already existed for changed zips, but this change makes every upgrading player hit it for their whole enabled set at once, and they'll think their Add-Ons were switched off. Fix: count conversions with `replaces` separately and say e.g. "Updated X for this version." (no "starts off"); keep "It starts off" for first conversions. Adjust both tests that assert the old line.

## Answers
**1. Is the client-side schema key sound?** It's sound for v0.2.6: every v0.2.5 record has no key (re-imports), and v0.2.6 test builds wrote schema 3/4, so they differ too. Keeping the key in the client is correct (AGENTS.md: no Torque readers in the runtime graph).
But it isn't complete. The Add-On importer writes many other packs with their own schema numbers (item presentation 2, physics/icons/porting/behaviour 1, …; addon-import/src/lib.rs ~2068-2998, porting.rs), and importer fixes often change output with no schema bump. Either case would leave stale conversions under an unchanged key. **v0.2.7 (before the next release):** key = the release name (`updates::NAME` when `updates::RELEASE`) plus the schemas, so every release re-converts once and every importer fix reaches players. Dev builds keep schemas only, so Max's dev runs don't re-convert on every commit.

**2. Copied old content folder with old bundled Add-Ons?** Acceptable. Those packs ship with the game; copying an old `content` over a new one replaces them. The player now sees "made for an older version of the game; reinstall it", which is the right action. The release notes already say "unzip into a NEW folder".

**3. No real-importer end-to-end?** Not a gap for this change. The change is the plan and the record, which the fake-importer test covers through sync and Library. The real importer's output is covered by its own tests and the release check against bundled and Max's real Add-Ons. One optional release check, worth one run on the PC with a read-only copy: take a v0.2.5 folder's content with Max's converted Add-Ons, run the release build once, and confirm they re-convert, stay on, and load.

## Not blocking (v0.2.7)
- Adopt records are written with `importer: None`, so an adopted conversion is re-imported on the next sync. That's a correct outcome, but Adopt has become a two-step re-import. Either emit Import with `replaces` directly for `(Some(id), None)` or record a key on adopt; pick one.
