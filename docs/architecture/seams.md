# Engine seams and who is building them

Add-Ons change the game through generic engine seams (see
[platform-principles.md](platform-principles.md) and
[../modding/torque-equivalents.md](../modding/torque-equivalents.md)). When
two threads need the same seam, one builds it and the other builds on it.
Twice in v0.1.11 two threads started the same seam before anyone noticed
(datablock reading and the ammo HUD; own-model loading and textured icons).

**Before you build a seam, look for it here.** If it is listed, build on the
owner's commit or ask the coordinator for it; do not start a second one. If
it is not listed, add a row in the same commit that starts it, and tell the
coordinator. Once a seam is on main, its row says "on main" and anyone may
extend it.

Owners are named by thread title.

| Seam | What it covers | Owner | State |
|---|---|---|---|
| Hit regions | `info.region` on damage: head, torso, legs | Bushido's Adventure Pack | in flight |
| HUD "holding" line | per-gun HUD text while an item is held | Bushido's Adventure Pack | in flight |
| Scope zoom | zoom, `hide_nodes`, `both_arms` | Kaje's Sniper Rifle | in flight |
| Image scripts | `Image.scripts` state-machine hooks | Butterfly Knife and Grenade | in flight |
| Voxel bricks and own models | `place_voxel`, `*.shape.json` loading, textured item icons, `mode.json` | Trench Warfare game mode | in flight |
| Gun behaviour | hitscan, spread, shake, `left_image`, `set_speed_scale`, projectile bounces, children and aura, damage direction, Add-On settings, HUD format, magazines, explosions | Tier Tactical weapons | in flight |
| Classic Add-On loader | loading imported originals, datablock reading (Discovery injected) | Tier Tactical weapons | in flight |
| Host rules companion | rules that ship beside a weapon or tool | Fill Can Add-On | in flight |
| Item looks | `item_appearance`, `looks.json` skins, reach while holding | Gravity Gun rework | in flight |
| Mounting and look limits | `mount_object`, `look_limits` | Modder's weapon hook ideas | in flight |
| Tethers | tether and winch | Grapple rope Add-On | in flight |
| Passages | `Brick::stretched`, `EffectsWorld::set_passages`, clip planes | Portal bricks Add-On | in flight |
| Seat-tagged moves | moves tagged with the seat they steer | Turret aim flickers for watchers | in flight |
| Importer base datablocks | base v20 bricks, projectiles and damage types without `--core` | Trench Warfare game mode | in flight |
| Client app split | `crates/client/src/app.rs` into modules | Code health audit | in flight, lands last |
| Registries, protocol changes, progress entries | how features add ops, messages and notes without editing shared lists | Tech debt hot spots | in flight |
| Brick values | `set_brick_field`, `brick_field`: values rules keep on bricks, readable by every Add-On | Capture the Flag (Slayer) | in flight |
| Drop key with empty hands | `Command::DropKey`, `on_drop_key` | Capture the Flag (Slayer) | in flight |
| Dropped item names | `Drop::name`, `name_drop`, name tags over drops | Capture the Flag (Slayer) | in flight |
| Image lights | `Image::light`, drawn at the mounted image in its paint | Capture the Flag (Slayer) | in flight |
| End-of-round report | `show_report`, `report_column`, `Notice::Report`, the client's Report window | Capture the Flag (Slayer) | in flight |
| Kept worn images | `mount_image(..., #{ keep: true })`: only the Add-On that put a worn image on changes it | Capture the Flag (Slayer) | in flight |
| Orbit camera | `orbit_camera` with a body that acts (Throwing) or is frozen (`watch`, spectating), `ControlObject::Orbit::body` | Capture the Flag (Slayer) | in flight |
| Rules bots | `add_bot`, `remove_bot`, `rest_bot`, `bot_tool`, `bot_kinds`, `bot_limit`, a player's `spawner`; capability `bots`; the engine brain roams, fights and respawns them, sharing the 16-bot cap with spawn-brick bots | Capture the Flag (Slayer) | in flight |
| Message box | `message_box(p, title, text)` (v20 `MessageBoxOK`), capability `chat` | Capture the Flag (Slayer) | in flight |
| Saved mini-games | a build keeps its saver's mini-game (settings, Add-On settings, teams, `per_minigame` state keys); loading sets it up again and sends `on_minigame` `loaded` | Capture the Flag (Slayer) | in flight |
| Copy jobs | big copy work a slice each tick (`CopyWork`, `cancel_copy`, `copy_working`), copy ghost subset, `IdMap` | Duplicators | in flight |
