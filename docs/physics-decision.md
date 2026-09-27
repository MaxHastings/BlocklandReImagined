# Physics selection

2026-09-26: Maxwell clarified that Jolt is optional and prefers an appropriate
Rust physics library. This supersedes the goal record's initial Jolt preference.
Evaluate Rapier 0.36, pinned in Cargo.lock, as the initial implementation.

Rapier is independent of wgpu and provides collision queries, rigid bodies,
kinematic character movement and a ray-cast vehicle controller in Rust. Those
capabilities match the required scope. Character and Jeep feel still require
game-specific tuning and Maxwell's playtest; library support alone is not proof.

Inspected alternatives: rolt/joltc-sys 0.3.1 wraps Jolt 5.0 but omits character
and vehicle APIs. jolt-sys 0.1.5 hardcodes Visual Studio 2019 and Windows runtime
libraries. A dedicated C++ bridge is feasible but not justified at this stage.
The abandoned bridge build file was removed before any Jolt compilation.

Use fixed simulation ticks and server authority. Enhanced determinism is useful
for repeatable tests, not a promise of cross-platform lockstep networking.
Keep authoritative game/save data separate from physics handles and snapshots.
Player input generates desired motion; collision resolution constrains it.
Bricks remain authored static construction, without fracture or collapse.

Reference: https://rapier.rs/docs/user_guides/rust/character_controller/
