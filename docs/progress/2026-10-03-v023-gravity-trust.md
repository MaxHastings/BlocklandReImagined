# 2026-10-03 v0.2.3 Gravity Gun trust and spawned-body scope

Stability/creator lane audited existing authority read-only and handed root `/tmp/bri-v023-gravity-trust.patch` plus `/tmp/bri-v023-gravity-trust-tests.patch`. Root owns production integration and compute. No Cargo, game window/input, commits, content/original-install writes or additional agents.

## Ordinary full trust and the reported limitation

Maxwell reported Full brick trust but inability to move another player's vehicle/Steel Ball. No captured mini-game or source-brick context was supplied. The ordinary two-live-principal outside-mini-game path already grants this: `may_move` accepts BUILD or above through the actor's principal-based trust map, therefore FULL. Native object scan exposes that same result; hand pushes, `start_hold`, operation admission and periodic live hold revalidation use it. No package, content name or NPC exemption is proposed.

In a mini-game the existing documented movement contract follows vehicle damage policy. Full brick trust does not override different games, disabled VehicleDamage, or UseAllPlayersBricks=false for a guest-owned body (only the mini-game owner's objects participate). Those are concrete normal denial paths, not proof that the user was in one. Changing trust to bypass those rules would be a policy change rather than a cause correction.

A meaningful new test uses two non-admin transport-verified principals and ordinary FULL invite/accept commands, then the actual selected shipped Gravity Gun image's `WeaponTrigger`. Both synthetic wheeled crate geometry and the actual shipped Steel Ball are denied before trust, caught and physically lifted afterward, and released through ordinary live validation after demotion to NONE. Existing older trust coverage used a direct package command at BUILD level, so this covers the user's real hand-control/grant boundary. This test is expected to pass current production; its result is still pending.

## Concrete separate stale-group defect

An actual spawn brick retains its builder's group after that builder disconnects. The current canonical `brick_group_owner_for` resolves that absent group to a mini-game's owner when that owner has FULL trust; connected builder identities remain their own. Event/radius brick policy already uses that resolver. `spawn_vehicle_for` records the raw brick group in physical ownership, and `vehicle_damage_decision` instead constructs its policy target from that raw absent account. Thus the same authored spawn group can belong to the mini-game for brick rules while its linked live body is still outside it. The ordinary Gravity Gun's native permission check denies that body.

The proposed correction derives only a **linked spawn's policy owner** from its current source brick using the existing group resolver and source mini-game. Physical owner IDs, actual vehicle IDs/pose/occupants and independent package body ownership remain unchanged. The canonical `MinigamesWorld::can_damage` still decides scope and VehicleDamage. Actual gravity scan/hold, pushes, hammer admission and vehicle harm already call this shared decision path. This does not turn FULL trust into an unconditional game permission.

The real lifecycle regression plants Bob's Ball spawn with the existing fixture helper, grants FULL through ordinary commands, disconnects Bob normally, and lets Ann create an ordinary mini-game. It requests an actual native grab and observes lift. A separate loose Bob body remains denied because it has no linked spawn; configuring VehicleDamage=false then denies and ends the held Ball. No hold/trust/brain internals are injected. Trusted public setup brick editing proves the real grant covers the retained group; that setup helper is not claimed to test the ordinary paint tool or mini-game painting policy.

This distinct departed-builder case is concrete source-path proof, not attribution to the user's live-body report. A normal owned save/load follows the same raw-group spawn path, but no new save/load regression was added here. Boarding's existing `can_ride` takes only an owner ID and was audited without changing its separate use policy.

## Requested boundary verification

Apply test-only patch to unchanged production first:

```sh
cargo test --locked -p bri-sim --test showcase full_trust_grants_actual_gravity_gun_controls_over_another_players_bodies -- --nocapture
cargo test --locked -p bri-sim --test showcase a_departed_full_trusted_spawn_group_keeps_the_same_gravity_permission_as_its_brick -- --nocapture
```

The first should pass; the second should fail on `held_by == Some(actual Ball)` before the correction. Then apply only the vehicle policy correction and repeat the second, followed by the existing showcase suite/vehicle permission coverage as root judges appropriate. No test result is claimed yet. `rustfmt --edition 2024 --config skip_children=true` and `git apply --check /tmp/bri-v023-gravity-trust.patch` passed against current shared source at handoff.

## Root verification

The baseline ran the ordinary Full-trust/native-grab coverage successfully and failed the intended departed-builder boundary: two passed, one failed (`/tmp/bri-v023-gravity-trust-before.log`). Root applied only the canonical linked-spawn ownership correction. The complete showcase suite then passed 32/32 (`/tmp/bri-v023-gravity-trust-after.log`). This proves the distinct retained-group defect; it does not establish the user's current mini-game configuration or bypass any mini-game permission.
