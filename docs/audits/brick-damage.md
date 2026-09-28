# Brick damage audit against v20

Date: 2026-09-28. Branch `claude/brick-damage`, from main `1932015` (protocol
39). No wire change, so no protocol bump.

Scope: which weapons, explosions, tools and events damage bricks, inside and
outside minigames, and whether the brick comes back. That covers
`ProjectileData::onExplode` and `onCollision`, `miniGameCanDamage`, the
minigame Brick Damage and Brick Respawn Time settings, `fakeKillBrick`, and
the hammer, wand, admin wand and undo.

Max was asleep; every choice below follows v20 and is listed at the end.

## Sources

Recovered v20 scripts (`.research/v20-dso`, not committed), line numbers from
`server/scripts/allGameScripts-Vanilla.cs` unless noted:

- `ProjectileData::OnCollision` (8014): direct hits on a brick when the
  projectile has `brickExplosionImpact`.
- `ProjectileData::onExplode` (8113): radius brick damage.
- `miniGameCanDamage` (22815).
- `fxDTSBrick::fakeKillBrick` (17471), `fxDTSBrick::Respawn` (17490),
  `onBlownUp` (17253).
- `hammerImage::onHitObject` (10536), `WandImage::onHitObject` (12120),
  `AdminWandImage::onHitObject` (12921), `ServerCmdUndoBrick` (5689).
- `getTrustLevel`: always `$TrustLevel::You` under `$Server::LAN`.
- `server/mainServer.cs` 631-648: Single Player and LAN set
  `$Server::LAN = 1`; Internet sets 0.
- Respawn times: `$Pref::Server::BrickRespawnTime = 30000` in the reference
  install's `base/server/defaults.cs`; minigame `BRT` clamps to
  `$Game::MinBrickRespawnTime` 2000 and `$Game::MaxBrickRespawnTime` 300000
  (2844, 21812); Create Mini-Game defaults Brick Damage on and Brick Respawn
  Time 30 s (`client/scripts/allClientScripts-Vanilla.cs` 1427).

## v20's rules

**Weapon blasts and direct hits fake-kill bricks.** They never delete a brick.
`transmitBrickExplosion` hides it, throws debris, and brings it back after the
respawn time. The respawn time is the shooter's minigame's `brickRespawnTime`,
or outside minigames `$Pref::Server::BrickRespawnTime` clamped to 1-60 s
(30 s by default).

Before any rule, the brick must pass the engine's
`canExplode(maxVolume, maxFloatingVolume)` and the shooter must have a client.
Inside a minigame, nothing happens for 3 s after the shooter's F8 drop
(`lastF8Time`). Then, per brick:

| Server | Shooter in a minigame | Which bricks break |
|---|---|---|
| Single player or LAN | no | Anyone's |
| Single player or LAN | yes | Anyone's, if that minigame has Brick Damage on |
| Internet | no | The shooter's own (or the source object's brick group) |
| Internet | yes | `miniGameCanDamage == 1`: same minigame, Brick Damage on, and the minigame owner's bricks (or anyone's with Use All Players' Bricks) |

Every broken brick fires `onBlownUp`.

**Tools delete bricks for real.** The hammer, wand, admin wand and undo call
`killBrick`. None of them asks about Brick Damage or respawn:

- Hammer: skips bricks that would strand others (`willCauseChainKill`), then
  needs Full trust (always granted on LAN), and fires `onToolBreak`.
- Wand: needs "You" trust; the minigame's Enable Wand gates holding it.
- Admin wand: admins only, no trust check.
- Undo of a plant: the planter's own brick; a brick holding others up needs
  Full trust from every group it touches.

**`fakeKillBrick` is an event output.** It ignores minigames and Brick
Damage. Debris is thrown with force `VectorLen(vector) * 2` from the brick
centre minus the normalized vector, and the brick returns after
`mClamp(time, 0, 300)` seconds. `Respawn` brings it back at once.

## Ours against v20

