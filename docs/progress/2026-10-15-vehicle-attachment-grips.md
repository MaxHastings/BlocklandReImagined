# 2026-10-15 Attachment grip nodes and hand target transforms

Close offscreen source grip inspection exposed two shared mechanism gaps. For a
weapon model with no look clip, the renderer pitches its entire attachment, but
Definition::hand_hold only applied yaw. Its old regression encoded this mismatch.
Point targets now follow full attachment pitch/yaw in the authored mounting basis;
barrel-look models keep their pivot-based fallback. Other seats stay fixed.

Weapon seats may also name right_node/left_node attachment sockets. The client
uses weapon_node, the exact source path used for muzzle/ejection presentation,
including clip sampling and turret sway. Explicit missing nodes report once at
load and leave that hand unbound, rather than silently using a guessed point.

Inspection also found weapon_node and point hand target transforms omitted the
vehicle's scale. Their world positions now use the same scale as vehicle geometry.
This covers socket positions; it does not establish scaling of every effect size.
The new fields have a protocol-change note and a seam owner. No game-specific
names or assets are added to the public implementation.

The point-hold pitch regression passes. A client regression exercises animated
and whole-attachment paths, nonzero aim/sway, vehicle scaling, conflicting point
fallback and missing explicit nodes. Compilation initially needed an explicit
nested BTreeMap type before validating loaded node names; corrected.
Further checks and new close source-backed captures are pending at this entry.

Private source markers/standing gunner clip put hand targets about half a native
unit from previously estimated mesh targets. Private conversion binds the original
markers and named grip nodes. These are technical/source-backed corrections,
not a claim of exact rendered fidelity or Maxwell acceptance. Original source
pose/marker residuals, rider dynamics, seated rifle shot parity and full all-eight
packaged delivery remain open. No new visible gameplay was launched.

Updated evidence: named-node regression passed (0.05 s after 74 s build), quick
passed four steps (65 s), and focused passed six steps (86 s). Post-node close
captures are in artifacts/warthog-node-grip-inspection in the main checkout.
Down/up inspection confirms targets now follow attachment pitch, but exposes a
larger import loss: the private gun model has no clips or original vehicle node
hierarchy, and the whole support tilts. The source model has separate gun-mount,
gun, chain and barrel nodes with original aim/fire/inertia clips. Restoring that
data is the next step; the generic grip seam does not certify the flattened model.
