# 2026-10-01 The bundled Hookshot test waits for the gun to come up

The Gate ran `cargo test -p bri-addon-import --test grapples bundled` on
Maxwell's real copies at main b5d99c948. The Grapple Rope passed. The
Hookshot failed with "pulled toward the wall: at most 0", and failed the
same way on a rerun.

## Cause

The test, not the game. The real `hookshotImage` stays in `Activate` for
0.5 s (`stateTimeoutValue[0] = 0.5`). The stand-in had 0.1 s. The test
harness equipped the gun, waited 30 ticks (0.25 s) and tapped the trigger,
so with the real copy the tap landed while the gun was still raising. A tap
during `Activate` fires nothing, in Torque as here. The real script itself
ports correctly. Its import reads every 100, far 16, fast 50, near 15,
slow 30 and stop 5, and the port is applied (unlisted copy).

How this was checked: the stand-in folders with the original `.cs` files
from grapple-originals.zip swapped in, run through the bundle path. It
failed at the same line before this change and passes after it. The Grapple
Rope's original passes the same way.

## Fix

- `Game::new` in `grapples.rs` now steps until the held image reaches
  `Ready` before handing over, as a player waits for the gun to come up.
  It panics if the image never gets there. This waits on state, not on a
  tick count.
- Guard: the stand-in Hookshot's `Activate` is now 0.5 s, as the
  original's is. On the old harness, both Hookshot pull tests fail at the
  Gate's line.

Maxwell's in-game report (no pull, no rope) came from the missing host rules
in the release bundle, fixed in 771a5432. This entry only fixes the test
that checks that fix on his real copies.