| Rule | Ours | Evidence |
|---|---|---|
| Fake kill, respawn, debris | Matches: `Session::fake_kill_brick` hides the brick and queues its return | `rocket_knocks_bricks_out_in_a_brick_damage_minigame_and_they_respawn` |
| Minigame respawn time | Matches: `brick_respawn_ms`, default 30 s, UI clamps 2-300 s | same test: back no sooner than 30 s |
| Outside-minigame respawn | Matches the default: fixed 30 s. There is no host preference for it | `rocket_outside_a_minigame_follows_v20_lan_and_ownership_rules`: back 30-31 s after |
| LAN or single player, no minigame | Matches: anyone's bricks | same test |
| Internet, no minigame | Matches: shooter's own only | same test |
| LAN, minigame | Matches: Brick Damage only | `lan_hosts_let_minigame_rockets_break_anyones_bricks_like_v20`, `rocket_leaves_bricks_alone_in_a_minigame_with_brick_damage_off` |
| Internet, minigame | Matches: `can_radius_damage` with the brick owner's membership | same two tests |
| F8 lockout for bricks | **Was missing, fixed here**: a rocket already in flight broke bricks after its shooter's F8 drop | `a_rocket_in_flight_breaks_nothing_after_its_shooter_drops_in_a_minigame` fails without the fix |
| Hammer deletes for good | Matches, and ignores Brick Damage | `hammer_deletes_bricks_for_good_whatever_the_minigame_says`; chain-kill and trust in `tests/tools.rs` |
| Wand, admin wand, undo | Match (existing tests in `tests/tools.rs`) | `wand_breaks_anywhere_...`, `admin_destructo_wand_breaks_bricks_from_afar`, `undo_reverts_paint_and_print_then_breaks_the_plant` |
| `fakeKillBrick` | Matches, and ignores Brick Damage | `fake_kill_brick_ignores_brick_damage_and_respawns_on_its_own_time`: force 20 for vector 10, back after 5 s |
| `onBlownUp` | Fired per broken brick | `blow_up_bricks` |

All eight tests in `crates/sim/tests/brick_damage.rs` pass with
`--include-ignored`; they need the converted weapons pack and event catalog,
like the other weapon tests. The combat, tools and session suites also pass,
and `cargo clippy -p bri-sim --all-targets -- -D warnings` is clean.

## The fix

`blow_up_bricks` (`crates/sim/src/session/events.rs`) now returns early while
`teleport_lockout(source, 3000 ms)` holds, which is the same check weapon
damage and firing already use. v20 does this in both `onCollision` and
`onExplode`. Before, the trigger was blocked after an F8 drop but a rocket
fired just before it still broke bricks.

## Add-Ons and tonight's kept decisions

Add-Ons can still change all of this. The rules live in the host's weapon
path, not in content, so a package's own `explode` and event outputs take
other routes.

- **Add-On blasts on bricks (kept: permanent).** A package `explode` removes
  bricks for good, through `packages.rs`. This branch only touches weapon
  brick impacts, so the two don't conflict. An Add-On that wants v20-style
  temporary blasts can fake-kill instead.
- **Harmful event outputs (kept: follow brick trust).** These are SetVelocity,
  AddVelocity and Dismount; none of them touches bricks. `fakeKillBrick`
  follows the same principle: event outputs ignore minigame damage rules, as
  in v20.

## Choices made without Max

1. **Brick Damage defaults on** for new minigames, as v20's Create Mini-Game
   does. It was already our default.
2. **Outside minigames, single player breaks anyone's bricks,** because v20
   single player runs as `$Server::LAN`. On a LAN everyone is trusted anyway.
3. **No server setting for the outside-minigame respawn time.** We use v20's
   default of 30 s. Adding the preference would be a new option, and features
   are frozen.
4. **`fakeKillBrick` with time 0 waits 1 s.** v20 passes 0 ms to the engine.
   The engine side was not recovered, so the existing 1 s floor stays.

## Not verified or not implemented

- `canExplode`'s floating-volume allowance (`brickExplosionMaxVolumeFloating`)
  is engine code that was not recovered. We use only `maxVolume`, and also
  skip base plates and indestructible bricks. A brick that is floating and
  larger than `maxVolume` but within the floating limit would survive here
  and might break in v20.
- The internet, non-minigame "source object's brick group" allowance is not
  modelled. For a player shooter it never applies. It would matter only for
  brick-spawned shooters such as bots.
- A direct hit that fails the brick rules should fire `onProjectileHit` on the
  brick. The weapon contact path handles that input; this audit did not
  re-test it.
