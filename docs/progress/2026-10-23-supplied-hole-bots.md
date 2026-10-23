# 2026-10-23 Supplied Bot_Hole files: pursuit, spawn, appearance and reset corrections

Maxwell supplied `E:/Downloads/Bot Add-Ons.zip`. Surdavow clarified the request:
v21 Bot_Hole functionality with BRI's improved AI, without a specific zombie
feature request. His separate `Bot_Zombie_L4B` is not in this archive and is not
qualified here. This work starts from current main `08b8da55786dad355fbf6268a7a715d3a658ef3b`,
not the stale October 16 primary checkout. Another old `zombie-check` worktree
was left untouched. This thread uses `zombie-hole-20261023`.

## Inputs and existing evidence

The outer ZIP SHA-256 is
`0ea42bbb7d321350fec1a14e778567cf55671e8e455668a16500658194d16a4b`.
Its canonical nested inputs are:

| Add-On | Supplied label | SHA-256 |
| --- | --- | --- |
| Bot_Hole | BL v21 r2024 | `500b917ca1f62f71e5a4c43f8c1097478e71d974e9adddb9cd04d88a3aa80220` |
| Bot_Zombie | BL v21 r1865 | `002a9e6da08e9841a29c203908b89d23b0ee11aee6102b009079c995e9782bcf` |
| Bot_Blockhead | BL v21 r1865 | `c5949968b3835dc44b00e8aba4ff40dbc16b9990f0aedd48b194a0fb9948f581` |

These match the originals already used by BRI. v0.3.5's installed native
Hole Bots and Zombie manifests name the first two exact source hashes.
No source archive or original script is committed. The October 2
[receipt](2026-10-02-zombie-hole-bots.md) establishes the earlier native brain
and intrinsic-hole import work; its successful import/function accounting did
not establish full Bot_Hole compatibility. Fresh import still reports 188
unhandled framework behavior entries. That is importer accounting, not a
measurement that 188 distinct features are absent.

## Confirmed corrections

- Normal reconciliation recognized a catalog's intrinsic `bot`, while forced
  respawn required a selected `brick.vehicle`. The actual typed admin reset
  failed with `This brick has no vehicle`. One shared spawn-kind lookup now
  serves reconciliation, forced reset and minigame reset selection, preserving
  explicit spawn choices over the catalog default. The original framework's
  `MiniGameSO::reset` calls `hResetBots`; normal minigame resets must refresh
  hole bots as well as ordinary spawn-brick bots.
- Authored bot/uniform face and decal names were compared exactly against full
  resource paths. The actual Zombie asked for `asciiTerror` but retained a
  random face even with the real base avatar catalog. Dressing now reuses the
  avatar catalog's existing unique short-name validation. Missing/ambiguous
  names retain the previous choice; explicit full identities still work.
- The Zombie port omitted source `decalName = "AAA-None"`, leaving a random
  shirt decal. The port now captures and applies this copy's decal alongside
  its face and colors. Its independent CC0 fixture verifies captured data.

## Spawn preference versus pursuit boundary

Maxwell and Surdavow observed bots abandoning visible enemies near their spawn
boundary. The shared brain erased enemy memory and filtered direct sight once
its feet crossed the home chase radius. This affected brick-spawned bots
regardless of whether the target was still visible; changing search distance
could not fix it. In the supplied Bot_Hole `holes.cs`, direct sight disables
wandering (lines 652–657); home return is inside `if (%wander)` (786, 821).
This is source-backed evidence for an idle home preference, not a source
requirement to clone the original brain or its hidden-target shortcuts.

The bounded correction gives ordinary brick bots the same unbounded pursuit
eligibility as rules bots. Their existing perception, hostility, dated memory,
search exhaustion and reachable routes still govern whom they can pursue.
Idle home/stroll behavior remains. An explicit guard/patrol/follow intent
retains its authored pursuit boundary. This changes the documented semantics
of `chase_radius`: it remains the home allowance for idle interactions and the
explicit follow pursuit bound, rather than an implicit spawn combat wall.
No radius constant was increased, no content name was special-cased and no
wire/content schema changed.

The content-free public-session regression first fails on the old brain:
`bot stopped at its 8-unit home boundary` (`pursuit-before.log`). It requires
actual melee damage beyond each of two home radii, then physical return home
after loss of sight/evidence. A separate native rules test keeps an explicitly
ordered guard near its post, then clears that order and requires a real attack.
An additional private actual-file consumer uses the unmodified source Zombie
kind and places its opponent beyond the previous BRI 48-unit cutoff in the
ordinary hosted map; it crosses that boundary and lands a real bite. The final
actual-file run passes all four scenarios in 17.39 seconds, including natural
death/respawn and two-client observer agreement.

## Reproduction and checks

The ignored local `.research/` retains the original ZIP inputs, actual-file
consumer `supplied_hole_bots.rs`, source-qualified failing logs and corrected
runs. The actual-file consumer is staged temporarily under
`crates/addon-import/tests/`, then removed. It is not an asset-dependent CI gate.
Permanent content-free regressions cover intrinsic-hole admin/minigame resets,
image aliases/ambiguity/missing choices, and decal extraction.

