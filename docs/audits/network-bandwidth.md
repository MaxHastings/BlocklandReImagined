# Network bandwidth audit

Branch `claude/bandwidth-audit-9j7dwx`, 2026-09-28. Protocol 45 (Gate
renumbers on landing).

Max's rule (2026-09-28, 14:37Z and 14:38Z): never spend bandwidth on things
that are cosmetic and heavy; keep the network light for what matters.
Knocked-out brick physics is a client-side cosmetic like particles.

## Result

Real QUIC on loopback, 4-second windows after a 1-second warm-up, 60 fps
clients sending input as the game does (two 120 Hz inputs a frame, six-deep
redundancy). "Host" is the payload the host hands QUIC, summed over every
player; "down" and "up" are one player's bytes on the wire, QUIC and UDP
headers included.

| Scene | Host before | Host after | Down before | Down after | Up before | Up after |
|---|---:|---:|---:|---:|---:|---:|
| Idle freebuild, 1 player | 12,948 B/s | 1,452 B/s | 15,259 | 3,014 | 32,422 | 13,009 |
| Idle freebuild, 8 players | 653,288 | 13,896 | 86,255 | 3,453 | 33,427 | 13,151 |
| 8 players running | 657,208 | 136,596 | 86,289 | 19,549 | 33,432 | 13,262 |
| 8 players building (5 plants/s, 10 Hz ghost bricks) | 675,388 | 166,160 | 89,154 | 23,748 | 34,065 | 15,432 |
| Rockets knock 282 bricks out of a wall, 7 watching | 674,276 | 31,508 | 88,943 | 5,905 | 33,473 | 13,219 |
| 8 players firing rockets | 849,496 | 45,670 | 111,285 | 7,830 | 33,560 | 13,339 |

The "before" building row had no ghost-brick reports (protocol 41 had not
landed); the after row includes them. Join downloads did not change: a flat
build of plain plates costs 6.1 B a brick for 10,000 bricks and 5.9 B for
100,000 (MessagePack then zstd). Real saves vary more: Golden Gate Bridge was
12.6 B a brick (`docs/networking.md`).

For scale, v20's Broadband network setting was 1023-byte packets at 32 a
second each way (about 32 KB/s per client), and Dial-Up 240 bytes at 16 to
20 a second. Every scene above now fits under Broadband for each player.
Eight idle players used to need 5.2 Mbit/s of the host's upload; they now
need 0.11 Mbit/s.

Reproduce:

```sh
cargo test -p bri-net --release --test bandwidth -- --ignored --nocapture
```

The host counts what it sends by kind (`ServerHandle::traffic`,
`crates/net/src/traffic.rs`), so the table also prints each kind's rate.

## What travels, and what it is

Classes: **authoritative** (the host decides and every client must agree),
**derivable** (a client can compute it from what it already has) and
**cosmetic** (no effect on play).

Max's test for "cosmetic" (14:40Z): cosmetics are easily mistaken for things
that matter. If two players seeing something differently could change what
either can do or what happens (collision, blocking, hits, damage, scoring,
triggers, events, minigame state, anything a script or event reads), it is
gameplay and stays synced. Only what is purely seen or heard, and read by
nothing else, may go local. When unsure, it stays synced and only gets
cheaper (rate, quantizing, packing). Every cosmetic or derivable call below
says why. No gameplay state moved to the client in this audit: every change
sends the same host state less often, smaller or only when it changes.

