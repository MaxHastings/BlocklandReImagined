# Skinned attachment reflection follow-up

Reviewing de6efb9b found that `VehicleAssets::metal_centres` still visited only
rigid model instances. Independently posed guns draw from a separate cache; their
metal must also contribute its current world-space centre for environment probes.
The query now includes the posed geometry and its normal/portal-carried instance
transforms. The independent-pose regression verifies each gun contributes its own
posed centre. No duplicate rest gun is drawn and texture pixels stay shared.

`python tools/check.py focused --crates bri-client --depth 0`: three steps/72s,
pass, including all-target clippy and 566 passing unit tests (76 content-dependent
ignored cases remain for the integrated gate). The private source gun muzzle
parity check also passed: 195 aim combinations, origins within .001 native units,
barrel directions within 1e-4, post unchanged. Initial private test syntax omitted
leading zeros on two float literals; corrected before the successful run.

Private core regeneration with the restored hierarchy also passed, including
source triangle/material/winding correspondence and output asset hashes. Its ten
core packages are diagnostic output, not the requested complete playtest. Source
conversion is checkpointed privately at 7a4c953. Existing installed playtest and
public release remain unchanged. Full effect, dynamics, environment, expansion
and release acceptance in the private execution plan remain open.
