# v0.4.2 integrated candidate and release boundary

All approved feature code reached remote main at
`ba4f1bbb54c4da18b27f15a088514cda58ccdc62` through the ordinary
`python tools/gate.py --push` path. The pre-push hook remained enabled and
reused its still-locked parent's exact commit/base validation. PR407 contains
the combined integration; PR408 contains its importer/map ancestor
`12641b6f834db0e39465374578963c7c74e6b884`.

Maxwell explicitly approved the combined playtest and directed publication
after the remaining Add-On work, main integration and release checks. His
latest instructions also authorize completing publication verification and
cleanup without another release approval request. This is the release goal,
not completion of the broader alpha contract.

## Scope and human acceptance

- Bots: PR404, `c1d183527bbeeb0b665a03f54f8e1315a18e6ed2`; finite owner handoff
  satisfied. Destruction: PR405, `2d95e3fbfab874837454457a9835f337871715df`;
  finite handoff satisfied. Both are ancestors of the combined source.
- Water: PR406, `a8a5e5d39aeeed0ff0687cddf1f6da16f35f28ce`. Only the accepted
  original-texture sun-glint modulation is included. Rejected overhaul history
  is excluded; swimming, buoyancy, source textures and hazards are unchanged.
- Ambience: the two approved user edits, approved full loop seams, and final
  restored +12 dB total mix. Storm is unchanged. Only localized fridge and
  authored Creek waterfall/shore layers remain alongside the bases. Original
  inactive media is retained. The owner explicitly declared the finite lane
  satisfied after direct listening acceptance.
- Tools: only the three approved artisticdude swishes through existing image
  Fire-state sounds. Stock Gun/Akimbo policy and vehicle recordings remain
  excluded, as does the cancelled broad SFX audit.
- Importer/map: automatic in-content staging no longer treats installed content
  as a protected original source. Compatibility notes expose unsupported
  behavior. Source-backed legacy resource paths and exact installed sky reuse
  preserve incoming mission properties and enforce integrity checks. The four
  supplied Add-On gameplay ports are deferred.

The original ambience-owner gain/acceptance commits were cherry-picked during
rapid data-only auditions. The subsequent documentation integration records the
original owner's complete ancestry as well; its runtime/data tree must remain
identical to the approved combined source. Build-time conservative pending-mix
labels are superseded by the dated direct acceptance receipt, without
restamping accepted media.

## Actual validation

Exact combined-source CI run `40993869240` passed all seven required jobs on
`ba4f1bbb54c4da18b27f15a088514cda58ccdc62`; the workflow's legacy aggregation
job is intentionally skipped. Importer CI run `40993659144` on `12641b6f8`
also passed its seven required jobs. These are platform source checks, not the
three native release package/startup receipts still required before publishing.

The guarded combined main push passed history/protocol, tool and documentation
checks. Focused checks completed in 1,637 seconds: 3,735 passed, zero failed,
348 ignored across 277 reported test groups. This was a scoped gate, not the
full release gate. Its log is retained at
`.bri-gate/logs/ba4f1bbb54c4-scoped.log` outside Git.

The importer handoff records 105 focused native tests and warning-free changed
library, importer-binary and integration-test Clippy. Existing reconversion and
in-game transition suites each passed two cases using the exact already-built
client unit-test executable. The initial extra Cargo command accidentally used
a different incremental mode; only that verified redundant Cargo process tree
was stopped, its log retained, and the cached executable used instead.

The unmodified `Map_IZB.zip` SHA256
`e8d542ec8c208e291a5987653909b0f134f49ce1a258740ce0755e38cc0032b0`
converted with the new importer against the independently verified clean v0.4.1
archive content, using `--installed` and no original-source reference. Its
seven sky images match the installed originals' source identities, byte hashes
and dimensions. Original red sky/fog, 300/1,000 fog distances and sky flags are
preserved. There are no stand-in or unresolved textures.

An isolated exact combined-source native package/version/headless startup
check also passed, with the imported map present among 22 maps. It verified
13,403 seed content files. Client SHA256:
`a19cf57203270fc24d33de5362ba24f86ce3005246712cf75c2b319dc4fec858`.
No visible game or automated gameplay was used.

Retained map limits are explicit: FileObject node48 is unadapted, absent
RepeatTerrain uses an unverified classic default, an optional terrain detail
texture is not packaged/rendered, and imported Dynamic lighting/default surface
limitations remain. Successful conversion/admission is not original visual or
gameplay fidelity. The importer owner independently reviewed these receipts and
declared its finite technical requirements satisfied.

Failure evidence is preserved. Original importer CI/local map fixtures omitted
a required Sun and deterministically failed; the fixture was corrected without
weakening assertions. Windows whole-workspace format invocation hit OS206;
per-package formatting passed. The earlier destruction lane's pre-existing
ragdoll issue400 failure/retry remains recorded as flaky, not silently green.

## Content and source audit before publication

The seam audit reviewed all 204 rows: 552 named items, 13 descriptive/path rows
and two intentionally absent historical capabilities. Shader functions and the
serde-derived `height_black` name were checked explicitly after the initial
extension filter omitted them. Source presence is distinct from consumer and
human acceptance. The seam page records the reviewed main source and date.

The existing coordinator base packs are unstamped and their planning output
does not establish final-source freshness. Release staging will run actual
repository generation recipes with source/hash receipts, using the existing
warm shared target rather than creating a cold target. Original installations,
the primary dirty checkout, accepted previews and user package selections stay
preserved. Approved raw ambience inputs and pinned tool inputs are retained;
no new recording or broad library search is authorized.

Remaining release requirements: freeze the integrated/documented source;
generate and verify actual final content/bundle inputs; complete the deliberate
full gate with both performance budgets and real 23-save corpus coverage;
build/check all three native platform packages; independently verify versions,
all manifest hashes, source and base/Add-On equality; only then tag/publish.
After publication, download and verify public assets, sums, tag and latest
endpoint, merge the dated publication receipt/STATUS closeout, preserve needed
evidence and clean finished merged work outputs, then delete the one-time
release heartbeat. No publication or full-gate pass is claimed here.