| What | Channel | Class | Before | Now |
|---|---|---|---|---|
| Own pose (`Pose`) | datagram | authoritative (prediction reconciles with it) | 40 Hz, named fields, 245 B | 40 Hz while it changes, 10 Hz while still; compact, 91 B |
| Other players' poses | datagram | authoritative position, velocity and look (hits, collision and look-driven events read them); jump timers, energy and input ack derivable, since only the owner's prediction reads them | own `Pose` to everyone | `RemotePose`: no jump timers, energy or input ack; velocity in cm/s and look in 1e-4 rad as i16; 52 B. Sent while changing, 3 intervals after settling, then 1 Hz |
| Vehicle poses | datagram | authoritative | 40 Hz each, parked or not | same settle and keepalive; parked vehicles cost nothing. 129 B a driving four-wheel pose |
| Admin camera orbs | datagram | kept synced: it only shows where an admin looks, but it is cheap and admins act on what others see, so unsure means synced | 40 Hz | settle and keepalive |
| Datagram packing | datagram | | one pose per packet (about 55 B of IP, UDP and QUIC each) | items packed up to 1100 B; one packet per interval per player |
| Movement input (client to host) | datagram | authoritative input | named, 6 inputs, every rendered frame (120/s at 144 fps) | compact; at most one datagram per 14 ms; a held frame's inputs ride the next |
| World update (`Delta`) | reliable, zstd | mixed | 20 Hz even when empty (157 B) | absent fields left out; empty updates at 10 Hz (54 B), which clients' respawn countdowns and item fades read |
| Bricks planted, changed, removed | update | authoritative | whole brick | unchanged (see below) |
| Knocked-out and killed bricks | update + `BrickKill` cue | brick state authoritative (it collides, blocks and events read it); debris cosmetic because it can never move, block or slow anyone and nothing reads it (Pushable physics bricks, 66ee059) | brick flags + one cue with its look; clients throw debris locally | unchanged: already one event, local debris |
| Knocked-out brick rigid bodies | none | cosmetic: client-only, pushed one way by players and never pushing back, so no two views can disagree about play | never synced | stays client-side (Pushable physics bricks thread) |
| Projectiles | update | authoritative (they hit, damage and knock bricks out); the flight between spawn and impact is derivable because clients run the host's own step (`bri_weapons::coast`) from the host's state, and the host resends any projectile whose real flight leaves that path (bounce, stick, redirect), checked every 50 ms | whole list with positions, 20 Hz, while any flies | `WeaponDelta`: sent when they appear or leave their coasted flight (bounce, stick); clients coast them with `bri_weapons::coast`, the host's own step |
| Held images, static items, dropped items | update | authoritative | whole weapons view when anything changed | per-owner image changes; static items and drops only when they change |
| Vitals (incl. ghost bricks), inventories, avatars | update | authoritative | whole map when any player changed | only the players that changed |
| Add-On package entities | update | authoritative | whole list, 20 Hz, while any moved | new or changed entities in full; moves as id, position, yaw; removals by id |
| Names, minigames, vehicles list, palette, time scale, broken shapes | update | authoritative | on change | unchanged (rare) |
| Chat | update | authoritative | new lines only | unchanged |
| Cues (sounds, effects, animations, debris, pain) | update | cosmetic and one-shot: only seen or heard; the damage, deaths and brick changes behind them travel as authoritative state | one event each | unchanged: already one-shot with local presentation |
| Notices, replies | reliable | authoritative | per event | unchanged |
| Package state | reliable, per viewer | authoritative | on change | unchanged |
| Admin snapshot | reliable, per player | authoritative | to every player on each join, leave or admin change | unchanged (rare; see follow-ups) |
| Welcome and map change | reliable, chunked | authoritative | MessagePack + zstd chunks | unchanged; checkpoint adds `projectile_falls` |

Nothing cosmetic is streamed continuously any more. The remaining steady
cost is movement: other players' poses while they move (the running row),
own input upload, and driving vehicles.

## Rates, compression, quantization, culling

- **Rates.** Poses 40 Hz (v20 sent up to 32 packets a second on Broadband).
  World updates 20 Hz, 10 Hz when empty. Still items: 3 intervals after they
  settle, so one lost datagram never leaves a stale resting pose, then 1 Hz.
  When a still item moves again, the host first resends its resting state at
  the previous interval, so receivers interpolate from where it rested.
- **Tolerances.** A pose counts as still within 1 mm, 1 mm/s and 1e-4 rad,
  so physics float noise does not keep it streaming.
