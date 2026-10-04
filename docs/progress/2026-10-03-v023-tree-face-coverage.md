# v0.2.3 decorative brick face coverage audit — 2026-10-03

User reports the top face of the supporting brick disappears under the exact
Octo Tree brick. Extent initially varied between a patch and the whole top;
no lighting mode was implicated. Native content inspected here contains Pine
Tree but no confirmed Octo Tree identity. The demonstrated mechanism applies
to authored geometry generally; exact Octo Tree runtime reproduction remains
unverified.

## Source evidence

`crates/client/src/brick_cover.rs` credited full tangential overlap of logical
brick Bounds for each covering neighbour, summed overlaps, and hid the entire
tagged face once its authored required area was met. It checked palette alpha
and BLB COVERAGE flags but did not establish actual touching face geometry,
authored vertex alpha, source attachment cells, or unique coverage.

Native Pine Tree (`v20/brick/brickpinetreedata`) has6x6stud logical bounds,
22plate height, Bottom hides_adjacent=true/required_area4. Its five opaque
Bottom-tagged quads lie at y=-2.2 and quilt a central2x2stud rectangle; the
bottom attachment template contains only those four central occupied cells.
Canopy geometry and logical bounds extend well beyond that stem. Native mesh
file `d60e7be3b48d70d5b4c8205837518dad7770db10cb3a38ca98b721c87a46fb1f.brick.json`
SHA256 `1e7f0f060eb8faf8473f88d814b24cfdc26bffbf427764780f863d2086b31079`.

One tree on a native4x4F supporting plate therefore previously credits16cells
and can hide its whole top although the stem covers only4. A single tree
cannot hide a32x32F top under that rule (36<1024); a6x6 arrangement of native
trees has canopy bounds that collectively cover that base while stems leave
large exposed regions. Literal authored vertex alpha also chooses blended
render geometry despite opaque palette paint.

## Proposed correction and bounded work

Review-only production patch `/tmp/bri-v023-tree-face-coverage-production.patch`
proves actual opaque, axis-aligned tagged-face geometry on the touching plane.
It accepts a filled rectangular quilt, intersects it with rotated occupied
attachment cells, unions covered cells across neighbours, and requires the
whole actual target tagged rectangle to be covered. Sparse target faces cannot
be hidden by coverage elsewhere in their logical box. Existing visible,
undisplaced opaque-paint and per-face authored COVERAGE conditions persist.

Adversarial review identified unbounded repeated quad union in the first draft;
that draft was not applied. The revised proof immediately retains faces on
meshes with more than64 total quads, caches all six proofs once per immutable
mesh during a chunk build, and caps examined covering cells at16,384 per face.
Nonrectangular, disconnected, off-plane, translucent or incomplete unfamiliar
faces remain visible. This permits extra overdraw conservatively instead of
false hiding or unbounded geometry work. No tree identity or mode checks and
no global face-culling disable are added.

## Verification status

Rustfmt and `git apply --check` pass for the review-only combined patch.
Independent adversarial static review finds both reported issues addressed, with no new blocker. Candidate enumeration by the existing Index visitor is not capped; only new geometry proof/cell work is bounded, and completed/exceeded faces skip further cache/dedup work.
No production application or Cargo/render receipt yet. Root owns application
and serialized compute. New regression covers4rotations, off-stem1x1support,
reverse sparse target, coincident overlap, normal opaque full coverage,
authored alpha, off-plane tags,100,000quad fallback, and generated native Pine
geometry under native and renamed IDs plus a32x32F multiple-tree case.

Baseline-only test patch `/tmp/bri-v023-tree-face-coverage-baseline-regression.patch`
compiles against the old Covers fields. Final tests require the new per-build
cache field. Commands and failures will be recorded after root runs them;
no acceptance item is checked on source evidence alone.
