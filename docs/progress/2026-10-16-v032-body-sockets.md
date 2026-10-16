# 2026-10-16 v0.3.2 shared body sockets, Stage C

Root received Stage B `1127ffdbdf1001f096cbe631117f70dd2968f017` and granted
Stage C separately on the same isolated branch. Production paths remain only
content/src/body_pose.rs and client/src/avatar.rs. Tests are inside their unit
modules plus the expressly granted existing client/tests/world_items.rs. Own
architecture/progress/seam rows complete this atom. No host/session/net/package,
manifest/dependency or other checkout changes.

## Before production delegation

Three native Avatar characterizations were added and run against unchanged
Stage B production. `cargo test -p bri-client --lib socket_`: three passed,
zero failed/ignored,15.13s build/.04s tests; `target/socket-before.log`.

- Foreign missing named tool action samples the actual reference ready/action
  sockets while leaving every foreign-body posed node identical. Fitted and
  authored-hand cases, nonzero socket offset, scaled/rotated/translated player,
  tool-only precedence without unacted nodes, clear/repose and first-person
  fitted fallback. A negative multiplication order demonstrably disagrees.
- An owned action during a live native locomotion transition uses the corrected
  unacted inverse; omitting correction conjugation demonstrably disagrees.
  Disabling unacted sampling remains explicit absence, not a guessed delta.
- An offset-only stand-in socket retains its interpolated seated frame and
  offset/pitch/tool order; swapping pitch/offset or using the raw model frame
  demonstrably disagrees. Missing nodes and nonfinite aim retain their old result.

Actual native Avatar outputs also feed real WorldItems in third/first-person
views. The hand mount already carries action in third person; the eye-offset
image receives its native action once in first person. The no-action comparison
distinguishes those cases. `cargo test -p bri-client --test world_items
native_avatar_socket_actions_reach_world_items_once_in_each_view`: one passed,
zero failed/ignored,12.28s build/.05s tests;
`target/socket-world-items-before.log`. Existing assertions were not changed.

## Extraction and after checks

Content now has BodySocketInput/body_mount_node/body_offset_socket,
body_mount_action, first_person_hand_frame and sample_reference_tool_action.
The reference helper takes a native rig, its two explicit hand indices, selected
native clip and played time; it returns only socket displacement. Avatar keeps
actual mount/for_mesh lookup, named/role fallback eligibility, clocks, tool-only
override precedence and first-person presentation selection. It still holds its
own body geometry, unacted state, roles and fitting caches. Eye/camera/gameplay
origin policy is untouched.

Preserved order: model * raised-or-corrected-node * offset * tool-action;
seated-frame * offset * aim-pitch * tool-action; and
inverse(unacted * correction) * posed * correction. Finite/nonidentity filtering
and mirrored/scaled first-person hand placement retain their original contract.
Out-of-range reference indices are absent operations through bounded node access.
No new validity filter is applied to ordinary socket matrices.

The same three native Avatar cases pass after extraction, three/zero/zero,
1m44s build/.34s tests; `target/socket-after.log`. A pure public-input test adds
out-of-range reference mount and first-person hand, singular/nonfinite action
basis, identity-action absence, mirrored fallback and the existing finite-only
first-person contract (zero scale is not newly rejected).

`python tools/check.py quick --crates bri-content,bri-client`: four steps154s,
client590 passed/zero failed/76 existing content-dependent ignored;
content101/zero/zero; both format checks pass. `target/socket-quick.log`.
Focused passed all11 steps in208s with both changed crates at --depth0 and
repeated explicit --tests flags for world_items, body_layer_plan,
seat_pose_composition, seat_fallback_anchor and gltf_body. Both all-target Clippy
checks use -D warnings. Client590/zero/76 existing ignored, content101/zero/zero;
content doc tests zero/zero/zero. Integration counts: world_items15/zero/13 existing
ignored (including the unchanged new native characterization), body_layer_plan
2/zero/zero, seat_pose_composition7/zero/zero, seat_fallback_anchor4/zero/zero and
gltf_body4/zero/1 existing external-file ignored. Log `target/socket-focused.log`.
No runtime assertion failure occurred or was waived; no existing assertion was
changed. Initial rg with a Windows wildcard path was rejected and corrected to
explicit file paths. `git diff --check` passes.

```powershell
python tools/check.py focused --crates bri-content,bri-client --depth 0 --tests bri-client:world_items --tests bri-client:body_layer_plan --tests bri-client:seat_pose_composition --tests bri-client:seat_fallback_anchor --tests bri-client:gltf_body
```

Root supplied immutable Stage1 `94bae5632d1a19d16d9c663e146bcc5034522d55`
and its separate required metadata fix `853d02f3c3b076cf28e9c59dac7ec6d7758555e9`.
Inspected those via pinned git show only, without merging/cherry-picking or
copying dirty shared files. They are not asserted to be public main/released.
Private AA PR54 `def522999e9e9413a9a2c947f5504a8eeb08995b` activation handoff
was also read by pinned git show. Its old-pin package-view identity audit is
static evidence, not a new replacement counterexample on Stage1. Re-evaluate
existing item/custody/mount/life identities before requesting another mechanism;
no guessed package counter or global kit grant is introduced.

No original fidelity, host pose/eye/fire, source timing/completion, replication,
client channel15 or release closure is claimed. Halo/AA are active; GMod stays
parked. Root owns consumer routing, dependency topology and the next production
reservation. v0.3.1 and both delivered Halo previews remain unchanged.


The architecture note now proposes a separate D1 reservation for retained host
rigs/bindings and read-only diagnostics, using Stage1's existing validated asset
owner and an atomic combined install. Exact requested sim/net/local-host paths
and tests are listed there; none were edited. Bind-only shapes currently omitted
from clock metadata and active-only views losing natural-ended final-key holds
are explicit boundaries, not fabricated support. Root reviews topology/API before
any next production edit. The broader Session pose diagnostic and original
consumers remain required after subsequent state/frame/pre-fire integration.
