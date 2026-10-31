# Bot_Hole event crosswalk: current gap

Read `events.cs` directly from the supplied read-only `E:/Downloads/Bot
Add-Ons.zip`, nested `Bot_Hole (BL v21 r2024).zip` (the approved plan records
its SHA-256). The native catalog is `crates/events/src/bots.rs`; execution is
`crates/sim/src/session/bots/controls.rs`. This is a source/catalog crosswalk,
not a claim that imported rows already execute.

| Source registration | Current native assessment |
| --- | --- |
| Brick `setBotPowered(bool)`, `respawnBot`; inputs `onBotSpawn`, `onBotDeath` | Matching native brick controls/hooks exist; preserve lifecycle and permission tests. |
| Bot `SetBotPowered(Off/On/Reset)` | Not the same contract as the native brick boolean: source Reset calls a distinct loop reset. |
| Bot `SetSearchRadius(mode, radius)` | Native brick `setBotSightRadius` is a bounded float, not a compatible alias. Source `AlwaysFindPlayer` explicitly changes search behavior; the requested eligible-player tracking policy remains unimplemented. |
| Bot `SetWanderDistance(mode, radius)` | Native brick `setBotWanderRadius` and `setBotReturnHome` offer related but different controls. Source Off, StayAtSpawn and infinite-distance modes cannot be represented by a radius alone. |
| Brick `setBotType`; Bot `SetRunSpeed`, `LookAtPlayer`, `LookAtBrick`, `GoToBrick`, `SetIdleBehavior`, `SetActivateDirection` | No matching native bot operations in the current bot catalog; inspect existing general mechanisms before implementing. |
| Bot `SetBotName`, `SetTeam`, `SetMeleeDamage` | Native `setBotMeleeDamage` is a brick-target float with different source bounds. Source `SetTeam` sets Bot_Hole relationship type (Enemy, Neutral, Friendly, Mercenary, Owner, Custom), not a numbered minigame team. Name and relationship controls remain open. |
| Bot `SetWeapon`, `DropItem`, `PlayGesture`, `SetRandomAppearance`, `SetAppearance` | No corresponding native bot operations in this catalog; assess existing item, gesture and appearance seams and explicit unsupported cases. |
| Brick inputs `onBotActivated`, `onBotReachBrick` | Not registered in the native bot catalog. |

Original source registrations target `Bot`, while this native catalog's outputs
target the spawner brick and its spawn/death input currently exposes `Bot` as a
Player slot. Preserve those target and parameter distinctions; do not wire menu
entries to no-op handlers or treat a same-looking name as semantic compatibility.
The source also contains commented-out registrations, which are not counted as
available events. User-requested selected-player and continually-selected-enemy
tracking need separate authorization, lifecycle and no-through-wall-fire tests.
