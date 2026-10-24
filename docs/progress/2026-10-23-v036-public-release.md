# 2026-10-23 v0.3.6 public release

Published [v0.3.6](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.3.6)
at 02:44:17 UTC on October 24 / 9:44:17 PM America/Chicago on October 23,
from `34c508fa3c82c491f1840666159cce822f0c0861`. The public tag and four
asset digests match the verified inputs, and GitHub identifies it as latest.
Published tags and prior release assets were preserved. Subsequent documentation
commits do not change this build's source pin.

Players can pursue enemies beyond idle spawn homes, reset an intrinsic Zombie
Hole without duplicating its bot, and build start/stop or timed replacement
encounters with native wrench events. The menu now shuffles original photos
and native JPEG/PNG screenshots with an approximately five-second cycle and
one-second crossfade. See the [release notes](../releases/v0.3.6.md),
[bot evidence](2026-10-23-supplied-hole-bots.md) and
[menu evidence](2026-10-23-menu-background-slideshow.md).

## Integrated verification

- `python tools/gate.py 34c508fa3c82c491f1840666159cce822f0c0861 --release-content`
  passed in 1,489 seconds: all 542 test binaries, including ignored/content
  checks, passed without retries; strict clippy and startup content passed.
  Release simulation means were 5.51 ms on Crazy Collage Madness (7.00 ms
  budget) and 4.34 ms on TDM Afghan (5.50 ms), each with 32 bots / 3,600 ticks.
- Matching [Windows CI](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40050503152)
  passed all jobs. `python tools/gate.py --push --release-content` integrated
  the exact source; [PR302](https://github.com/MaxHastings/BlocklandReImagined/pull/302)
  was marked merged. Seam availability was then updated on main before tagging.
- [Build run 40050517941](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40050517941)
  passed Windows, macOS and Linux at the exact release source. The tag's
  [publication run](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40055384228)
  published those artifacts. Superseded build-only artifacts were not used.
- Bot quick/focused tiers, seven live builder scenarios and four fresh
  supplied-source consumers passed. Two actual QUIC clients exercised ordinary
  wrench authoring, lifecycle, late join, reset and removal. A production
  `App::load` / tick / GPU-ready / render / UI consumer verified the photo fade
  through final pixels, with 248,224 changing pixels and no missing textures.
  The 23-save corpus also converted and hosted successfully; no input was skipped.

The initial gate rejected a stale Workshop test path for ui-pack-004. The test
now resolves the current registered UI pack (005); that was a test-harness
correction, distinct from the production bot/menu fixes. An initial full run
at the final source passed its tests but exceeded both performance budgets
(10.87 / 6.52 ms). An unchanged isolated run passed (5.09 / 4.26 ms), followed
by the successful full confirmation above. The failed measurements are retained;
their cause is not established. No budget, assertion or failure allowance was
weakened. The confirmation required no flaky-test retry.

## Distribution and remaining human checks

Every ZIP payload was checked for safe unique paths, exact manifest membership,
size and SHA256. Windows has 13,168 payloads; macOS/Linux each have 13,162.
All ship 160 intended public Add-On folders, select 78 packages, and contain
13,144 byte-identical native content files. Refreshed Hole/Zombie/Shark payloads
match the verified bundle. The offline BOT-SPAWNERS guide is included; private
AA/Halo/GMod content is excluded.

| Archive | SHA256 |
| --- | --- |
| Windows | `5d762689098897163b55747b369671d097a723420c6b6531fea2ce244dbf3d3a` |
| macOS | `b6659ac714758b36495b16b472e1ce83d302fda41b34bcb5d07156fa6dfdad86` |
| Linux | `cb33bc7d2f729fb8284de3301621d1620e6d162e841ec5dc61ea20c4cdcac63a` |

The extracted Windows folder is `E:\Downloads\BlocklandReImagined-v0.3.6-windows`.
It reports `v0.3.6 (34c508fa3)`; its startup check reports no Add-On health
problems, and its Bedroom server completed 240 ticks on an ephemeral loopback
port. No visible game window or interactive play session was automated.

Maxwell still checks perceived chase/combat quality, encounter editing, rendered
bot appearance and menu transitions in ordinary play. Full legacy Bot_Hole,
custom L4B, infection markings/team adaptations and the unreproduced mixed-bot
Windows NaN report remain explicit limits. This release does not complete the
alpha or AA2 mission contract.

Detailed source inputs, negative controls, consumers, images, failed/passing
logs and release receipts are retained outside the disposable worktree at
`../BlocklandReImagined-private-evidence/zombie-hole-20261023`. Cleanup is
limited to this lane's finished worktree and build output; the primary checkout,
v0.3.5 folder, source installations and other threads' work are retained.
