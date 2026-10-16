# Authored seat held-image composition

A seat can opt into owning the ordinary ready layer for an explicitly covered
mounted-image set. It does not change clip priorities or infer policy from a game,
seat name, Both hands, or preserve_hand_pose. Legacy seats retain their layering.

```json
{
  "rider_pose": "package_passenger_clip",
  "rider_ready_policy": {
    "contracts": [
      {"pose": "both", "images": [{"slot": 0, "image": "package:image/rifle"}]},
      {"pose": "right", "images": [{"slot": 0, "image": "package:image/rifle"}]}
    ]
  }
}
```

The example IDs and clip name are invented. A package must recover and bind its
actual applicability and aim/action crosswalk from original evidence. This
mechanism and a neutral synthetic pose do not certify original passenger aim.

Each contract names an exact whole active mounted-image set and a resolved body
ready role: right, left or both. Image order does not matter; stable IDs match
exactly, including case. Extra secondary/worn/attachment images make a contract
unmatched unless explicitly listed. The native mounted-image slots are 0..3
(weapons::IMAGE_SLOTS=4), distinct from the model's mount-point node numbers.
A cross-crate regression guards the schema's mirrored slot domain; there is no
new weapons dependency in the vehicle library.

Policies allow 1..16 contracts and 1..4 unique slot declarations per contract.
IDs are nonempty, at most 256 UTF-8 bytes, without control characters or leading/
trailing whitespace. Duplicate slots and duplicate image-set/role contracts are
invalid, even if declarations are reordered. Separate roles for the same image
set are distinct contracts, not an implied union into Both. Empty/oversized
policies, invalid slots/IDs and policy without rider_pose reject validation.
These are conservative native work bounds, not original-game content counts;
required original coverage must be implemented, not waived if a budget needs
revision supported by evidence.

The reusable vehicles::schema matcher returns a three-role eligibility value for
the exact image set. covers_resolved uses the body's existing RoleClips fallback
provenance. For example, a body requesting AimBoth but actually using AimRight
needs an explicit right contract; a both-only contract cannot claim that fallback.
No fallback is changed or guessed from a clip filename.

Avatar composition additionally requires the selected seat clip to exist on the
current body and be the active mounted sit/root clip. Standing, dead, missing
clip/body changes, absent policy, empty sets and unmatched images keep existing
layering. The only omitted layer is ordinary held-ready. Authored actions,
gestures, look and seat-aim layers, transitions, support/seat IK, preserve-hand
policy, hand-action deltas and clip priorities retain their existing semantics.
Those independent stages can still move hands; this is not an instruction to
freeze the whole pose. In particular, a source aim law cannot be replaced with a
neutral seat snapshot.

The app supplies current mounted seat and images from its authoritative view,
excluding dead riders and destroyed/broken mounts as for rider_pose. It does not
store a stale per-character exemption. Matching and fallback selection have no
client/rendering dependency and are reusable by the later authoritative pose
consumer. This slice does not change fallback hip anchoring, host muzzle/hand
origins or ray rules, wire data, or the schema's existing version-7 policy.

Root owns integration and the next anchor/authority reservations. Halo owns the
original clip/held-image/aim applicability, package binding and original/native
comparison at the exact engine SHA. Required closure includes source poses across
covered held states, actions and aim, unchanged unrelated tools/default seats,
then corrected authoritative mounted Session and obstruction coverage in the
following slice. Maxwell owns all interactive passenger aim/camera playtests.

## Explicit fallback hip point

A seat with rider_pose may additionally author rider_fallback_hip: [x, y, z].
The point is in the unscaled native seat-node frame: +Y up, forward -Z. It is
relative to the same actual seat origin used for the rider's model root, before
passenger turn. Each component must be finite and within [-64, 64], as for seat
eye bounds; an anchor without a selected rider_pose is invalid. No source-game
point is inferred from bounds, model names or screenshots.

The static seat path transforms this point by the existing rendered seat position
and rotation and the vehicle's scale once. The animated mount path transforms it
by the full posed world mount-node matrix, retaining its vehicle and node scale
without multiplying vehicle scale a second time. Existing seat origins/transforms
are unchanged. The world hip target is independent of rider body scale and turn.

Only when the selected mounted sit/root clip is missing on the current body does
the client convert that world target through the inverse rider rotation and body
scale, then subtract the sampled fallback hip in rider model space. The body that
owns the selected clip keeps its authored model root and ignores the anchor.
Absent anchors retain the existing missing-clip hip-to-seat-origin behavior;
absent rider_pose retains legacy fitting. The target cache is rebuilt each frame,
dead/dismounted inputs exclude it, and a changed body resolves its own current
clip/hip. This does not change hand, eye or aim policy or authoritative origins.

Halo owns recovering and binding original seat/hip evidence and comparing the
original/native consumer at the exact committed engine SHA. Root owns the later
authoritative mounted-pose/Session work. Synthetic points do not close either.
