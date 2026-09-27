# v20 bug sweep (2026-09-27)

A pass over weapons and items, sound cues, GUI screens, minigame flow, wrench
events, save/load and replication, compared against the decompiled v20
scripts in `.research/bl-decompiled/v20` and the stock add-on scripts in the
reference installation. Areas owned by other sessions (vehicles, head
collision, player animations, Torque quirks, trees, spray cans, and the five
threads started on 2026-09-27) were skipped except where noted.

Ranked by what a player would hit first.

## Fixed in this sweep

- **Explosion splash damage did not follow v20.** `ProjectileData::onExplode`
  has no line-of-sight test, measures to the target's centre, and uses a
  quadratic falloff `1 - (d/r)^2` for damage and impulse. Ours blocked
  splash behind any brick, measured to the nearest bounding-box face, used a
  linear falloff and ignored projectile scale. A rocket at half its damage
  radius now does 75 instead of 50. Downward pushes on grounded targets are
  flattened, as in `radiusImpulse`. (`crates/weapons/src/runtime.rs`)
- **Rockets hurt their shooter with Self Damage off.** The host checked
  splash with the direct-hit rule, so in LAN and single-player minigames
  your own rocket still hurt you. Splash now goes through
  `can_radius_damage`, which honours `selfDamage` like v20's `onExplode`.
  (`crates/sim/src/session/weapons.rs`, `weapon_query.rs`)
- **Minigame reset left vehicles and items as they were.** The rules engine
  emitted `ResetBricks`, but the host ignored it. Reset now respawns the
  owners' vehicle and bot bricks (`spawnVehicle(0)`) and makes their picked-up
  items available again (`Item.fadeIn(0)`). A blocked vehicle respawn is
  noted, not fatal. (`crates/sim/src/session/combat.rs`)
- **Event-driven minigame reset was silent.** `MiniGame > Reset` from a brick
  now sends "*name* reset the mini-game" like the owner's Reset button.

## Still open, highest first

1. **One failed step stops the whole host.** `net/src/server.rs` runs
   `session.step()?`, and `step` propagates errors from combat, tools, brick
   damage and minigame effects (`damage_player(...)?`, `tool_fire(...)?`,
   `apply_minigame_effects(...)?`). Any unexpected error in a gameplay
   adapter ends the game for everyone instead of being logged. Needs an
   engine decision: isolate adapter errors per event and keep ticking.
2. **Trust is stricter than v20 on LAN and for public bricks.**
   `getTrustLevel` returns full trust for everyone when `$Server::LAN`, and
   full trust against any public-domain brick group. Ours
   (`tools.rs: trusted_brick_edit`) allows only the brick owner or an admin,
   so on LAN a second player cannot hammer, wrench or paint anything they did
   not build, and nobody but admins can edit public (owner 0) bricks.
   Multiplayer and admin thread (Trust).
3. **A minigame loadout with the same item twice breaks every respawn.**
   v20's `ItemData::onPickup` has no duplicate check and the minigame
   equipment lists allow repeats. Ours rejects duplicates in
   `WeaponsWorld::give_at`, `set_inventory` and `ToolInventory::validate`
   (also on the client replica), so `give_loadout` fails and the respawn
   aborts. Relaxing it changes what clients accept, so it needs a protocol
   bump; the coordinator assigned VERSION 18 but the bump was not approved in
   this session. Held for Max's go-ahead.
4. **Client > PlaySound is heard by everyone.** v20's
   `GameConnection::playSound` is `play2D` to that client only. Ours emits a
   positional world cue at the player's feet for all clients. The fix is a
   private `Notice::Sound`; it needs the same protocol bump as item 3.
5. **The Horse Ray does nothing.** The runtime emits `HorseTransform`, but
   `step_weapons` counts it as a "weapon integration gap". Gameplay leftovers
   thread (player types).
6. **No join and leave messages or sounds.** v20 sends "*name* connected."
   (`MsgClientJoin`) and "*name* has left the game." and plays
   `ClientJoinSound`/`ClientDropSound`; `MsgAdminForce` lines play
   `AdminSound`. None exist in the session, and the `ui.client_join`,
   `ui.client_drop` and `ui.admin` triggers are never fired. Multiplayer and
   admin thread.
7. **Minigame Cleanup and EjectVehicles effects are ignored.** On join, leave
   and reset (non-LAN), v20 clears the client's event schedules, resets their
   vehicles and removes their event-spawned projectiles and vehicles; the
   owner's vehicle bricks eject non-members. Vehicles thread plus events.
8. **Balls can never be caught.** `step_weapons` passes a `catch` closure
   that always returns false, `StartBall` is ignored and the `BallCaught`,
   `BallRest`, `FootballCatch` and `Tumble` events fall into the gap counter.
   Gameplay leftovers thread (ball throw).
9. **Gun shell casings never appear.** The host emits `WeaponShell` cues and
   `weapon_effects.rs` queues them as `HostRequest::Shell`, but nothing calls
   `take_host_requests`, and `client/src/weapon_debris.rs` (772 lines, with
   `weapon-debris-pack-001..003` on disk) is never constructed by the app.
10. **Who's-typing indicator is missing.** `UiAction::StartTyping` and
    `StopTyping` are no-ops; v20 sends `MsgStartTalking`/`MsgStopTalking` to
    everyone and shows the names above the chat (`WhoTalk_addID`). Needs
    replicated state. Save, load and UI thread (chat HUD).
11. **Explosion vertical impulse is not converted.** `Explosion` has no
    `impulseVertical`; the tank shell and cannon ball explosions (2000) lose
    their upward kick. Vehicles thread.
12. **Recently-F8'd players are not protected.** v20 skips projectile
    collisions and explosions for 3 s, and item pickups for 5 s, after a
    minigame player uses the drop-at-camera key (`lastF8Time`). Minor.
13. **Brick PlaySound plays on fake-dead bricks.** v20 returns when
    `getFakeDeadTime() > 120`. Minor.
14. **Missing v20 dialogs**, not verified in depth: `TrustInviteGui`,
    `JoinServerPassGui`, `LoadBricksColorGui` and `saveBricksWarningGui` have
    no native screen.
15. **Two ignored weapons tests fail on main**, unrelated to this sweep:
    `pack_complete_native_models_and_hidden_variants` still expects 17 items
    (the pack has 21) and `malicious_projectile_and_state_inputs_are_bounded`
    expects a transition-budget diagnostic the wand self-loop rule now
    prevents. Cleanup thread.

## Checked and matching v20

- All 21 stock items and 46 projectiles are in `weapons-pack-007`.
- Every v20 brick input event is fired by the host, including touch, key,
  ball, teledoor and print-count inputs.
- Wrench hit and miss sounds, trust centre prints and admin override follow
  `wrenchImage::onHitObject`.
- Main-menu title music: v20 defines `MainMenuGui::PlayMusic` but never calls
  it, so no title music is correct.
- Minigame join/leave/end chat lines and the 5 s reset cooldown match
  `MiniGameSO`.

## Verification

```powershell
cargo test -p bri-weapons -p bri-sim -p bri-minigames -p bri-events
cargo test -p bri-weapons --test runtime -- --ignored   # needs content/
cargo clippy -p bri-weapons -p bri-sim --all-targets
```

All non-ignored tests pass and Clippy is clean. The ignored weapons suite
passes except the two pre-existing failures in item 15; the updated
`explosion_radius_falloff_ignores_cover_and_intends_bricks` passes. The reset
fix has no dedicated test: the item harness cannot give a joining player the
brick owner's id.
