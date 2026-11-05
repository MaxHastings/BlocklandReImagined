# 2026-11-04 Mouse-driver weapon action aim

Maxwell accepts preview.5 destruction and vehicle hand occupancy, but reports
unreliable personal gun direction with Strafe Steering off: forward, backward
or vertical shots. Strafe Steering on appears okay in his current playtest.
This blocks release. Accepted NPC and destruction behavior are left unchanged.
Maxwell subsequently finished playing and explicitly authorized checks/builds.

The client reliable command path used raw Controls yaw/pitch as ActionAim.
MouseDriver deliberately uses those fields for steering, while ride_view builds
the visible seat/head look separately. The first shot consumes the reliable
click aim; subsequent held shots used player state.forward. A correction only
to the click would leave those two sources inconsistent on mounted views.

The correction separates control turn from gameplay action look. Controls
samples action_angles from the composed seat/head or hull frame while preserving
on-foot body aim, observer isolation and visual camera-shake isolation. Reliable
commands use those angles. Mounted movement samples the same direction in
optional MoveInput::action_look; normal non-view controllers retain None.
ActionAim aliases one validated motor ActionLook type, so both transport paths
share angle limits and direction semantics. Peer::action_direction supplies the
ordinary host weapon frame, trigger-lease reacquisition and action fallback;
a reliable click still owns its first shot. Authoritative origin, permissions,
state-machine timing, damage and control steering remain unchanged.

This adds a movement wire field; mounted-action-look.md advances the counted
protocol version. Alpha peers must match. The worst-case datagram test includes
maximum action-look values and all 12 redundant inputs; no redundancy/size
allowance is weakened. Explicit complete input constructors in the vehicle
actor adapter, tutorial, performance tool and player-type fixture specify None.
No content-name condition, guessed offset or new Add-On setting is introduced.
This runtime dependency change invalidates importer generation receipts; strict
release inputs must be regenerated rather than relabeled current.

Evidence completed before packaging: client all-target cargo check; warning-free
all-target clippy for motor, sim, net, client and vehicles; 45 controls cases;
51 synthetic session cases (9 content variants ignored); independent host
continuous-fire regression; bounded/malformed action-look check; maximum
12-input datagram budget; net unit tests (100 pass, 3 explicit ignores);
player-type integration (8 pass). Controller currently has no library cases,
so its empty unit harness is not meaningful behavioral evidence. The existing
host/guest Tank seat/camera integration also passes. Logs live in the private
v044-release-prep-2026-11-04 evidence folder. The host fire fixture verifies
multiple shots after click aim is consumed, then changes aim while held.
It exercises a direction contract, not original gun-state fidelity.

The new offscreen App/QUIC case supplies an automatic straight projectile via
an ordinary server loadout hook and boards a car for host/guest, in both
steering modes. It compares projectiles with actual rendered view samples,
including subsequent held shots after turning the mouse again. The vehicle
continues to settle during transport, so the expected samples track each frame;
comparing all shots with the first frame's stale orientation was invalid.
Direction tolerance stays 1e-4. The fixture disables muzzle convergence and
velocity inheritance explicitly to observe the requested ray; real guns retain
those mechanics. The check passes in 39.31 seconds: 24 matched shots across host/guest,
both steering modes and three distinct mouse turns per context. The changed
App integration target also passes warning-free clippy.

Setup failures were corrected, not reported as flaky product passes: the
mutated weapon fixture initially lacked its matching presentation checksum;
a mini-game/respawn boarding setup failed before gunfire (waiting for grounded
respawn alone did not resolve it), so this aim case reuses the established
server loadout hook instead of coupling to mini-game setup. The Tank fixture
cannot exercise both steering roles; the car can. The case waits for the host's
steering preference acknowledgment rather than assuming the first mounted
snapshot already reflects it. A Windows default-text-decoding error stopped
an initial test append before editing; subsequent writes use UTF-8.

The warm playtest-profile build completed; its final source identity must be
rebuilt after commit before packaging. Strict Add-On regeneration is running
with the new importer, rather than accepting earlier generation receipts.
The replacement private playable folder, hidden packaged startup, manifest
verification and shipped Speedkart checker results remain pending.

The full release gate was stopped during workspace build when Maxwell started
preview.5. Its partial result does not certify this correction. Seven-job CI
and all three build-only native package jobs passed on earlier source
430c27cbb, which lacks this change. New platform CI and exact-source release
checks remain required before merge/publication. No public release/tag or held
destruction merge is authorized before final combined gameplay approval.
