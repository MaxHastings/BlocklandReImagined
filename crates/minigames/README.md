# Native vanilla minigames

`bri-minigames` is a deterministic, host-driven authoritative rules module. It
contains no renderer, socket, physics owner, legacy reader or script interpreter.
It is a root workspace member. The host session in `crates/sim/src/session.rs`
owns a `MinigamesWorld` and replicates `MiniGameView`s to clients.

## Integration

Load `content/minigames-pack-002/catalog.json` as `Catalog`, then:

```rust
use bri_minigames::*;

let mut world = MinigamesWorld::new(catalog, PolicyMode::Internet, true)?;
let player = world.connect(authenticated_account, display_name, is_admin)?;
world.set_ready(player, true)?; // after the host's loading/connection phase completes
let effects = world.execute(Command::Create {
    actor: player, color: 0, settings: Settings::default(),
})?;
// Apply every returned effect in order to the authoritative host world.
```

Keep `PlayerId { account, session }` separate from `ActorId`, vehicle IDs, brick
IDs and connection addresses. The host maintains explicit adapter maps. A
reconnect receives a new session even when its authenticated account is unchanged.
Never accept an account, actor, `EventAuthority::System`, owner-brick identity,
moderation authority or captured projectile provenance from an unvalidated packet.
The command/effect enums deliberately have no network serialization contract.

Call `step()` once per host tick at **120 Hz**. Apply returned `StartBall` effects.
There is no internal unbounded effect queue. Every command returns its own effects;
the host must consume them before the next command. Errors leave commands unchanged.
`games()`, `players()`, `game()`, `player()` and `free_colors()` supply list/detail,
HUD and late-join views. Commands cover create/configure/join/leave/invite/accept/
reject/ignore/kick/reset/respawn-all/end/manual-respawn and three message events.
The UI should initialize a new title from the owner's display name plus
`'s Mini-Game`, capped to 35 characters; `Settings::default()` uses the original
disconnected GUI fallback title, `Default Mini-Game`.
`disconnect()` removes membership and ends an owned game; skip departed player's
spawn/view effects while applying cleanup and remaining members' effects.

Host moderation uses `moderate_end()`, separate from owner-only UI commands.
An admin flag does not grant configuration, kick or end ownership. The source's
admin exception for immediate **outside-minigame** respawn is retained.

## Required effect bindings

| Effect | Authoritative host action |
| --- | --- |
| `Created`, `Configured`, `Ended`, `Membership`, `Invitation`, `Score` | Update replicated game/member lists, name color, invitation dialogs, title/settings and scoreboard. Clear playing/running UI on exit/end. |
| `Spawn` | Replace player body, assign returned `LifeId`, choose spawn, reset health/energy and apply `Equipment`. `None` equipment means standard outside-game tools/player type. Spawn includes immediate start-ball mounting when present. |
| `RestoreOwner` | Keep living owner at current position, heal, restore Standard Player and outside default tools; replace life token. Preserve/reconcile held tool as original end callback does. |
| `Cleanup` | Cancel owned event schedules when requested; reset owned spawn vehicles; delete that player's event-created player/projectile/vehicle/corpse objects. Apply before following spawn. |
| `EjectVehicles` | Reevaluate minigame use permission of occupants of this account's spawn-brick vehicles; eject disallowed occupants. |
| `ApplyEquipment` | Replace only `changed_slots`; remount the currently held changed tool or unmount if empty, unless holding brick image. On `change_player_type`, change native data, energy HUD, arms/avatar and brick image; dismount if new type cannot ride. `cancel_building` clears brick inventory/ghost/temp brick and mounted brick image. `unmount_paint` clears held spray image. Update build/paint/wand UI from equipment. |
| `StartBall` | Mount image for living members only when slot zero hands are empty; sports changes use six ticks/50 ms. |
| `Death`, `RespawnDeadline` | Enter observer/death presentation, display remaining respawn wait. Deadline permits manual `Respawn`; it does not schedule automatic spawn. |
| `ResetBricks` | Respawn vehicles on listed owners' vehicle spawn bricks and reveal their brick items immediately. |
| `Reset` | Replicate round/reset notice; scores and respawns already have explicit preceding effects. |
| `Message` | Deliver chat/center/bottom text only to listed recipients, respecting authored display duration. Render supported game text markup safely. |

`Spawn`/`RestoreOwner` invalidates old life tokens. Only after applying accepted
damage and observing authoritative death call `died(victim, life, killer)` once.
Duplicate/dead/old life tokens are rejected. A killer from another minigame is
rejected; map/environment damage passes `None`. Suicide applies only kill-self
points; other deaths apply die points and valid killers receive kill-player points.
`brick_score(player, planted)` follows a committed brick transaction, never a
placement/removal request. Host-authorized Client score events use `event_score`.

## Permissions and object attribution

Use `target_for_player()` for a current player life. For bricks/items/bots/vehicles,
construct `Target::Object` from the authoritative owner and origin:

