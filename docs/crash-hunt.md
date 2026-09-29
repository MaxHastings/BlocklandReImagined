# Crash hunt: fuzzing and chaos soaks

The a16 crash (a gun casing ejected inside a brick got a ray hit at distance
zero with a zero normal, normalized into NaN, and debris returned a fatal
error) was the kind of bug no scripted test reaches. `crates/chaos` looks for
that kind on purpose. Everything in it is a test crate: nothing ships in the
game.

## What runs

| Test | What it throws at the game |
|------|----------------------------|
| `session_chaos` | Six bots in one host session doing everything at once: planting inside each other, the walls and themselves; firing rockets and guns (casings) from inside bricks and straight down; spawning, driving and wrecking every vehicle family; admins loading overlapping builds, clearing, warping and changing time scale; joining, leaving, chatting (empty, 4,000 characters, control characters); and hostile values (NaN, infinity, bad slots). Every six ticks everything the host replicates is scanned for NaN; a contained step failure or a panic fails the run with the seed and the last 64 actions. Deterministic per seed. |
| `net_chaos` | The same bots as real QUIC clients of a real host on loopback, two of them administrators, joining and leaving as they go. No client may be dropped unasked, nothing a client receives (poses, vehicles, weapons, vitals, cues, the world) may hold NaN, the host may report no step failure, and a newcomer must still get in and be answered at the end. |
| `physics_fuzz` | Rapier driven the way the game drives it (bricks planted and removed, vehicles and mounts spawned, teleported and removed, collision refreshes between steps) in random order, with Rapier's debug consistency checks on. |
| `geometry_fuzz` | Aiming rays, weapon sweeps and box sweeps from anywhere (inside bricks, walls and sensors), in any direction including zero and NaN; brick collision shapes from degenerate, negative and NaN sizes. Every hit must be finite with a unit normal. |
| `net_fuzz` | Random bytes, and real server frames and datagrams damaged inside their compression, through every decoder; decoded messages go into a replica, which must refuse them or stay finite. |
| `save_fuzz` | A real save (bricks, owners, events) with values swapped for extremes, keys dropped and lists cut or repeated, loaded twice over itself into a running host. |
| `weapons_fuzz` | The weapons runtime against a world that answers legally but unkindly (hits at distance zero, normals along the shot, explosions centred on their targets, brick events that bounce or redirect by zero or huge amounts), and Add-On weapons imported from damaged scripts. |

## Running it

```sh
cargo test -p bri-chaos                       # the CI-sized run, about a minute
```

Longer hunts (Max's PC, or any machine left alone for a while):

```sh
# Many seeds, a minute of play each
BRI_CHAOS_SEEDS=64 BRI_CHAOS_TICKS=7200 cargo test --release -p bri-chaos --test session_chaos
# Ten minutes over the network
BRI_CHAOS_SECONDS=600 cargo test --release -p bri-chaos --test net_chaos
# Deeper fuzzing
PROPTEST_CASES=20000 cargo test --release -p bri-chaos
# The real game: generated content (slate and bedroom by default)
BRI_CONTENT=content BRI_CHAOS_SEEDS=16 cargo test --release -p bri-chaos -- --ignored
BRI_CONTENT=content BRI_CHAOS_MAP=v20/add-ons/map_bedroom/bedroom.mis \
  BRI_CHAOS_SECONDS=600 cargo test --release -p bri-chaos --test net_chaos -- --ignored
```

A failure prints its seed. `BRI_CHAOS_SEED=<hex>` replays exactly that run
for `session_chaos` (the network soak is timing-dependent, so its seed gives
the same bots but not the same interleaving). Proptest failures print the
shrunk input; add it as a named test beside the fuzzer that found it, as
`physics_fuzz.rs` does.

## Adding to it

- A new command or system: add it to the bots in `crates/chaos/src/bots.rs`,
  including a hostile variant.
- A new replicated value: add it to `check_replicated` (`local.rs`) and
  `check_replica` (`net.rs`). The NaN scan walks any `Serialize` value.
- A new decoder or loader: fuzz it with damaged real inputs, and push what it
  accepts on into the thing that uses it.

## Found and fixed so far (2026-09-28)

- Rapier's collision-only pass (`PhysicsWorld::detect_collisions`) left its
  persistent islands inconsistent when bodies were added or moved beside
  bricks: debug panics, and a release-build out-of-bounds index in Rapier's
  sleep scan with the zero-length-step workaround. Refreshes now go through
  `bri_physics::detect_collisions`, a microsecond full step holding kinematic
  targets.
- Rays starting inside a brick reported a zero normal in aiming, weapon
  sweeps and client building (the a16 crash class). `hit_normal` in
  `bri_sim::simulation` makes every reported normal a unit vector.
- The client accepted player poses with NaN energy, scale, head turn or jump
  normal.
- Once bricks covered every spawn point, the host refused every join and a
  map change failed. Players now spawn at a clear point when there is one and
  otherwise where v20 would put them, as respawns already did.
