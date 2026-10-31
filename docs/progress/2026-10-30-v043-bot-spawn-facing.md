# v0.4.3 bot spawn frame

Supplied v21 Bot_Hole `holes.cs` 1868–1870 applies the spawn brick's rotation.
The original ZIP identities and private evidence locations remain in the approved
release plan. Initial native bot construction used only the home position;
ordinary respawn explicitly selected yaw zero. A running brain also retained
its old aim across deaths.

Brick bots now derive motor yaw from the brick transform's actual forward axis.
The native quaternion angle has the opposite sign from motor yaw, so copying
that angle is insufficient. Initial embodiment and shared respawn start in this
frame; the brain takes the new body's facing before the spawn event. Rules bots
keep their existing game-owned spawn selection. Reset movement sequences remain
monotonic. This extends the existing bot lifecycle and adds no public seam.

The new authoritative-session regression failed on turn 1 with yaw zero versus
brick forward +X. It now passes all four rotations on creation, a forced new body
with the same brain, and brick reset with a rebuilt brain. It also retains the
existing sequence. `cargo test --locked -p bri-sim --lib brick_bots_inherit
--profile playtest` passes. `cargo clippy --locked -p bri-sim --all-targets
--profile playtest -- -D warnings` and scoped formatting pass after moving the
test module to the file's end as Clippy required.

Direct-consumer targets `pick_spawn_facing`, `prediction` and `v20_events` pass:
2, 13 and 10 cases respectively; two timing probes and two native-content variants
remain their existing explicit ignores. This is synthetic/session verification,
not native content, two-client late-join or Maxwell's playtest acceptance.
Navigation, search and physical stacking remain separate open acceptance items.
