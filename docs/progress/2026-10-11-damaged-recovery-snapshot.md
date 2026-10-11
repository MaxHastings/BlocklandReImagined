# 2026-10-11: a damaged recovery snapshot no longer turns recovery off

The client test for a damaged crash-recovery snapshot (left from the
engine-hardening lane) found a real gap.

**Before.** A snapshot left in the client's state folder that does not load
(cut short by a lost disk tail or an interrupted copy) was offered back as
usual, but Keep failed on it every time. When the player then hosted, the
game tried to keep it as a save, failed, and, so as not to overwrite it,
hosted *without* a recovery snapshot. That happened on every later hosted
game until the player chose Discard.

**After.** `recovery::make_room` frees the slot before a game is hosted. A
whole snapshot is kept as a save, as before. A damaged one is moved aside to
`recovery/hosted-world.damaged.build` (the latest only), and the new game is
protected again. Only a failure to write the save leaves the slot taken.

- Test: `bri-client` unit test
  `a_damaged_snapshot_is_offered_refused_whole_and_set_aside_for_a_new_game`.
  It cuts a 200-brick snapshot early (no readable head) and late (head
  intact). In both cases the snapshot is still offered, Keep refuses it
  whole and leaves it in place, and no save appears. `make_room` then sets
  it aside and frees the slot, a whole snapshot is still kept as a save, and
  Discard clears a damaged one.
- Checked in the cloud container, which can build the client once
  `libasound2-dev` and `libudev-dev` are installed:
  `cargo test -p bri-client --lib recovery` (2 passed) and
  `cargo clippy -p bri-client --lib --tests -- -D warnings`. Windows is
  left to CI.
