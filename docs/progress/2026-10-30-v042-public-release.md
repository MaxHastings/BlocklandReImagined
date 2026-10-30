# 2026-10-30 v0.4.2 public release verification

[v0.4.2 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.2)
was published October 30, 2026 at 04:22:56 AM America/Chicago (09:22:56 UTC),
from `93c1439dacea2c6bce19ca735195bb0f6b944daa`. Maxwell approved the combined
warm playtest and final R19 ambience mix, then explicitly authorized finishing
the importer, merging the complete work and publishing after release checks.
The subsequent finite gate corrections have technical evidence; this receipt
does not claim he interactively played that final corrected executable.

## Integrated source

The release contains snowflake PR403, bots PR404, brick destruction PR405,
the accepted small original-texture water glare change PR406, ambience and
three approved tool swishes PR407, bounded Add-On/map compatibility PR408,
hashed-policy LF portability PR409 and release-gate corrections PR410.
Every complete handoff is an ancestor of the tagged source. The rejected
water overhaul, cancelled Gun/Akimbo policy, vehicle recordings, random SFX
audit and deferred zombie/Roll gameplay ports are excluded.

PR410 reached main at 09:08:43 UTC through
`python tools/gate.py --full --release-content --push`. It reused the genuine
exact-source full pass, checked the 17-commit history/protocol range from
`60fcbe7f4`, and pushed without force with the pre-push hook enabled. The
annotated `v0.4.2` tag resolves to that exact main commit. Published v0.4.1's
tag and assets were not changed.

## Final checks

- Exact-source [CI 41026201525](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41026201525)
  passed all seven required jobs: Windows checks/binaries, Linux wheel and
  four Linux PR test shards. The workflow's pre-existing event-specific
  aggregate job is skipped; no new test skip was added.
- The official full release gate passed in 2,370 seconds on the same source:
  152 Python tool tests, locked workspace/all-target build, warning-free
  workspace/all-target Clippy, release-content startup and 528 Rust test
  binaries with ignored/content cases included. Their actual summaries total
  **5,547 passed, zero failed, zero ignored**, with **no retry** on this final
  run. The 18.3-minute test phase exceeded the tool's ten-minute duration
  warning; it was not omitted or relabeled as a quick pass.
- The actual fixed-save corpus passed **23/23**, including expected refusals,
  in 43 seconds. Neither missing content nor missing saves supplied a skip.
- Both actual saved-world, optimized 32-bot/3,600-tick performance campaigns
  passed unchanged budgets: Crazy Collage Madness averaged **4.978 ms** per
  step against 7.00 ms (p99 10.266 ms); TDM Afghan averaged **3.615 ms** against
  5.50 ms (p99 6.415 ms). These are host-simulation measurements, not gameplay
  FPS or a claim of complete interactive fidelity.
- All three native build/package/startup jobs in
  [build 41026380629](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41026380629)
  passed at the tagged source. The build-only workflow did not publish early.
  Independent ZIP checks hashed every manifest file: Windows **13,458**,
  Apple-silicon macOS **13,452**, Linux **13,452**. Actual executable formats
  are Windows x86-64 PE, macOS ARM64 Mach-O and Linux x86-64 ELF.
- Each archive matches all **3,132** unchanged base snapshot files checked
  outside the separately validated activation configuration, and all **10,175**
  generated Add-On bundle files. Base packs and the complete Add-On payload
  match across platforms. Each executable contains the exact version/source
  stamp; Windows `--version` also reports `v0.4.2 (93c1439da)`.
- The downloaded Windows archive was independently extracted, checked with
  `tools/package_playtest.ps1 -RepoRoot <coordinator> -VerifyPackage <game>`
  under PowerShell 7, then run with `bri-client --check <content> <isolated-state>`.
  It passed with 21 maps, 1,372 brick definitions and 14 validated modern-light
  maps, without a visible window or audio device. It reports **19 compatibility
  entries** and **226 content fallbacks**; passing startup does not remove
  those known unsupported behaviors. The new importer messages expose retained
  duplicate-datablock, unported-script and map-object gaps, rather than claiming
  assets automatically supply custom gameplay. The initial local invocation
  using Windows PowerShell 5 failed while evaluating the script's default
  repository parameter, before package checks. That invocation and the
  successful explicit-root PowerShell 7 run are preserved separately.

The full gate used the existing shared warm target and isolated release
content under its official lock. Its temporary shared-checkout content junction
was restored. Original installations, installed selections and the dirty
historical primary checkout were not used as writable staging.

## Content and source audit

