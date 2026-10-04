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

Max reported rocketed bricks staying gone, so the round trip is also proven
on the client side. `crates/client/tests/brick_respawn.rs` hosts a LAN game
over real loopback QUIC. The host plays through its own client, as Start
Game does, and a second client joins. Each player rockets their own
free-build brick outside any minigame.

Each screen runs the app's own pipeline: the network worker and world log,
the brick query and collision mirror (`Building`), and the render chunks
(`ChunkedWorld`). On both screens, both bricks:

- throw debris and stop being drawn and solid, while staying in the replica;
- are still out 20 s later;
- are drawn, solid and clickable again about 30 s after the blast.

It passes (31 s), so no client bug turned up on this path. If a rocketed brick
stays gone, the likely causes are an Add-On `explode`, which deletes for good
as decided above, or a build from before fake kills landed.

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
5. **A blast throws at most 2048 of its bricks as debris**
   (`session::MAX_BLAST_DEBRIS`, 2026-10-04). Every brick in reach is knocked
   out and respawns; at most 2048, spread over the blast, get a `BrickKill`
   cue. That is the most debris a client keeps at its highest Physics
   Quality, and a v20 client over its `$pref::Physics::MaxBricks` only hides
   the rest. The Mini-Nuke takes out about 5,000 bricks of Badspot's
   Christmas Block Party; one cue each overflowed the presentation queue and
   pushed out the blast's own explosion and sound. Clients take the
   unannounced bricks in the blast's reach out at once, as they do the
   announced ones, rather than fading them.

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

## How a dying brick looks (2026-09-28, branch `claude/tool-kill-feel`)

Max: bricks broken with the hammer or wand stayed as solid debris that took
a while to get out of the way, so it was hard to see what had been broken.
In v20 they did not collide, fell through the ground and turned ghostly
fast, unlike bricks knocked out in a minigame.

v20's scripts reach the engine two ways. Tools call `killBrick` (hammer
10552, wand 12129, admin wand 12923, undo 5711, chain undo 16816 in
`allGameScripts.cs`); blasts and `fakeKillBrick` call
`transmitBrickExplosion` (8093, 8187, 17473). Read-only capstone disassembly
of `blocklandv20.exe` (image base 0x400000) shows the client draws them
differently:

- **`killBrick`** (`isDead`, +0x25d). The death update (0x5399a8-0x539c0a)
  throws the ghost brick at `normalize(rand-0.5, rand-0.5, 1 + 4 rand) * 8`
  (Z up) and spins it about a random axis at `rand * clamp(8 / brickSizeY,
  3, 8)` rad/s (integer division, `brickSizeY` is datablock +0x6c). Its
  advance (0x53cfa9-0x53d11c) is closed form, `start + v t - 16 t^2`, with
  no collision test at all. Its target alpha is 0 (0x539965); 500 ms after
  death the colour update (0x53d29d-0x53d2ca, 0x53d553) closes on it by
  `3 dt` per frame. It is never a physics body.
- **Brick explosions** (`isFakeDead`, +0x25e). With `$Physics::enabled` the
  brick is registered as a Bullet rigid body (`registerFakeDeathBrick`,
  0x4e3ca0, called at 0x5393c2) that tumbles against the world, up to
  `$Physics::maxBricks`; the oldest leave physics and fall ballistically
  (0x5338c0). Without physics the brick falls ballistically too, fading
  after 0.7-1.2 s (0x539359).

Ours before: every `BrickKill` cue became a Rapier body, solid 3 s and
fading 2 s, whatever broke it. Now the cue carries `BrickDeath` (`Kill` or
`Blast`) and the server picks it by cause: `kill_brick` and
`kill_one_brick` without a blast (hammer, wands, undo, chain kills, package
removals without a blast) are `Kill`; `fake_kill_brick` (weapon blasts,
`fakeKillBrick`) and package blasts are `Blast`. Clients draw `Kill` as
v20 does, with no physics body, in `BrickDebris` next to the bodies and
through the same models. Whether the brick is gone and when it returns is
the same synced state as before; only the drawing is local.

Measured headless (`crates/client/tests/tool_kill_feel.rs`: LAN host
playing through its own client, as single player does, plus a joiner over
loopback QUIC; debris stepped at 60 fps on each screen's own mirror):

| Kill, per screen | Before | After |
|---|---|---|
| Hammer and wand: solid | 4.98 s | never |
| Hammer and wand: within 2 units of its spot | 4.88 s | 0.55 s |
| Hammer and wand: visible (alpha > 0.05) | 4.88 s | 1.48 s (gone by 2.35 s) |
| Rocket in a Brick Damage minigame | 4.98 s solid, 4.88 s visible | unchanged |

Host and joiner read identical numbers.

## Saves loaded with ownership (2026-09-30)

On internet hosts, a brick's minigame is found from its brick group as
`Session::brick_group_owner_for` resolves it: the connected player of that
number or identity, else the minigame owner when they have Full trust over
the group, else nobody. See the progress entry of that date.
