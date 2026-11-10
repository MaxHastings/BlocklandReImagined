# 2026-11-10 PR 442 engine review and issue 428

Maxwell authorized autonomous engine correction, issue resolution, merge and
the next three-platform release, with no interactive playtest or further
publication approval. Full alpha acceptance remains open. Original v20 inputs
and the dirty main checkout are untouched.

Working branch: `codex/v045-engine-review`, reusing the clean, merged
`BlocklandReImagined-worktrees/voip-review` checkout. Current main is
`e68f1d345`; PR 442 head `9e2451967` was merged locally as `5c94b33c9` to
preserve contributor history. No remote merge or publication is claimed here.

Review corrections in progress: explicit state exports; per-recipient
presentation budgets and single-charge broadcasts; expiring camera steering;
reversible, admission-bounded unlit materials; explicit alpha-masked emission
over actual body paint; typed optional hand reach instead of a NaN sentinel;
authored aim-lock swing limits and consistent remote action aim; optional quiet
bot spawning; namespaced, checked UI images with authored layout; cancellation
of queued sound profiles.

Issue 428 uses `addon.json` to describe one authoring/distribution folder with
content and server components. It reuses the companion/transactional installer,
package IDs, independent hashing and server-only execution/distribution. See
the original `sample-co-located`, its end-to-end install test, the authoring
guide and `v045-migration.md`. No general scripting VM or new trust exemption.

Evidence so far: original PR package-runtime lib 50 tests passed. Package lib
68 passed after correcting a discovered `path::inside` bug that truncated a
new staged path at its first missing parent (the ZIP regressions failed on
the old implementation). The co-located checker/ZIP/install/enable/host/client/
distribution/disable test and unsafe-layout rejection test pass. Explicit
export privacy and existing interop integration: 12 passed. Client compilation
found additional consumers of typed hand positions; correction and rechecks
are in progress. `cargo fmt --all` exceeds Windows command length, so formatting
runs per crate. These are scoped results, not a full release pass.

RenderMan/flashlight/walkie folders and `.research/testing/render_stare_harness`
are absent locally and not attached to the PR conversation. Reproducible
original fixtures cover engine contracts; their exact private consumer behavior
remains unverified. Continue review and targeted regressions, then exact-source
release gate, platform CI and package verification before publication.

Follow-up: Maxwell explicitly added main-checkout reconciliation. The primary
checkout is now clean/current at e68f1d345; 84 original local files are preserved
in a SHA-verified external archive. Durable decisions and dispositions are in
`2026-11-10-main-checkout-reconciliation.md`. The single unmerged jeep test
commit remains on its existing branch. No original source or primary target
was deleted. The earlier preservation statement describes the initial boundary,
which this explicit follow-up expanded.

Current focused evidence: interop/presentation integration 13 passed; camera
controls 8 passed; rested-bot facing/explicit locomotion 2 passed, including
an assertion that ordinary rest still suppresses prior follow intents. Both
scene shader sources validate (3 shader checks total). The offscreen masked
emission regression passes ordinary and Dynamic lighting, fractional coverage,
actual body paint and the legacy opaque/unlit flag combination. Added native
light scale preservation and kept-offhand checkpoint admission/lifetime coverage;
checks are pending. Weapon runtime checkpoints now use alpha schema 5, with no
migration. Stock BLS import is unchanged.

Final scoped lint: `cargo clippy --locked --all-targets` for bri-package,
bri-package-runtime, bri-content, bri-weapons, bri-weapons-import, bri-client,
bri-render, bri-sim, bri-ui, bri-net, bri-addon-import, bri-motor,
bri-fx-runtime and bri-chaos with `-- -D warnings` passed. The earlier runs
found source errors that were corrected, including an additional vehicle grip
consumer; they were not unexplained passing retries. Weapon animation/identity
integration passed eight tests, including kept-offhand checkpoint/lifetime
and forged-image admission. Documentation checker passed 79 maintained files
and six generated guide layouts. Final camera/image/light/audio rechecks and
the extended release gate remain pending. Evidence is kept outside Git under
`E:/Downloads/BlocklandReImagined-Release-Evidence/v045-2026-11-10`.

Final focused rechecks passed: camera filter nine tests (eight camera plus
one existing vehicle match), same-frame sound stop, adjacent-holder flicker,
and eight screen-image schema/animation cases including PNG/JPEG checksums,
encoded dimensions, namespace isolation, duplicate keys and pixel budgets.
The scaled image-light integration initially failed exact float equality
after matrix rotation decomposition (one component differed by 0.0000007).
Cone comparisons now use a 0.00001 absolute tolerance while retaining the
exact expected direction/angle, positions, radius and paint checks; the
corrected test passed. Full per-crate formatting and history checks passed.
Public review is PR 443; publication and merging are still pending CI/gate.

Release input recovery verified all 100 original archive hashes against the
unchanged default catalog. Sources include the existing parent Games/.research
Tier+Tactical archive and primary content/Add-Ons drop folder; different
editions found by default search were refused. Copies and per-source hashes
are recorded in external pinned-originals.json. The four immutable v0.4.2
ambience OGG baselines and the approved outdoor WAV also match their registry
hashes. Bundle generation uses both recovered core scripts explicitly and
current importer sources, without previous-import or missing-content flags.
Final package unit 68, co-located integration two and interop 13 rechecks pass.
The isolated content upload packs exactly the 19 effective base packs; an
additional ui-pack-004 is retained only for old content test fixtures.
