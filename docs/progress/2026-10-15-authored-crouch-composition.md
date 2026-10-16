# 2026-10-15 Authored crouch locomotion composition

Shared imported-body inspection found the dedicated legacy crouch thread is
sampled after the selected crouch idle/run/back clip. With absolute full-body
channels, it replaces the moving clip with its held final pose. Hand fitting
policy controls a different layer and cannot repair this.

Archetypes can now declare `crouch_composition: "locomotion"`; the resolved,
replicated look table carries the same typed policy. It suppresses only the
dedicated legacy crouch thread. Omitted/`"overlay"` retains stock behavior, as
do hidden/box-body stand-ins. Changing body policy resets the old crouch thread.
Ready, aim, action, physical stance, collision and camera logic are unchanged.
An incompatible authored ready clip still needs correct source mapping.

The synthetic client regression samples distinct two-frame absolute idle,
forward and backward clips, compares every posed node with the selected source
sample at a nonzero phase, checks legacy held-overlay behavior and the stand-in
fallback, then switches a live body back to the legacy policy. A package/session
regression checks authoring, resolution, serialization and the stock default.
These use no original game assets.

Validation:

- Client source check passes (34.98s).
- Targeted pose regression passes. Its initial synthetic fixture supplied one
  rotation for two frames; channel validation correctly rejected it. The fixture
  now supplies two rotations; runtime validation was not changed.
- Quick tier for content/motor/package-runtime/sim/client passes: 10 steps/192s,
  including 568 client and 403 simulation tests.
- `python tools/check.py focused --crates bri-content,bri-motor,bri-package-runtime,bri-sim,bri-client,bri-net --depth 0`
  passes: 18 steps/1025s, including network crossover/sized join, package/session
  integration, serialization, client units and clippy. Most time was dependent
  test compilation/linking. The final full integrated gate remains required.

The AA private reproduction owner must opt in and revalidate its own source
roles on the committed engine SHA. No AA checkout, captures or source fixtures
were modified or executed here. Halo is not blindly opted into this policy:
authored ready/aim layers require their own source comparison. This closes a
specific composition mechanism, not imported-body fidelity, playtest acceptance
or the full v0.3.1 release. The installed playtest remains unchanged.
