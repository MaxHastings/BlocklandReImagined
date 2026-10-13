# 2026-10-13 Modding building blocks for v0.3.0

Maxwell asked to bring forward the modding hooks first planned for later,
the things Gmod, Roblox and v20 modders lean on. They were built in his order
of priority, and then three classic mods were ported onto them to find what
was still missing.

## What landed

- **Add-Ons extend each other.** An Add-On lists `exports` (each
  `api_<name>(from, args...)`) that dependents call with `call_addon`, and
  `events` that it `emit`s for Add-Ons that list them in `listens` to hear
  with `on_addon_event`. A call runs in the callee's sandbox (its state,
  capabilities and, since the roleplay port, its own host data, settings and
  data files via `World::as_package`) and joins the caller's transaction, so
  a failing caller keeps nothing. Chains are at most 3 deep and never
  re-entrant. (`crates/sim/tests/addon_interop.rs`)
- **Saved data across restarts.** `set_host_data` (`storage`) is written to
  `<state>/addon-data/<add-on>.json` on every host (Maxwell said yes,
  relayed by the coordinator). `player(p).uid` gives rules a lasting key to
  file a player's data under. (`crates/net/tests/host_setup.rs`)
- **`on_object`.** Created, removed and changed (owner or driver) events for
  vehicles, bots, Add-On creatures and dropped items, whoever made them,
  from a per-tick diff. A game without the hook pays nothing.
  (`crates/sim/tests/object_hooks.rs`)
- **Collision hooks.** `set_pass_group` (groups 1 to 15) lets chosen
  bodies pass through each other, carried in `PlayerState` and
  `RemotePose` flags so prediction agrees with the host. The filter is
  `bri_motor::player::moves_against`, shared with the portal collision
  thread as asked. `on_contact` also hears two vehicles crash (`kind:
  "vehicles"`). (`crates/sim/tests/pass_groups.rs`, `vehicle_crashes.rs`)
- **UI panels.** `show_panel`/`close_panel`/`on_panel`: text, lists,
  fields and buttons, built from the game's v20 widgets, validated on both
  ends. Maxwell chose option B: lists can also drag rows between each
  other (`drag: true`), and the drop reaches the rules with the row's text
  so they can check it is still there. (`crates/sim/tests/panels.rs`,
  `crates/ui/tests/addon_panel.rs`)
- **Camera and input, sandboxed.** Client code with `camera.control` places
  the camera in a `camera_frame` export; `input.capture` claims game
  controls (never menus, chat or pause) and reads them instead. Both change
  only what that player sees and which keys the game hears.
  (`crates/client-sandbox/tests/sandbox.rs`, sample
  `sample-overhead-camera`)

Kept out, as asked: engine overrides, file or web access, welds, voice.

## The ports

- `sample-rp-bank` and `sample-rp-inventory` (roleplay): a bank exporting
  `balance`/`pay`/`give` with money kept in host data by `uid`, and a
  backpack, shared chest and shop panel. Items drag between backpack and
  chest; everyone with the chest open sees it change.
  (`crates/sim/tests/roleplay.rs`)
- `sample-wire-lite` (Wiremod): bricks become switches, and/or/not/xor
  gates and lamps; wired by aiming or by dragging one gate onto another in
  a circuit panel; an empty-hand click flips a switch and the lamps show
  the result in their colour. (`crates/sim/tests/wire_lite.rs`)

Gaps the ports found, fixed here: no lasting player id (added `uid`); an
exported function read the caller's host data instead of its own; and
`host_data` did not read back writes made earlier in the same call chain,
so a bank paid twice in one call lost the first payment.

## Checks

`python tools/check.py focused --base b857f94` (format, clippy `-D
warnings` and integration tests of the changed crates and dependents). The
GPU tests in `bri-client-sandbox` need an adapter this container lacks, as
before this branch; `bri-ui`'s `screen_sweep` synthetic case already failed
on the base.

## Next

- Maxwell's playtest: the backpack and chest window (drag feel, the
  dragged row following the cursor) and the overhead camera key.
- Gmod-style constraints (welds) stay out of scope, as do voice and file
  or web access.
