# Sampler exact-oracle correction and tutorial print reproduction

The f554 candidate full gate failed and is not eligible for main. Session10705
exited1: build607s, content9s and Clippy160s passed;459 binaries ran, tests1361s
plus67s retries, total2050s. app_soak failed in parallel then passed alone. The
new absent_and_explicit_spherical_preserve_native_sampling case failed both times:
expected y=.5 versus actual .50000006. The exact oracle sampled key rotation but
omitted the existing outer identity-to-layer Slerp and normalization at weight1.
Absent/explicit policies themselves stayed bit-identical; production was unchanged.

Root corrected only that exact oracle to include the legacy outer composition.
No epsilon was raised, assertion removed or gate exemption added. Eight targeted
cases pass. Root content quick and focused(depth0 plus explicit interpolation
target) both exited0, logs artifacts/v031-sampler-oracle-{quick,focused}.log.
The next combined candidate still needs its own full gate and Windows checks.

In isolated v031-linux-wheel-fixes, an enabled headless UI regression reproduced
long tutorial completion clipping: synthetic text control height90 versus144
required. The production fix grows the ML child AND clipping dialog, preserves
its center where possible and keeps the print within the logical canvas. Enabled
synthetic and actual converted-content variants both pass at803x530 scales1/1.5
and1280x960 scale2, including long/short replacement, repeated frames, resizing
and expiration. Initial integer-clamp compilation mistake was corrected before
any acceptance claim. Expanded evidence target-print-expanded.log; scoped UI
quick/focused are running. No visible game or interactive input was used.

Inventory-after-pickup and paint-wheel reports are still under investigation;
this print fix does not claim to fix either. Neither17/18 goals nor the claimed
scale cause establishes another root cause without reproduction. New v0.3.2 lane
is isolated; current Stage1 handoff94bae is under bounded metadata containment/
identity review before root imports it. Automatic publication remains authorized
only after the scoped release is verified.
