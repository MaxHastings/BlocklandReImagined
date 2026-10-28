# 2026-10-27 Mast video landing correction

Maxwell's preview.2 recording, F:/NVIDIA/Blocklandreimagined-v0.3.7-destruction-preview.2-windows/Blocklandreimagined-v0.3.7-destruction-preview.2-windows 2026.10.27 - 23.19.29.01.mp4,
is 18.96s, 3440x1440 at ~30fps. Read it locally with OpenCV and inspected the
extracted frames (ignored artifacts/collapse-assemblies/video-review). At 8-10s
parts clip into the hull and landing assemblies kick apart/settle awkwardly.
At the end a rocket passes through settled visible rubble without reaction.
This invalidates preview.2's gameplay acceptance; passing member-count tests
did not establish a good landing. No interactive game was automated.

Code-backed causes: a box around a sparse group invents solid space around
sails/crossbeams, permitting false deep penetration on dynamic-body creation;
eight extreme corners can miss the middle of a broad sail over a narrow deck;
the old upward-floor-only hit filter ignores hull-side impacts during toppling;
and deleting sleeping bodies also deletes local shot targets. Maxwell explicitly
selected client-only rubble with a clear visual reaction, retaining authoritative
rockets continuing through it. No server rubble physics or fake gameplay blast.

Correction: merge actual original-member boxes only across matching full faces,
then use their compound union. Bound it to 64 primitive boxes per body and 2048
primitive admission slots, retaining geometry if physics cannot be admitted.
Now groups can collide with each other without their old empty box proxies.
Cache distributed real material samples, including large face interiors, across
64 spatial cells in addition to the original eight extremes. Prefer each cell's
lowest sample. Solid impacts include hull sides; shared candidate budget 4096.
Sleeping bodies retain targets and wake on shots. A minimum 1.5-unit/s chunk hit
jolt keeps grouping from making a projectile's visible response negligible;
ordinary small debris, blast radius/force and authoritative projectiles retain
existing behavior. Budget-evicted geometry still has bounded cosmetic limits.

Targeted tests: six section tests passed. New cases check an open collision
union keeps its gap, a sleeping chunk survives advancement then wakes/moves on
a shot, and a broad sail over a raised narrow deck. The latter's old eight-corner
negative control misses the deck; the new probes limit penetration to 0.25 units.
The first improved-contact fixture still penetrated 1.32 units, exposing both
cell selection and the floor-only filter; corrected rather than loosening the
threshold. Existing 1500-member/eight-body, late delivery and toppling cases pass.

## Preview.3 handoff, 2026-10-28

Engine commit 089c9400b0808874ecd9557c0a3c4b4755820e91 is pushed to draft PR #379.
Quick and focused tiers both passed all 710 client unit tests (78 ignored), and
all-target clippy passed. Six targeted section tests passed, including the
old-corner negative control, open collision unions and settled projectile hits.

Built client, server, importer, Add-On checker and runner with
BRI_VERSION=v0.3.7-destruction-preview.3 cargo build --profile playtest --locked.
This first profile population took 14m23s; keep its target/playtest cache for
subsequent iterations. Prewarm unchanged server/dependency crates while working
on client-only corrections; changing profile after the fix starts a new first
build in this worktree. No broad physics gate was restarted during this handoff.

The verified preview.2 snapshot supplied content, compiler DLLs, launcher and
credits; five new paired executables and updated playtest notes replaced build
outputs. Config normalization is permitted only for package enabled-state files;
other copied inputs were verified against their prior manifest hashes. Main's
live generated Foley fixtures were not used or refreshed; the concurrent jet
audio chat owns those. The original source game and accepted audio installation
remain untouched. Awaited packaged --check exited 0: 21 maps, 1396 definitions,
35 save pictures and 14 modern light sets. Existing optional Butterfly Knife
warnings remain. --version verified preview.3 and source hash 089c9400b.

Launch E:/Downloads/Destruction-preview-2026-10-27-r3/BlocklandReImagined-v0.3.7-destruction-preview.3-windows/Launch.cmd.
Client 98839040 bytes, SHA-256 7c632b4889e8df64b7fd800c89cf188bb6b383e0b0c66beae71fb057f9e692de.
All 13173 file hashes, 117 unique package identities/directories and final ZIP
CRCs passed; the copied root README version was corrected before final hashing.
The parent contains DESTRUCTION-PREVIEW-RECEIPT.json and the archive. Logs and
snapshot/check scripts are under ignored artifacts/collapse-assemblies/.

Maxwell owns the corrected mast/settled-pile playtest. The PR remains unmerged;
full gate and human acceptance are still required. After jet audio lands on main,
bring that optional sound schema into the physics branch before using main's
updated fixtures for the merge gate. This receipt establishes technical checks,
not a claim that the new recorded scene has already been visually accepted.
