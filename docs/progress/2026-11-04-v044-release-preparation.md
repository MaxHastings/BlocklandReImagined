# 2026-11-04 v0.4.4 release preparation

Maxwell requested preparation so v0.4.4 can be cut after he approves the
playtest. He confirmed the PC is available for checks now. Publication, tags
and the held destruction merge remain conditional on that approval.

The preparation branch is `codex/v044-release-prep`, based on integrated engine
candidate `93a31023e2400a70d39725ec51c6444e39f4fe3c`. That source passed all seven
executed CI checks, with the conditional Windows job skipped. The main checkout
is still on the jeep race branch with three unrelated tracked changes; it is
untouched. The default `tools/release.py` flow cannot use that dirty checkout.
Preparation instead uses isolated content and a dedicated gate under the
required worktrees directory, leaving the existing work and previews intact.

The public latest-release endpoint and four asset names were checked: v0.4.3,
published November 1 at 09:45:42 UTC, source `d2643c62b`. STATUS previously
still named v0.4.2; it now distinguishes the published release from the pending
v0.4.4 candidate. The release notes include clipboard/search corrections as
well as destruction, navigation and generic native vehicle presentation. The
Speedkart give-back remains a separate Add-On, without a default-bundle change.

Private preparation evidence is under
`E:\Downloads\BlocklandReImagined-Release-Evidence\v044-release-prep-2026-11-04`.
The selected 86 directories comprise 19 base packs and 67 enabled default
Add-On/companion entries. They were copied into isolated content; no undeclared
private content or raw source archives enter the release payload. The final
Add-On source inventory recovers all 100 pinned originals byte-for-byte from
existing local copies. No pin, author attribution or edition was substituted.
Missing copies caused early attempts to stop, and their logs are retained.

All 100 originals have current-importer generation receipts. NPC generation
produced 118 recordings. Ambience generation initially lacked its immutable
v0.4.2 baseline cache; four exact OGG inputs were recovered from the retained
v0.4.2 package, and the approved outdoor WAV was copied from Maxwell's Downloads.
Every input matches the existing registry hash. The normal ambience builder
then generated four clips for 21 maps, without a source/mix/approval change.
The validated native packages and current credits were staged and assembled
with the standard `addon_bundle.py from-release` layout reader; this is staged
current output, not reuse of prior original imports. `addon_bundle.py sources`
passes with `previous_imports: false`. `pinned-originals.json`,
`ambience-inputs.json`, `staged-native-inputs.json` and the build logs retain
the exact inputs, generation/assembly scope and intermediate failures.

The source-backed engine seam recheck covers 213 rows. Script registration,
source-path and serde-generated-spelling exceptions were checked explicitly;
source availability does not imply complete consumers or human acceptance.

The current 120-second cube replay failed the unchanged pursuit assertion:
no zombie killed the stationary host. One reached the host's floor at tick
9311. The historical successful v0.4.3 probe used an explicit host position
`[90.3241,294.6100,98.5718]`; the current saved spawn is
`[89.25,294.6,98.25]`. Those are different encounter inputs. Compare the exact
historical probe before claiming a regression or reducing the requirement.
The failing log and all-tick trace are retained. The shorter AFK trace never
established success, and no assertion has been weakened or skipped to hide it.
An initial historical CLI control used the default disabled Bot Add-Ons and
set three spawn bricks aside; that control is invalid. The corrected controls
explicitly enable Hole/Zombie as ordinary package choices, load all five bricks
and use current validated content. Matched v0.4.3 source comparison remains
pending before attributing any observed difference to a regression.

Pending: integrated content/startup and matched cube-baseline results,
exact-source full release gate including retained save corpus/performance,
build-only native release artifacts and archive/startup verification. No tag,
public release or destructive cleanup is authorized by preparation alone.