Actual originals go through fresh import, native dependency enabling and
`dedicated::load`, using an invented room and the actual generated base avatar
catalog. Source checks establish health 50, colors, face and blank decal. The
corrected run passes three scenarios in 15.53 seconds: sandbox noncombat then
minigame pursuit/damage; death followed by admin/minigame resets without bot
duplication; two actual QUIC clients observing spawn, late join, appearance
state, advancing reset spawn tick and removal after ordinary Undo Brick.
This is authoritative gameplay/replication evidence, not a pixel comparison or
human feel acceptance. The clients do not render in these checks.

`supplied-real-avatar-before.log` rejects the original face behavior.
`supplied-reset-qualified-negative.log` rejects the old reset function with the
correct typed admin action while the combat control passes. Restoring the
shared spawn-kind correction makes `supplied-corrected.log` pass both cases;
`supplied-network-and-lifecycle-fixed.log` adds observer/removal acceptance.
Earlier floating-brick placement, invented-catalog face checks, raw slash-chat
instead of the typed admin request, synthetic default rocket identity, and a
wrong Undo enum were harness errors. Their logs remain distinct; none is
claimed as a production defect or accepted negative control.

Run the staged consumer with `BRI_BOT_ZIP_DIR` naming canonical `Bot_*.zip`
inputs and `BRI_CONTENT` naming read-only generated base content:

```powershell
cargo test --locked -p bri-addon-import --test supplied_hole_bots -- --ignored --nocapture --test-threads=1
```

The earlier quick tier passed (104 seconds). The first focused run was stopped
when Maxwell reported the pursuit defect, superseding that candidate; it is
not recorded as a final pass. The first post-correction pursuit run reached
and damaged the target but then failed a reused hiding helper whose goal
assertion was specific to an earlier, distant chase. The new case now directly
moves the player out of sight and checks real return movement; the original
knowledge/search tests retain their stricter goal assertions. The corrected pursuit/knowledge target passes all 11 cases, and the explicit
guard test passes. Its first fixture placed the enemy at the minigame spawn,
inside the guard post; moving the enemy after minigame reset and asserting its
actual position qualifies the control. Final focused/integration results are
pending at this checkpoint.

A full default Add-On source bundle rebuild could not find the unrelated exact
pinned Weapon_Package_Tier1 ZIP; three other copies were correctly rejected.
The local preview therefore preserves the published v0.3.5 bundle and original
credits, replacing only Zombie's native package with a freshly imported,
validated package from the same supplied source hash. No source pin was waived.
This is explicitly not a successful full bundle rebuild.

## Remaining implementation versus playtest acceptance

The original framework's own bot wrench UI and event API are not fully ported.
Examples present in the supplied `events.cs` but without native mappings here:
`onBotSpawn`, `onBotDeath`, `onBotActivated`, `onBotReachBrick`, `respawnBot`,
`setBotType`, `setBotPowered`, search/idle/wander controls and bot appearance/
weapon/team commands. These need bounded native compatibility work; an import
or a successful spawn does not make them functional. No TorqueScript VM is
introduced and the improved BRI brain is retained.

Zombie infection ordering/marking/name prefixes and full authored body-part
selection remain incomplete; this change does not close them. The earlier
mixed Zombie/Blockhead Windows firefight crash remains unqualified. Human
playtesting should check spawn placement, arms/face/colors/decal rendering,
navigation and melee feel in an ordinary build, repeated deaths/resets and a
joining friend's view. These checks require a candidate with the new engine
and regenerated Zombie package; published v0.3.5 is unchanged. Missing v21
controls/events and the absent L4B Add-On are implementation/source gaps, not
items Maxwell can accept through playtesting.

## Release authorization and next bounded work

Maxwell subsequently authorized a new release, integration and cleanup, with
bots useful for building varied modes such as a Left 4 Dead-style experience.
Published v0.3.5 stays immutable. The immediate extension is native builder
controls through the existing event editor: independent sight/wander/home
settings, pause/resume, melee damage, and spawn/death lifecycle events. These
will use explicitly named native parameters in world units, rather than
pretend to implement every old Bot_Hole output signature. Existing Blockhead,
Hole Bots, Zombie and Shark packages are the relevant release selection.

The supplied Zombie's collision callback checks half-health eligibility before
calling its parent collision handler (bot_zombie.cs 244–264); current native
melee checks only after damage. This incorrectly converts on the threshold-
crossing bite and can kill a critically injured bot before conversion. A
bounded ordering correction needs an above-threshold damage control, a
critically injured conversion case, and continued human damage/death controls.
The remaining source appearance/marking adaptations stay explicit.

Integration/release will require the final focused tier, complete gate and
matching Windows CI, then three-platform build/archive checks under the
existing release procedure. No interactive playtest automation is authorized.

The first full focused candidate run (989 seconds) finished all five crates'
non-GPU tests with one failure in the newly extended reset fixture: guessed
command numbers were stale after the bot's own actions. The fixture now kills
through a declared native event, after the ordinary spawn shield expires,
without injecting commands into the AI's sequence stream. The qualified
`hole-reset-fixture-qualified.log` passes. This is a harness correction, not a
new gameplay fix or a waived test. The focused tier then passed all six planned steps in 671 seconds
(`focused-qualified.log`), including the corrected fixture and client unit
tests, before the first source-fix commit.
