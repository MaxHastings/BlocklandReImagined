# 2026-10-01 Key 1 selects the first brick

Max reported that pressing 1 sometimes does not select the first brick in the
brick bar, so he had to scroll to it.

Cause: this was faithful v20 behaviour, not a port bug. v20 binds 1 to
`useBricks` (c:15331), not `useFirstSlot` (unbound in v20); 2-0 are the slot
keys. `useBricks` (c:4380) re-selected the *current* brick slot, so after
using any other slot 1 returned to that one, and pressing 1 while holding it
put the brick away (`directSelectInv`'s same-slot deselect). The v20 HUD hint
"Press 1 or 2 3 4 5 6 7 8 9 0 to use bricks" groups 1 with the slot keys,
which is what players (Max included) expect.

Change (deliberate pivot from v20): `HudModel::use_bricks` now selects the
first filled slot in the bar, through the same `direct_select_inv` path as
2-0, so a second press deselects like the other slot keys and the empty-bar
messages and instant-use fallback are unchanged. The command keeps its name
and bind, so saved controls, the HUD hint and the tutorial's "Press 1 to
equip bricks" need no change. What is lost: 1 no longer jumps back to the last
used slot; the wheel and 2-0 still do.

Guard: `models::hud::tests::use_bricks_always_selects_the_first_brick`
(fails on the old code at the first assert: slot 3 stayed selected).

Commands (cloud, `CARGO_PROFILE_DEV_DEBUG=0`):
- `cargo clippy -p bri-ui --all-targets -- -D warnings`: clean.
- `cargo test -p bri-ui --tests --no-fail-fast`: all pass except the
  offscreen GPU render tests, which fail here with "no wgpu adapter" (the
  cloud container has no GPU; unrelated to this change).
