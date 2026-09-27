### 2026-09-26 — native vanilla minigame rules (isolated agent handoff)

Added standalone `bri-minigames`: typed owner/member lifecycle and invitations,
all vanilla settings, explicit Internet/legacy-LAN damage/use policy, stable account
plus session/round/life identity, source scoring and unlimited lives, manual timed
respawn, native player/loadout/build/paint/sports effects, owner-brick MiniGame event
authorization, reset/respawn-all and messages, deterministic spawn/timer helpers,
bounded native presets/world persistence. No legacy runtime dependency.

Read-only converter produced minigames-pack002: 11 selectable PlayerData,21 ItemData,
zero diagnostics, original/recovered hashes. User-facing GUI defaults intentionally
supersede constructor's transitional1 ms timers/damage-off fields. No source lives
limit found; root confirmed unlimited baseline. Vehicle player types remain
source-selectable. Wheeled vehicle respawn = max(source game delay,burnTime)+100ms;
outside base vehicle delay0. See docs/research/minigames for line evidence/decisions.

20 tests and Clippy -D warnings pass; full-catalog release smoke: eight sessions,
two games,12,000 ticks,671,200 damage checks,400 effects, native snapshot roundtrip.
Rules-loop initial33.8352ms on Ryzen7 7800X3D, rustc1.93.1; excludes host/physics/net.
Root must add workspace/dependencies and bind effects/queries to Simulation,
players,weapons,vehicles,bricks/events,transport/late join and familiar minigame UI.
No visible game/input/audio; originals unchanged; no alpha checkbox completed.
