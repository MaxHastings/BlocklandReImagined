# 2026-10-04 Life epochs: old lives, maps and connections stay behind

Branch `fix/life-epochs` from `53f05cf` (v0.2.3). Five review findings where
state from an earlier life, map or connection reached the next one. Each
was reproduced by a guard test that fails on `53f05cf` and passes after.

## 1. Auto-reconnect rejoined the server's name (S1)

Cause: `Attempt.name` doubled as the join address. Once the host's listing
named the server (`joined_server`, default "Blockland Server"), a lost
connection rejoined `join("Blockland Server")`, which `JoinTarget::parse`
rejects, so every reconnect after the first view ended on the failure
screen. The Add-On reload continuation and the identity question read the
same field.

Change: `Attempt.join_target` holds what the player joined, as given (an
address or an invite with its key); `name` is display only. Rejoin, the
`ReloadResume::Downloaded` continuation and the identity question read the
target (the question its keyless address, as `forget_server_identity` and
the saved list key by address). The Downloaded continuation now keeps an
invite's key, where it used to fall back to trust on first use.

Follow-on: a rejoin before the host timed the old connection out got a
fresh owner and "Name 2" (`join_inner` never reuses a live owner number,
deliberately: a same-key twin join is allowed and unprivileged). Rather
than make any same-principal join evict a live player (two windows on one
PC share `client.identity`), the rejoin now presents the lost connection's
resume ticket (`View::resume`, `Client::connect_fetching_resuming`). The
host checks ticket and principal first and then replaces the stale
connection (closes it with "Replaced by a new connection", disconnects the
owner, resumes it). A ticket the host does not know (it restarted) is
refused and the client joins afresh. Trust rules are unchanged: the ticket
with another key or none is refused while the player is connected, and a
resume is never an administrator without the host credential. No wire
format change (Hello already carries `resume`), so no protocol-changes
entry.

Tests: `app::tests::a_lost_connection_rejoins_the_address_not_the_server_name`
(bri-client lib; before: "the lost game was not rejoined"), and
`resume_tickets_are_bound_to_identity_and_one_live_connection`
(hardening_net) now asserts the replacement and that key B / no key cannot
take over a live player (before: `Rejected("Owner is still connected")`).
New `a_rejoin_with_its_ticket_comes_back_as_the_same_player` covers the
client API and the unknown-ticket fallback.

## 2. Change Map carried old-map Add-On state (S2)

Cause: `adopt` built each player with `..peer`, so uniforms, look limits,
respawn time, sitting, overlays, Add-On body choices, sport datablock,
camera path/orbit, spray and ghost state crossed into the new mission,
whose fresh Add-On store never undresses them.

Change: `Peer::fresh` is the one constructor (join, resume and map change).
`adopt` builds from it and copies an explicit allow-list: name, principal,
avatar, clan, administrator, paint pick (`current_color`, `fx_can`),
talking, move/request sequencing (`input_drain`, `processed_move`,
`seat_since`, `input_budget`, `last_sequence`, `last_move_sequence`) and
chat repeat state. A field added later starts over unless named there.

Test: `a_map_change_takes_off_the_old_maps_uniform` (addon-import slayer;
before: the Custom uniform appearance on the new map).

## 3. A rule's respawn time outlived its mini-game (S2)

Cause: `respawn_ms` (`set_respawn_time`) was cleared only by
`RestoreOwner`; leaving a game kept it and Death preferred it in free
build (7200 ticks in the test). Uniforms are not engine-scoped: Slayer
undresses on its own `left` hook, so only the engine-scoped respawn time
was changed.

Change: a `Membership` effect that changes the player's game, and
`Ended`, clear `respawn_ms`; the new game's rules set their own on spawn.

Test: `life_epoch_rule_respawn_time_ends_with_its_minigame` (bri-sim lib).

## 4. Teleports fired regions they jumped over (S2)

Cause: region observation sweeps `previous -> position` every tick, so an
instantRespawn, reset, teleport event, admin drop or portal crossing
entered every sensor on the straight line between.

Change: motor `Player::relocations` counts `teleport` calls and openings
passed (`step_through`); `VehiclesWorld::relocations` counts
`set_transform`, `carry` and actor passages. Riding a seat (`place`) and
authoritative corrections (`restore`) are travel and do not count.
`RuleState.previous` stores the count with the place; a changed count skips
the swept test, so only the landing region sees an entry. The documented
limit is removed from KNOWN-ISSUES and the Rule Workshop DESIGN/PLAYTEST
guides.

Test: `life_epoch_teleport_over_a_region_never_enters_it` (bri-sim lib).

## 5. Delayed Player rows hit the next life (S2)

Cause: event Player entities always had generation 1 and `alive` checked
only `is_alive(owner)`, so a delayed `kill` on Player passed the stale-job
check against the respawned body. In v20 schedules on the Player object die
with it.

Change: `Combat::body` (1 on joining, +1 per `respawn`) is the Player
entity's generation (`Session::player_entity`, also used by the bot death
projection); `alive` requires the same body. Client targets keep surviving
deaths. The minigame `LifeId` was not used: `RestoreOwner` allocates a new
life for an owner who keeps the same body.

Test: `life_epoch_delayed_player_output_dies_with_its_life` (bri-sim lib).

## Commands and evidence

Builds share `/home/claude/bri-target` with other worktrees, whose
workspace-crate artifacts collide; every result below was re-run with a
per-package `codegen-units = 29` override (`--config .cargo-wt.toml`, not
committed) so this tree's crates hash uniquely. Before = `53f05cf` sources
with only the test files changed.

- `cargo test -p bri-sim --lib life_epoch`: before 3 failed, after 3 ok.
- `cargo test -p bri-addon-import --test slayer a_map_change_takes_off`:
  before failed, after ok.
- `cargo test -p bri-net --test hardening_net resume_tickets`: before
  failed, after ok.
- `cargo test -p bri-client --lib a_lost_connection`: before failed, after ok.
- After, full: bri-sim/bri-motor/bri-vehicles lib; bri-net lib,
  hardening_net, loopback, package_sync; addon-import slayer, ports,
  tier_port; bri-sim portals, hardening_session, v20_events, vehicles,
  mode_and_voxels, sports, tether, hardening_packages, session, combat: all
  pass. bri-client lib: 441 pass; the two `items::*icon*::synthetic` tests
  fail only because they write PNGs into `<repo>/target/`, absent with an
  external `CARGO_TARGET_DIR` (environmental, untouched code).
- `cargo clippy -p bri-motor -p bri-vehicles -p bri-sim -p bri-net
  -p bri-client -p bri-addon-import --all-targets -- -D warnings`: clean
  (with an empty `CLIPPY_CONF_DIR`, since an uncommitted `clippy.toml` in the
  enclosing main checkout leaks into nested worktrees).
- `rustfmt --edition 2024` on the changed files.

Next: the gate's full workspace run on merge.
