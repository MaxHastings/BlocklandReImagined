# 2026-10-18 Map movers and ladders (AA doors lane)

Branch `codex/aa-doors-ladder`, on `codex/aa-train-1` (88b6dee5). Generic engine
mechanisms for keyed movers (doors) and climb volumes (ladders); AA data and
policy are in the private Add-Ons repository. Design: `docs/architecture/map-movers.md`.

What changed: `bri_content::mover` (keys, easing, the Unreal-style reversal),
`Scene::movers`/`climbs`, `bri_motor::climb` and `PlayerTuning::climb`,
`NativeMap::movers`, `Session::{set_map_movers, mover_goto, mover_stop, mover_views,
movers_near_player}`, `Checkpoint`/`Delta::map_movers` (protocol-changes file),
`movers` capability and script functions, `CollisionMirror::set_map_movers`,
`bri_client::map_movers` and `MapScene::movers`.

Tests: `bri-content` (8 mover), `bri-motor --test climbing` (6), `bri-sim --test
map_movers` (4): a shut door stops a player and an open one lets them through, a body
in the way holds the door and a script reverses it, touching, the client copy.

Not done here: the AA route test, sounds and the climb animation (private repo).
