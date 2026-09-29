# v20 behaviour audit (events, server commands, datablock flags)

Branch `claude/bug-sweep-v20-behaviour`, 2026-09-28. What v20's server scripts
do, compared with what ours does, rule by rule. v20 citations are to the
decompiled scripts in `.research/bl-decompiled/v20` (kept out of git):
**G** = `server/scripts/allGameScripts.cs`, **M** = `server/mainServer.cs`.
Ours are repository paths. Status is one of:

- **matched**: same conditions and effects, as far as read here;
- **fixed**: was different, now follows v20, with a test citing the v20 line;
- **different**: deliberately kept, with the reason;
- **open**: different or missing, not changed here (owner or reason given).

Scope notes. Areas other lanes own were read but not changed: admin ranks
and clear commands, the Duplicator, event explosion/projectile caps, brick
load and replication, the name fix, mounts and the Stunt Plane. Engine
(C++) outputs such as `setColor` have no script body; for those the v20
side is the registration line and its parameter range, and our side is the
catalog entry (`content/events-pack-002/catalog.json`, compiled by
`crates/events/src/catalog.rs:294`) plus the host's effect in
`crates/sim/src/session/events.rs`.

## Fixed in this branch

| Rule | v20 | Was | Now | Test |
|---|---|---|---|---|
| Player and Client outputs act on whoever set the input off, in or out of a minigame (kill bricks in free build) | G:9527 `Player::kill` is self-`Damage`; G:9201 `Armor::Damage` has no minigame check; G:18186-18199 | `harmful()` refused Kill, AddHealth, SetHealth, BurnPlayer, ClearTools, InstantRespawn, SpawnExplosion, SpawnProjectile, ChangeDataBlock, SetPlayerScale and IncScore on anyone but the brick owner outside a shared minigame | `EventHost::permitted` allows Player/Client targets (`events.rs:1260`) | `v20_events::a_kill_brick_kills_whoever_sets_it_off_outside_minigames` |
| Hurting Player outputs respect spawn protection | G:9207, `$Game::PlayerInvulnerabilityTime` 2500 ms (G:2851) | Event Kill ignored it | `player_op` skips Kill, AddHealth<0, SetHealth 0 while `spawn_protected` (`events.rs:1023`, `combat.rs`) | same test |
| MiniGame target on single-player and LAN servers is the activator's game | G:17136-17139 (every input) | always the internet rule (brick must share the game) | `fire_input` passes `lan_host` to `minigame_target` (`events.rs:284`) | `v20_events::a_lan_minigame_owner_resets_their_game_from_anyones_brick` |
| The minigame's owner may Reset it from any brick | G:22236-22262 `MiniGameSO::Reset` | owner's own brick required | `minigame_op` uses `EventAuthority::Owner` for Reset by the owner (`events.rs:1173`) | same test |
| spawnItem, spawnProjectile, spawnExplosion do nothing from a fake-killed brick or one neither drawn nor ray-cast | G:17514, G:17600, G:17660 | only spawnExplosion checked fake death | `brick_op` checks `brick_spawn_allowed` (`events.rs:801`) | `v20_events::a_hidden_brick_spawns_no_explosion` (content-backed) |
| radiusImpulse outside minigames pushes only the activator on internet servers, everyone in reach on LAN; in a minigame, whom the activator may damage | G:17868-17926 | pushed every player in reach | filter in `RadiusImpulse` (`events.rs:979`) | `v20_events::a_radius_impulse_pushes_only_the_activator_on_internet_servers` |
| recoverVehicle leaves a ridden vehicle alone | G:17839-17866 | same as respawnVehicle | `recover_vehicle_brick` (`vehicles.rs`) | `vehicles::recover_vehicle_leaves_a_ridden_vehicle_alone` (content-backed, ignored) |
| `/cancelEvents` for players | G:4958-4990 | unknown command | `cancel_own_events` (`admin_world.rs`), routed like `/brickCount` (`packages.rs:1774`) | `v20_events::cancel_events_stops_a_players_own_pending_events` |
| At most 100 rows a brick; delays clamped to 0..30000 ms | G:740 `serverCmdAddEvent`, `$Game::MaxEventsPerBrick` G:2853 | 1024 rows, delays up to 300000 ms | `limit_rows` on every SetEvents (`events.rs`, `session.rs:1373`) | `hardening_session::event_rows_keep_v20s_hundred_row_and_thirty_second_limits` |
| Touch events skip a player holding the admin wand | G:17167-17173 | ran | `fire_touch_events` (`events.rs:554`) | covered by the matched touch path; no separate test |

