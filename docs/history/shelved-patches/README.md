# Shelved patches

Historical note. Written 2026-10-20. Patches that were shelved in the cloud project folder and are **not** on main as of main `a2c2430`. Each was checked against main (same-file line match plus a search for the feature). None is applied here; they are kept so the work is not lost. Patches from before 2026-10-13 target older code and will need rebasing.

| Patch | What it does | State |
|---|---|---|
| `join-codes-shelved.patch` | A self-hostable rendezvous and relay service (`bri-rendezvous`) so a host gets a short code (like `K7Q-4MZ`) and friends join without port forwarding: hole punching over QUIC, with an end-to-end encrypted relay fallback, plus a router simulator (`natsim`) for tests. 16 files, about 1,400 lines, from 2026-09-28. | Not on main. `crates/net/src/invite.rs` only leaves a seam for it (`docs/architecture/hosting.md`). Still wanted for "a friend can join without router setup". |
| `kept-layer-reuse-wip.patch` | Mirror and window planes copy from the player's kept shadow layers instead of redrawing the bricks. 2 files, 97 lines, 2026-10-07. | Not on main. A portal-shadow performance idea; WIP. |
| `driving-fix.patch` | One line in `crates/sim/src/nav.rs` `sample`: only treat a floor ray as starting inside something when `distance > 0.0`. 2026-10-08. | Not on main. Small; needs a check that the bug still reproduces. |
| `wheeled-vehicle-flight-fidelity.patch` | v20 applies thrust, lift and wing forces to every wheeled vehicle, so a wheeled Add-On that authors them flies; the importer maps those to a flying family (`wheeled_flight_family`). 5 files, about 280 lines, uploaded twice. | Not on main; conflicts in 3 of 5 files. Needed for flying wheeled Add-On vehicles. |

Checked and already on main (so not kept): `locomotion-vehicles-wip.patch` (92% of lines on main), `ammo-counter.patch` (98%), `bundled-wrench-check.patch` (98%), `parked-addon-goal-grounding.patch` (the `ScoreRise` completion is on main), `import-followup.patch` (landed as the Add-On import feature), `release-yml-linux.patch` (main has `linux-release-asset.yml`), and the three `code-health/app-split-hand-commits` patches (the client App split landed).
