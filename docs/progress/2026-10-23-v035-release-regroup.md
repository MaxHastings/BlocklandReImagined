# 2026-10-23 Next release regroup

Maxwell authorizes finishing the rendering/loading cleanup, reconciling open
PRs and local work, integrating ready AA2 work into main in both repositories,
and cutting the next release. Target v0.3.5; v0.3.4 stays immutable. Lizzy's
RX550 black-world report remains open but is not an indefinite release blocker.
No claim of repairing her machine or reproducing that failure is required.
Maxwell retains all interactive playtesting.

## Deliverables and sequence

1. Finish the bounded engine correction: package-relative animation admission,
   ten curated original menu images, semantic package checks, current release
   documents, submitted GPU pixel diagnostics, consistent SDR presentation and
   truthful compiler diagnostics. Trace the pre-server Loading Map wait using
   worker task names/timings; optimize only measured redundant work.
2. Reconcile the six open engine PRs and local tracked changes against actual
   main. Merge supported implementations with their consumers/checks; identify
   already integrated or superseded work and preserve unfinished experiments.
   Old branch ancestry alone cannot distinguish a missing change from a
   squashed/cherry-picked implementation. Do not merge whole historical
   integration trees or commit original/generated assets to catch up history.
3. Verify one integrated candidate: targeted regressions, full gate and Windows
   CI, original public map rendering with packaged inputs on available DX12,
   Vulkan and software paths. Run the existing bounded Pipeline and Interdiction
   mission/consumer checks against matching engine/Add-On main revisions.
4. Build the public platform releases from a recorded clean main revision using
   the ordinary release workflow. Verify the actual archives: versions, matching
   companions, manifests, selected content, menu art, notices and absence of old
   release handoffs/internal development documents. Publish only the verified
   candidate. Ship a matching private Pipeline/Interdiction Windows playtest
   separately, with its exact revisions, full file receipt and short test card.
5. Update current release status/notes and the existing AA execution plan, then
   remove only demonstrably finished worktrees/build output. Preserve unrelated
   local changes and unfinished research rather than silently deleting them.

## Reconciliation starting point

Engine main b3afae290; private Add-On main 12d3bc102. The latest optimized
Interdiction preview already contains engine changes through 496b165ae and
private aeaa01b11; it is not v0.3.4. Recent main includes map-pass submission,
headroom/recovery, casing contact, narrow-passage bot yielding, map emitters,
breakable props, volumes and package lifetime improvements.

Open engine PRs: #207 log/resource fixes (animation correction overlaps the
current audit); #284 night vision; #286 ledge/climb seams; #287 Use release;
#289 bot stall diagnostics; #290 casing diagnostics. #284 and #286 currently
have conflicts. No private PR is open. Review actual PR diffs, not their dated
status claims. Night vision and ledge PRs explicitly disclose partial behavior;
merging them cannot establish finished source-game behavior.

Read-only inventory found 72 engine and 75 private worktrees. Four engine and
six private worktrees contain tracked edits, including this active audit.
Historical engine integration heads are often not ancestors of main despite
related implementations having landed separately. Most private heads are
already ancestors. Ignored inventories/diffs are kept under .research/v034-fixes;
original source bodies and generated assets remain out of the public history.

## Acceptance limits

The release includes integrated engine mechanisms used by AA2; original AA2
assets remain private. Pipeline/Interdiction are development playtests, not
complete fidelity claims. The existing aa25/PLAN.md and mission handoffs own
remaining lighting, invisible blockers, M249 alignment, NPC traversal/combat,
co-op interaction and round presentation acceptance. This release does not
silently remove those requirements or close the full alpha roadmap.

Current technical evidence: the first focused engine run passes (28 dependent
crates, 1285 s); quick checks, strict affected-target clippy and 21 Python checks
pass after correcting two compilation issues in new diagnostics. RGBA/BGRA,
sRGB/unorm storage probes pass on NVIDIA Vulkan, NVIDIA DX12/FXC and Windows
software DX12/FXC. A full Bedroom High offscreen capture shows world textures
and restored original menu background. This uses release-derived inputs and
unoptimized diagnostics, not the final packaged executable or an RX550.
The optimized candidate and final archives are still pending.