## Input events (16 in the catalog)

| Input | v20 | Ours | Status |
|---|---|---|---|
| onActivate | G:17131 (targets G:17122) | fired by `Command::Activate`, `session.rs:1760` | matched (LAN target fixed) |
| onPlayerTouch | G:17151, G:17157 `fxDTSBrickData::onPlayerTouch` | `fire_touches` → `fire_touch_events`, `events.rs:554` | different: ours fires once on contact entry (`bri-motor` `player.rs`); v20's two-second touch immunity after spawning (G:17159, `$Game::OnTouchImmuneTime` G:2850) would then lose the touch of a player spawned onto the brick for good, so it is not applied. Kill bricks still spare a fresh spawn through the damage rule above. Admin wand skip fixed. |
| onBotTouch | G:17189-17234 | `fire_touch_events` (bots), `events.rs:569` | open: v20 also gives Client (the bot brick owner's client) and Driver targets; ours gives Bot only |
| onProjectileHit | G:17279 | `weapons.rs:340`; zero-delay Projectile rows act at contact (`events.rs` `set_projectile_response`) | matched; v20's per-brick and per-client flood checks (G:17285-17294) are covered by the host's event budget (event-caps lane) |
| onBlownUp | G:17241 | `blow_up_bricks`, `events.rs:536` | matched |
| onRespawn | G:17263 | `respawn_brick`, `events.rs:421` | different: v20's `ProcessInputEvent` (G:111-123) runs nothing without a client; ours runs a respawned brick's rows with no client. Kept: rows on Self need none. |
| onRelay | G:17273, `fireRelay` G:17710 (15 ms self limit) | `bri-events` runtime `runtime.rs:945` | matched; relay floor 33 ms for non-administrators (existing) |
| onPrintCountOverFlow / UnderFlow | G:17326, G:17337, counts G:18062-18093 | `runtime.rs:969-979` | matched |
| onToolBreak | tutorial add-on; G:17348; not tracked for cancel (G:400) | `tools.rs:424`, `tools.rs:482`; `runtime.rs:570` | matched |
| onTeledoorEnter / Exit | Brick_Teledoor add-on | `special.rs:212-213` | matched |
| OnKeyMatch / OnKeyMismatch, onTouchdown, onBallHit | Item_Key, Item_Sports add-ons | `weapons.rs:461-471` | matched |

Processing (G:111 `SimObject::ProcessInputEvent`): zero-delay CancelEvents
rows first, the schedule quota ("Too many events at once!") and the
repeated-overflow cleanup are in `schedules_exceeded` and the event engine
(event-caps lane); matched as far as read.

## Output events (65)

Parameter ranges come from the registrations and are enforced by
`Catalog::validate_row` (`catalog.rs:196`); every one below matched its
registration.

| Output | v20 | Ours | Status |
|---|---|---|---|
| setColor, setColorFX, setShapeFX, setColliding, setRendering, setRayCasting | G:17368-17373 (engine) | `events.rs:829-834` | matched |
| disappear | G:17405-17440 | engine Presence ops, `runtime.rs`; `events.rs:835` | matched |
| fakeKillBrick | G:17459 (time `mClamp(0, 300)`) | `events.rs:852` | open: a time of 0 becomes 1 s here (v20 restores at once); minor |
| respawn | G:17478 | `events.rs:857` | matched |
| setEmitter, setEmitterDirection, setLight, setItem, setItemDirection, setItemPosition, setMusic | G:11105-11485 | `events.rs:883-915`, with environment/item quotas | matched |
| playSound | G:17487 (not while fake-dead >120 ms; not looping or 2D) | `events.rs:928`; the choices are the `event-param:Sound` list | matched (looping/2D excluded by the list) |
| spawnItem, spawnProjectile, spawnExplosion | G:17512, G:17598, G:17658 | `events.rs:945-979` | fixed (hidden/fake-dead); caps are the event-caps lane |
| fireRelay, fireRelayUp/Down/North/East/South/West | G:17710-17832 | `runtime.rs`, neighbours `events.rs:1279` | different: ours relays only to the same owner's bricks. v20's `fireRelayFromBox` test `!%searchObj.getGroup() == %group` (G:17818) is always false, so v20 relays reached anyone's bricks; that lets one player trigger another's events, so it is not copied. |
| cancelEvents, setEventEnabled, toggleEventEnabled | G:111-500 (`SetEventEnabled` G:~430) | `bri-events` runtime | matched |
| setVehicle, respawnVehicle | G:11527, G:17834 | `events.rs:918-925` | matched |
| recoverVehicle | G:17839 | `vehicles.rs` `recover_vehicle_brick` | fixed |
| radiusImpulse | G:17868 | `events.rs:979` | fixed for players; open: v20 also pushes vehicles, corpses and items |
| incrementPrintCount, decrementPrintCount, setPrintCount | G:18062-18093 | `catalog.rs:361-378`, runtime | matched |
| CenterPrint, BottomPrint, ChatMessage (Client) | G:18140-18163 (`%1` name; chat `%2` score) | `client_op`, `events.rs:1134`; `semantics::client_message` | matched |
| IncScore (Client) | G:18138, M:1751 (works outside minigames) | `minigames::event_score` | matched |
| playSound (Client) | G:18165 | `events.rs:1134` | matched |
| Kill, AddHealth, SetHealth | G:9527, G:18334, G:18350 | `player_op`, `events.rs:1023` | fixed (who and spawn protection) |
| BurnPlayer, ClearBurn, SetVelocity, AddVelocity, SetPlayerScale, ChangeDataBlock, Dismount, ClearTools, InstantRespawn | G:9898, G:18186-18301 | `events.rs:1042-1118` | matched |
| SpawnProjectile, SpawnExplosion (Player) | G:18230-18282 | `events.rs:1076-1097` | matched (eye/forward; body centre for explosions) |
| ChatMsgAll, CenterPrintAll, BottomPrintAll, RespawnAll (MiniGame) | G:22330-22565 (member, and the owner's brick) | `minigame_op`; `minigames::authorize_event` | matched |
| Reset (MiniGame) | G:22236 (owner from any brick; 5 s cooldown) | `minigame_op` | fixed |
| Explode, Delete, Bounce, Redirect (Projectile) | G:18376-18420 | zero-delay rows at contact, `set_projectile_response` | different: delayed Projectile rows are refused (`events.rs` `apply`); by then v20's projectile has usually hit or gone |

## Server commands (98)

`/x` typed in chat calls `serverCmdX` in v20. Ours routes chat slash
commands in `crates/client/src/app.rs:6476` (vanilla ones to typed
commands, the rest to the host's package commands) and the host's
`Session::command` (`crates/sim/src/session.rs`).

| Command | v20 | Ours | Status |
|---|---|---|---|
| AddEvent, ClearEvents | G:740, G:1164 | `ToolAction::SetEvents`, `session.rs:1365-1380` | fixed (row/delay limits); relay floor matched. `$Pref::Server::WrenchEventsAdminOnly` (default 0) has no setting: matched at its default |
| RequestEventTables | G:1321 | host catalog sent at join (`UiUpdate::Events`) | matched |
| ClearColors, SetColorMethod, SetSaveUploadDirName, InitUploadHandshake, StartSaveFileUpload, UploadSaveFileLine, CancelSaveFileUpload, EndSaveFileUpload, ReloadBricks | G:1708-2790 | `Command::LoadBuild` (administrator only, as G:1761), colour choice `UiAction::LoadBricksColors` | matched (one command instead of a line upload) |
| Kick, Ban, RequestBanList, UnBan | G:3045-3585 | `bri-admin` | admin-ranks lane; not audited here |
| MagicWand | G:3596 (admin) | `AdminAction::DestructoWand` | matched |
| Wand | G:3607 (minigame `enableWand`) | `use_wand`, `tools.rs:361` | matched |
| ChangeMap, GetMapList | G:4252, G:4319 | `AdminAction::ChangeMap`, `RequestMaps` | matched |
| GetID, GetTransform | G:4420, G:4443 (admin debug) | none | different: debug readouts of engine object ids, which ours does not have |
| Fetch, Find, Warp, Spy, TimeScale, RealBrickCount, CancelAllEvents | G:4502-4692, G:4930 | `admin_ui::chat_command`, `crates/client/src/admin_ui.rs:285`; name match as `findClientByName` G:4466 | matched (admin checks; timescale clamp 0.2..2) |
| Ret | G:4594 (admin) | `Command::ControlPlayer`, anyone | different: returning to one's own body is harmless and ours also uses it after dying in a spy view |
| BrickCount | G:4721 (anyone) | `packages.rs:1788` | matched |
| TripOut, ColorTest | G:4733, G:4783 (admin jokes) | none | open: not implemented |
| Light | G:4751 (alive) | `toggle_light`, `combat.rs:690` | matched |
| DropPlayerAtCamera, DropCameraAtPlayer | G:4792, G:4866 (admin) | `Command::DropPlayerAtCamera`, admin camera | matched |
| Suicide | G:4882 → `Player::kill` → `Armor::Damage` (ignored 2.5 s after spawning) | `suicide`, `combat.rs:667`, immediate | different: kept immediate. Many flows (tutorial, tests, a stuck spawn) rely on it, and v20's effect is only a 2.5 s wait |
| CancelEvents | G:4958 | `cancel_own_events` | fixed |
| ClearBricks (`ServerCmdClearBricks`) | G:~4905 | `clear_own_bricks`, `admin_world.rs:93` | admin-ranks lane |
| DFG, GetPZ, RayPZ, SetPreviewCenter, IconInit, DoAllIcons, DoIcon, DoItemIcon, DoPackIcons, DoSecondPackIcons, DoPlayerIcons | G:5126-6713 | none | different: developer tools for icon and depth-of-field renders; ours renders icons offline |
| InstantUseBrick, BuyBrick, ClearInventory, UsePrintGun, UseFXCan, UseSprayCan, UseHammer, SetPrint | G:5242-6307 | building (`crates/client/src/building.rs`), `Command::UseSprayCan`/`UseFxCan`, `ToolAction::SetPrint` | matched (building and tools lanes) |
| StartTalking, StopTalking | G:6355, G:6369 | `Command::Talking` | matched |
| NextSeat, PrevSeat | G:6383, G:6466 | `Command::SwitchSeat` | matched |
| VehicleSpawn_Respawn, SetWrenchData | G:10960, G:10982 | `ToolAction::RespawnVehicle`, `ToolAction::SetWrench` (trust-checked, `hardening_session`) | matched |
| Alarm, BSD, Zombie, Hug, Sit | G:19878-19925 | `Command::Emote`, `app.rs:6504-6507` | matched |
| Trust_Invite, AcceptTrustInvite, RejectTrustInvite, IgnoreTrustInvite, UnIgnore, Trust_Demote, TrustListUpload_Line, TrustListUpload_Done | G:20743-21143 | `Command::Trust*`, `session/trust.rs` | matched (LAN trust = You, G:21249) |
| RequestMiniGameList, JoinMiniGame, LeaveMiniGame, RemoveFromMiniGame, InviteToMiniGame, AcceptMiniGameInvite, RejectMiniGameInvite, IgnoreMiniGameInvite, RequestMiniGameColorList, CreateMiniGame, EndMiniGame, ResetMiniGame | G:21380-21901 | `Command::MiniGame`, `bri-minigames` | matched |
| SAD, SADSetPassword | M:951, M:1029 | `AdminAction::Login`, `SetPassword` | admin-ranks lane |
| MessageSent, TeamMessageSent | M:1102, M:1037 | `Command::Chat`/`TeamChat`, `session.rs:1767` | matched (length, E-Tard filter). Open: v20's "Do not repeat yourself." warning for a repeat within 15 s and its URL links are not implemented. Different: v20's team chat cuts the first three characters of a long line (`getSubStr(%text, 3, ...)`, M:1051), a bug not copied |
| MissionStartPhase1Ack/2Ack/3Ack, BlobDownloadFinished | M:1548-1616 | the native join handshake (`bri-net`) | different: replaced by the native protocol |
| OpenPlayerList, ClosePlayerList | M:1774, M:1785 | the player list is replicated always | different: nothing to open |

## Datablock flags

| Flag | v20 | Ours | Status |
|---|---|---|---|
| `indestructable` (spawn point, vehicle spawn) | G:16386, G:16399 | `convert/src/catalog.rs:288` → `Definition::indestructible`; owners may hammer their own (branch 17797c1) | matched |
| `specialBrickType` Sound, SpawnPoint, VehicleSpawn; `brickType` | G:16369-16399 | special bricks, `crates/sim/src/session/special.rs`, spawn points, vehicle spawns | matched |
| `printAspectRatio` | G:16299-16360 | print aspects in the tool catalog (`tools.rs` `print_aspect`) | matched |
| `canCover`, `orientationFix`, `collisionShapeName` | G:15751-15966 | converter (`convert/src/catalog.rs`), `client/src/content.rs:555` | matched |
| `uiName`, `category`, `subCategory`, `iconName` | throughout G:15196-16399 | brick catalog and selector | matched |

Vehicle, projectile, item and emitter datablock fields are converted by the
import crates (`vehicles-import`, `weapons-import`, `fx-import`) and checked
by their own tests; this audit did not re-derive them.

## Evidence

`cargo test -p bri-sim --test v20_events --test tools --test hardening_session`
and the full `cargo test -p bri-sim` at hand-off (see `docs/progress.md`).