- **Compression.** Reliable frames are MessagePack with names, then zstd at
  level 1; names cost little after zstd. Datagrams are compact (positional)
  MessagePack, uncompressed; datagram variants have one-letter tags.
- **Quantization.** Only other players' velocity and look angles, which only
  drive drawing. Own poses, vehicles and projectiles stay f32: the owner's
  prediction and the coasted projectiles must match the host exactly.
- **Culling.** None by distance. Every player gets every moving player. v20
  scoped ghosts and prioritised by distance under its packet budget. With 8
  running players each player receives 19.5 KB/s, under v20's Broadband cap,
  so distance culling is not needed for alpha. See follow-ups.

## Coordination

- **Pushable physics bricks** (`claude/pushable-bricks`) owns knocked-out
  brick physics and keeps it client-side, per Max. The host sends one
  `BrickKill` cue per brick with its look and blast; the rigid-body motion
  never crosses the network.
- **Smooth moving objects** (`claude/smoothing`) owns client interpolation.
  Projectiles: the host sends a projectile when it appears or leaves its
  flight, `Checkpoint::projectile_falls` gives each falling definition's
  per-tick drop, and `bri_weapons::coast` is the exact step. bri-net's
  `Replica` coasts projectiles to each update's tick and then applies the
  host's corrections, so its weapon view is the host's state at that
  update's tick. That fits the smoothing client (2c4aa3f3): each coasted
  entry reads as a zero-size correction at its update's tick, a bounce
  arrives as a real one, and a projectile the host drops (`removed`) leaves
  the view in the same update. While any projectile flies, updates stay at
  20 Hz, so every update carries its tick.
- **Ghost bricks and rider look** (protocol 41) are measured in the building
  row: ghost reports ride `Vitals`, now sent only for the builder whose ghost
  moved.
- **Duplicator blueprints** (protocol 42) are a private `Notice::Blueprint`
  to the player who copied and a `Command::PlaceBlueprint` upload: one-shot,
  on use, and not streamed.
- v20 fidelity: nothing v20 synced for gameplay was dropped. Positions,
  vehicles, projectiles' spawns and impacts, brick state and chat all still
  come from the host.

## Budgets

`crates/net/tests/bandwidth.rs` runs three scenes in every test run and fails
when one gets heavier:

| Test | Measured | Budget |
|---|---:|---:|
| `an_idle_server_stays_under_its_byte_budget` (8 idle players) | 13.9 KB/s host, 136 poses/s | 30 KB/s, 300 poses/s |
| `an_explosion_stays_under_its_byte_budget` | 31.5 KB/s host | 70 KB/s |
| `a_rocket_fight_stays_under_its_byte_budget` | 13.3 KB/s of world updates | 30 KB/s |

A budget may rise only with the reason recorded here.

## Follow-ups (not done)

- **Distance relevance.** Send far players' poses less often. Needs
  per-viewer send records so a far viewer still gets each resting pose.
- **Vehicle pose quantization.** A driving jeep is 129 B at 40 Hz; wheel
  suspension and spin as i16 would save about a quarter.
- **Knocked-out brick records.** A fake-killed brick resends its whole record
  twice (out and back 30 s later) alongside its `BrickKill` cue. A presence
  patch would halve an explosion's cost, which is already small (282 bricks
  cost each watcher about 2 KB/s over 4 s).
- **Brick explosions stopped at the first 64 bricks by id** (fixed in
  7208d183). In `session/events.rs` the `take(64)` ran before
  already-knocked-out bricks were skipped, so a second rocket into the same
  spot knocked out nothing. The 64 now counts only bricks a blast knocks out,
  and, as in v20, a direct hit knocks out only the brick it struck while the
  explosion searches the radius, so a rocket takes at most 65.
  `brick_damage.rs` covers it without content.
- **Admin snapshots** go to every player on each join and leave (N² at a
  busy join). Rare, so left alone.
- **Fire-and-forget reports** (ghost brick, brick hand) get a reply each,
  about 0.5 KB/s per builder, because the client times out unanswered
  requests.
