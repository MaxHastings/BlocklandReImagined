# 2026-10-22 Ordinary Pipeline fall exposes view-kick limit

The private full-map round consumer loads the assembled map and mode through
the same HostSetup/Session path as a normal host. Soldiers move, fight and
operate objectives through ordinary controls; no fixture geometry, teleport,
forced kill or objective advancement is installed. At tick 11417 it reported
`op.bounds` for a finite view kick with peak 1.0562757 radians. This came from
fall feedback while a soldier traversed the map, not rifle recoil.

The source-backed fall conversion can legitimately request peaks above one
radian. The generic operation validator imposed a one-radian cap even though
the client already handles larger kicks and clamps its resulting pitch to the
normal look limits. Admit peaks up to a quarter turn, retaining finite-value,
nonnegative-peak, direction, settle-offset and rate validation. No wire format
or game policy changes. The existing view-shake seam remains the owner.

The regression submits the exact rejected payload through operation
authorization, checks the quarter-turn boundary, and rejects negative,
excessive, infinite and NaN peaks. It passes. Quick checks pass (44 unit
tests and formatting). The first `cargo fmt --all` invocation exceeded the
Windows command-line length limit; formatting the changed crate succeeds.
Focused Clippy and the complete assembled round follow-up are in progress;
the final push uses the ordinary repository gate. Private evidence lives in
`aa25/.research/pipeline-round-flow-20261022/ordinary-round-before.log` and
`ordinary-round-after.log`. This correction does not establish full objective,
round-flow, rendering or source-fidelity acceptance.