The 19 base recipes and 102 bundled entries were verified current against
the final source. Base content was actually generated at `70662ae53`, verified
at `60fcbe7f4`; the refreshed original Add-On imports and approved audio
overlays were actually generated at `60fcbe7f4`. Later bot/test changes do not
change those producer recipes. Their current-input proof retains that real
generation provenance; no receipt was merely restamped to the release SHA.

All 364 active bundle audio files match the approved preview. The two
user-edited indoor/outdoor bases retain the approved full seam-treated loops
and +12 dB total gain across 20 non-Storm maps, including player-built interiors.
Slate Storm is unchanged; the localized fridge and authored Creek waterfall
and shore voices remain. Original recordings are archived inactive. Pure
tool-swishes preserve original contact cues and stock projectile policy.

The source-seam audit rechecked all 204 rows, 552 named expressions and 13
manual owner-path cases against current files, retaining explicit qualified/
serialized-name mappings. Sprint-specific input and in-game terrain editing
remain absent. Source availability is separate from tests and human acceptance.
`python tools/check_docs.py` passed the 76 maintained documents and six
generated release-guide layouts before publication.

## Failures retained and corrected

The earlier `60fcbe7f4` full run had seven actual failures and a compiler-cache
socket failure. The later `f62e52cee` full run retained a persistent vehicle TDM
fight-floor failure plus a parallel volume-hook fixture failure that passed
alone. Those failed runs are not publication evidence. Final corrections
preserve assertions, seeds, layouts, corpus and performance budgets:

- Objective replanning now runs while the failed approach remains excluded.
  Delayed explosion risk follows actual predicted detonation timing, while
  retaining conservative own/ally harm checks and safe foot landings.
- The lower landing-waypoint block excludes seated passengers/gunners. Source
  evidence showed their placement sets `grounded` while they have no driving
  state; treating them as riders standing on a roof caused the TDM regression.
  The original fight/crew/gunner requirements now pass unchanged.
- The glass scenario retains all original freeplays/replays. A separate
  native-session setup supplies an actual opposing sight opportunity through
  the same authored glass, with an opaque control, instead of assuming a seed
  necessarily supplies one. The exploratory runtime perception observer is
  excluded from both final code and ancestry.
- Parallel volume tests now own independent `TempDir` catalogs. Their former
  shared PID directory let one test delete the other's package files. Event,
  lifetime and region assertions remain unchanged.
- Release-content audio tests use the real decoder and every weighted native
  variant, recognize the approved Fire-state swish, and accept the standard
  release content environment. Original impact/death contracts are preserved.

The earlier raw CRLF policy-hash mismatch was corrected by LF policy for hashed
JSON. Earlier native BadZip transport failures and a separate successful
same-helper download remain recorded; no missing-response diagnosis was
invented. Superseded builds were intentionally cancelled, not counted as
passing final packages. The existing ragdoll retry flake remains assigned to
[issue 400](https://github.com/MaxHastings/BlocklandReImagined/issues/400);
it passed without retry in this final run. A synthetic avatar-portrait retry
line from a Python gate unit-test mock is not a product flake.

## Public verification and cleanup

[Publish run 41032882027](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41032882027)
succeeded using the already tested build-only archives. All three ZIPs and
SHA256SUMS were downloaded again **without authentication**. Their hashes
match the tested artifacts and the public SHA256SUMS exactly. The public release
is neither draft nor prerelease, the unauthenticated latest endpoint returns
v0.4.2, and the remote annotated tag peels to the exact tested source.

| Platform | Bytes | SHA256 |
|---|---:|---|
| Windows | 258,140,030 | `ef5d2e5f94958823ca9895b7aedb8aa900facb3a424dc1c2bb91b7d3540b6844` |
| macOS | 227,633,666 | `1240edeaec6065def6203f678382b19006887e73ebe94e9e4b77756e839b650e` |
| Linux | 255,323,693 | `81b76a1a6d61ad95c1cdae14227087ee956f08d603df989fbc302cafdf12adec` |

Durable source/media, accepted and rejected captures, actual original inputs,
failed/final gate logs, recipes, platform evidence and public verification are
preserved under `E:/Downloads/BlocklandReImagined-Release-Evidence/v0.4.2`.
Finished-checkout retirement verifies clean tracked work, remote-main ancestry,
per-file archive hashes, confined absolute paths, build locks, recent writes
and active processes. Junctions are recorded and unlinked without traversal.
The approved playtest, Ableton review folder, original installations, dirty
primary checkout/main target, shared warm gate cache and unrelated/stopped SFX
work are protected. The final cleanup journal and checkpoint in that evidence
folder record the actual removed checkouts and reclaimed output.

Full alpha and remaining source-fidelity/unsupported-script work stay open.
The bounded Map_IZB import proof does not establish complete custom gameplay,
terrain detail, authored lighting or visual fidelity for every legacy map.
