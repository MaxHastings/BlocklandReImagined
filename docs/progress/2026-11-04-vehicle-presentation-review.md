# 2026-11-04 Vehicle presentation review (#433)

Maxwell authorized improving the contributor branch and merging only after
review and required checks. The review retains the intended steering, hand
holds, appearance and light-switch features, but replaces narrow or conflicting
mechanisms. No downloaded assets are committed.

## Decisions

- Brake, reverse throttle and jump are independent authoritative inputs.
  Brake no longer changes JumpHeld. Brake is replicated and checkpointed;
  checkpoint schema is 4 and an additive protocol-change record advances
  VERSION. Join admission rejects mismatched protocol versions. Alpha has no
  mixed-version fallback or save migration.
- Model-object appearances consume bounded named facts independently of lamps.
  Missing facts/objects/clips fail admission with diagnostics. A visual-only
  switch does not require a dummy light. Conditions and appearance data are
  source-independent; the vehicle adapter exposes a documented finite fact set.
- Vehicle lamps, native effect lights and mounted-image lights share cone
  geometry and full source transforms. GPU particles use the same attenuation;
  the FX light record is now 48 bytes, including its example shader.
- Body threads, look, steering and additive acceleration produce one pose for
  geometry, attachment frames, seat cosmetics and grips. Steering clamps at its
  authored endpoints. Every authored key participates in affected-node detection.
  Animated object channels and skins retain the existing dynamic-mesh path.
  Shared models keep definition-specific appearances and animation declarations.
- Grips use the actual body transform inverse, including scale and look turn;
  remove the extra seat-facing correction. Releasing a two-hand tool releases
  both seat holds. Strict scaled-target coverage also exposed an IK straight-arm
  bend-axis defect: choose an orthogonal axis and use atan2 for the current angle.
- Keep the declared on_vehicle_lights rule callback in the established authorized
  operation path. Rendering reads state; it does not execute rule policy.

See [native presentation bindings](../modding/native-presentation.md) for the
admitted interfaces and resource limits. Cosmetic seat-node deltas do not change
host-owned seat physics.

## Windows evidence

Executed in the reused v043-final-gate worktree, not the dirty main checkout.
Logs and external content are retained outside committed sources.

- `python tools/check_format.py`: pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
- `cargo test --locked -p bri-content --lib`: 178 passed.
- Client vehicle module: 27 passed, 3 ignored. Client avatar integration:
  54 passed, 25 ignored. Weapon effects: 17 passed, 1 ignored.
- FX GPU contract: 3 passed, including opposing spotlight directions and the
  48-byte GPU layout. First-person view integration: 2 passed.
- Vehicles library: 35 passed, 3 ignored; native integration: 37 passed,
  37 ignored. Network driver-input test passes every jump/brake/reverse bit
  combination through actual datagram encode/decode.
- `cargo test --locked -p bri-client --lib`: 785 passed, 79 ignored.
  Existing three sim vehicle unit tests pass; the added visual-only
  switch/dead-vehicle regression also passes its separate exact run.
- `cargo build --locked -p bri-client --bin bri-client -p bri-package-runtime
  --bin bri-addon-check`: pass. Three supplied Speedkart packages, privately
  adapted to the admitted schema, pass bri-addon-check.
- `bri-client --check content <private-check-state>`: exit 0, with 226 existing
  compatibility/fallback reports. This is startup/resource verification, not
  a claim that legacy content is fully supported.
- Explicit ignored `authored_vehicle_presentation_offscreen` with private
  BRI_REVIEW_VEHICLE_ASSETS/BRI_REVIEW_OUTPUT: pass. Five states rendered;
  inspected headlight off/on captures, steering endpoints and resolved grips.
  This establishes technical/visual evidence, not Maxwell's gameplay acceptance.

The supplied ZIP SHA256 is
`cfda83477af01d181601050e9215804645fa6b13b788d5324591584d7823fe9c`.
Original files are retained read-only in the private evidence directory. The
adapted copy changes only vehicles.json: lens_objects becomes visuals and the
combined brake activation becomes a condition; assets retain their bytes.

Intermediate failures were corrected rather than hidden: compiler/Clippy
errors during implementation, an unreachable synthetic IK fixture, the exposed
IK bend-axis defect, and an incorrect asset-root argument in the external probe.
A broad focused run overlapped a source edit and reached doctests with stale
compiled dependencies; it is not recorded as a full pass. Windows formatting
was run per package after a combined cargo-fmt invocation exceeded the command
line limit. No retry-only success is characterized as a clean flaky-test pass.

Final head `51798d555` passed the history/protocol check and all seven executed
platform CI jobs (Windows check and shipping binaries, Linux wheel routing,
four Linux test shards). The conditional Windows test job was skipped; local
Windows runtime checks are scoped above. Merged as `0fcf77f0d` on November 4.
The rocket-collapse and zombie-tower reports are separate
next-release work; no correction or interactive acceptance is claimed here.
