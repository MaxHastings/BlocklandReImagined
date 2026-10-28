# 2026-10-28 Destruction lifecycle and pipeline hardening

Maxwell approved fixing the lifecycle/resource findings, then explicitly asked
for merge after technical checks; interactive playtest is now after merge. His
latest preference remains cheap coherent destruction, preserved original art,
client-only rubble and a visible local projectile reaction, with no new toggles.

## Integrated correction

Simulation now owns a section journal beside original damage families. Planned
membership is shared with the producer; common edits interrupt it. Sealing
reports the actual committed prefix and starts shared restoration deadlines.
Several sections can reference one original without overwriting ownership.
Ready/Retired wire barriers plus packed transient source tags let the client
recover missing geometry and reject obsolete deliveries. Fall begins only after
the sealed material and local preparation exist. This is a streamed gameplay
commit, not a claim that all skyscraper bricks disappear atomically.

Admission precedes section removal and accounts for material, appearances,
unique and instanced mesh quads, spatial buckets, source payload and leases.
Occupied capacity queues a proof; an intrinsically oversized proof stays intact
until later damage/build changes. Traversal, source seeds, invalidation regions,
mutation estimates and descendant-weighted restoration have explicit envelopes.
No visual counter is passed off as a complete performance guarantee.

Client preparation/model upload/finalization are staged. Landing granularity
uses available body slots; disconnected/proxy overload retains a shared visible
assembly before allocation. A common lookahead collision window serves ordinary
debris and chunks, while bounded nearby edit history avoids pauses from remote
edits. Contact checks run ahead of the displayed clock. Hit queries cover
shared/frozen/preparing assemblies too, and use displayed projectile segments;
authoritative rockets still continue through cosmetic rubble.

Original WAN/LAN/minigame policy is preserved through one eligibility helper,
including the existing LAN exception used by imported local builds. No new
sound assets. Boxing the cold rule-region array offsets the new section pointer;
the 256-byte Brick memory ceiling and serialized rule array shape remain intact.
The small rule-region/bot/test edits are representation adaptations, not changes
to their gameplay policy. New protocol evidence is in
crates/net/protocol-changes/section-delivery-barrier.md.

Rebased the destruction branch onto accepted audio main 2b3e49404b. Only the
seam-table row conflicted; retained both the audio owner and destruction seam.
The dirty shared main checkout and accepted installation remain untouched.

## Verification record

- Initial all-target typecheck passed for client/sim/world/net/chaos.
- Quick first exposed the existing 256-byte Brick ceiling (264 after adding
  the section pointer); fixed representation instead of relaxing the test.
- Earlier client failures exposed frame-dependent ordinary collision readiness
  and an obsolete unsealed mast fixture. Readiness now consumes fixed-step
  credit; the fixture explicitly seals delivery. Targeted checks then passed.
- Latest quick: client 718 passed/78 ignored, net 90 passed/3 ignored, chaos 1
  passed. Sim 449 passed/one new test failed because its synthetic definition
  had no render quad. Corrected that fixture to use an actual test mesh.
- Focused initially stopped on a collapsible-if warning; corrected it without
  suppressing lint. Focused rerun passed: formatting, all-target clippy, all selected dependent integration tests and 718 client unit tests (78 ignored). The tier took 1066 seconds, including linking the affected integration executables. No failed check was waived.
- Documentation validation passed: 71 maintained documents and six generated release-guide layouts. The accepted audio content snapshot matched all 13158 source file hashes and 117 unique package IDs/directories.
- Full gate, final Windows CI and preview.4 package receipts are still pending
  at this commit's preparation checkpoint. Record their final results in the
  delivery entry; this note does not claim they passed.

New regressions cover actual-count sealing, common appearance edits, shared
original deadlines, admission/retirement, instanced geometry cost, weighted
restoration, missing cue recovery, cosmetic backlog, model/preparation budgets,
body-free shot reactions and disconnected landing overload. Existing volume,
permissions, source saves, mast/deck/hull, gap and seeded-timing tests remain.

## Practical limits

Cosmetic contact and non-box proxies remain approximations. Terminal-only unseen
projectile trajectories are not guaranteed. Intrinsically excessive collapses
can remain standing; local resource pressure can delay preparation. Per-member
render/transport costs still scale with material. No claim of arbitrary
million-brick fixed-cost collapse or human feel acceptance is made. Maxwell
alone performs interactive gameplay; no visible game or input automation was
used. Warm build output is retained while this playtest work remains active.

## Final source-retirement follow-up

Review during integration found that fragment restoration deletes a transient
source ID, while the old client visibility predicate only recognized a present,
restored brick. One fragment could therefore outlive its replacement original
until the rest of its section retired. Section presentation now consults exact
current generation ownership after the relevant world tick, both at admission
and on source changes. Ordinary whole debris retains its existing predicate.
A regression removes one fragment while another original still owns a section
member; only that retired fragment disappears. Focused client verification
passed formatting, all-target clippy and 719 units (78 ignored), 178 seconds.
Commands: `python tools/check.py focused --crates bri-client` and the earlier
`python tools/check.py focused` (2786 tests across 239 results, 256 ignored).

Candidate 864d3282b built all five playtest executables with
`BRI_VERSION=v0.3.7-destruction-preview.4 cargo build --profile playtest --locked
--bin bri-client --bin bri-server --bin bri-import-addon --bin bri-addon-check
--bin bri-addon-run`, 11m53s. Its full gate passed history/tools, all-target build
(598s), content startup (14s) and all-workspace clippy (218s); tests were still
running when this follow-up was prepared. This earlier result is superseded for
final readiness by the source-retirement correction. Final-head gate/CI and
packaging must pass before merge. No check was bypassed and no gameplay was
automated. Final receipts belong with the separate preview.4 package and PR379.