* `Membership::Owner` derives membership from the currently connected brick owner.
* `Outside` has no explicit minigame.
* `Explicit { game, round }` captures host-created event object scope; stale games
  and rounds are rejected. `spawn_brick` distinguishes loose item pickup.

At projectile creation call `projectile_source(player)` and retain its returned
`DamageSource` beside the projectile. It carries session/game/round provenance.
Vehicle damage uses `DamageSource::Vehicle` with the validated driver and captured
scope. Drivers, occupants and vehicle owners remain distinct host identities.
Call `can_damage` for direct damage, `can_radius_damage` for explosion damage,
`can_use` for pickups/mounts/tool use and `can_build` for building/paint/wand. Do
not treat `OutsideMinigames` as permission: apply the host's normal sandbox/trust
policy. Own outside vehicles retain the source same-owner exception for use/damage.
Damage does not consult `players_use_own_bricks`; that setting affects use/spawn.

Environment fall/impact checks `falling_damage`; outside games it returns the
tri-state for the host server preference. Lava/water/suicide/script damage bypasses
weapon/self settings as source armor callbacks do. The host still owns invulnerability,
held admin wand immunity, passenger protection, trust and damage amounts.

`PolicyMode::Internet` is the default recommended host setting even for local
transport. `LegacyLan` explicitly reproduces original relaxed LAN use and object
damage checks, different cleanup behavior, and nonpersistent invite ignore. It
must be a trusted host configuration, not inferred from a client's claimed network.
LAN radius damage still honors the separately checked self-damage flag.

## Spawn and respawn timers

`pick_spawn(player, &[SpawnPoint { id, owner }], random_word)` implements owner-only,
all-member or own-bricks spawn selection. Each eligible unique brick has equal
weight; sorted IDs make iteration deterministic. A uniform host PRNG word chooses
the bucket. `None` means map/default spawn. Collision clearance belongs to the host.

`respawn_delay(game, RespawnObject::{Vehicle,Brick,Item})` returns fixed ticks. Item
respawn is four seconds. Vehicle zero means immediate. Outside games, vehicle delay
is zero and brick delay defaults to 30 seconds; a configured outside server brick
preference must override that fallback. `wheeled_destroy_respawn_delay(source_game,
burn_ms)` returns `max(configured vehicle delay, burn_ms) + 100ms`, rounded up. Source
wheeled/AIPlayer deaths derive the game from the damage source; flying vehicles
derive it from the destroyed vehicle. The latter and AIPlayer use the base delay.
Host owns scheduled spawn replacement/cancellation and authored burn/destruction.

Respawn clicks use strict elapsed-time `>`: 1000 ms becomes 121 ticks, rather than
120. Changing respawn duration recomputes already-dead members' deadlines. Stock
lives are `Lives::Unlimited`; no invented round win condition or finite limit.

## Persistence and bounds

`Preset::new`, `to_json`, `from_json` store versioned settings independent of player
connections. `save`/`restore` store native world state, deadlines, scores, invites,
ignores, membership, game rounds and monotonic identity counters. Restore validates
membership/color/identity invariants against a trusted native catalog. Persist this
snapshot alongside the host world at the same tick. Resume saved session IDs only
when the host restores the same authenticated bindings; otherwise disconnect them
before fresh clients connect. Snapshot files are trusted server state, not uploads
from clients. Schema version is 1; unsupported fields/settings fail validation.

Bounds: 1024 players; 10 simultaneous games/colors; 5 equipment slots; 35 title
characters; 200 event-text characters; event duration 1–10 s; 65,536 spawn candidates;
8 MiB snapshot; 64 KiB preset; bounded catalog entries and 160-byte content IDs.
Scores use saturating i64 arithmetic. Pending invites are one per player and do not
expire in stock behavior; end/disconnect invalidates their game. Respawn settings
are validated in milliseconds against original server bounds. UI adapters should
clamp legacy seconds (RT 1–30, VRT 0–300, BRT 2–300); the rules never silently replace
unknown content or an invalid setting with another item/player type.

## Verification

```powershell
cargo test --manifest-path crates/minigames/Cargo.toml
cargo clippy --manifest-path crates/minigames/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path crates/minigames/Cargo.toml --example catalog_smoke -- content/minigames-pack-002/catalog.json
```

Twenty tests cover source defaults, all settings' rule/effect routes, ownership,
two games plus outsiders, explicit LAN policy, generation/round/life invalidation,
event permissions, scoring, spawn selection, sports loadouts, timers and malicious
snapshot/input bounds. The full native catalog smoke configures all 11 selectable
player types and 21 items, then runs eight simulated players/two games for 12,000
ticks and 671,200 policy checks with native save/restore. It is a pure rules test,
not a networking, physics, rendering or gameplay-feel test.

Native playable implementations of every player type, held tool and sports
behavior belong to those subsystems. This crate does not establish their visual
or movement fidelity.
See `docs/research/minigames/README.md` for precise source evidence and decisions.
