# 2026-10-02 Rule Workshop playable design spike

Branch: `rewrite/rule-workshop`, from main `e05af0dfe`. This is a disposable
creator experiment. Maxwell explicitly clarified that no permanent rule, beta
save/network format, generalized provenance or collaborative authoring platform
should be inferred from this work. Preserve this branch and judge its value by
unexpected playable combinations, not completion of a rewrite.

The existing typed event catalog and bounded scheduler remain the execution
path. Optional typed IF guards execute when due and read current state. Core
facts cover player/object regions, native score/death/spawn/round changes and
timers. Small bounded integer state supports brick/player/match/team/object
experiments. Canonical MiniGame scoring, team assignment and round ending remain
the owners of those operations. Team score is a current-member sum of real
scores, not a separate private scoreboard. Existing damage, tools, portals,
vehicles and classic event actions are reused.

The Wrench Events row remains recognizable; IF controls are optional and rule
copying is local editing. Eight `/rulelab` recipes plant ordinary editable
programs: switch/door panel, delayed puzzle, ordered race, contested hill,
kill-scoring controller, steel-ball goals, state/timer sandbox, and a package
route switch. The route switch registers a real Add-On output and three inputs
in the same editor. No alternative rule interpreter was introduced.

Important choices, limits, awkward seams and reusable/experimental assessment
are in `docs/rule-workshop/DESIGN.md`. Creator mutation instructions are in
`docs/rule-workshop/PLAYTEST.md`. Both are included in Windows packaging. Rule
guards and authored region dimensions survive saves, replication and copies;
transient counter progress does not. Matching branch clients/hosts are required.
Added `crates/net/protocol-changes/rule-workshop.md`. This is not a compatibility
freeze. Original content and source installations remain uncommitted/unchanged.

Validation before the Windows handoff:

- `cargo test -p bri-events -p bri-ui -p bri-sim -p bri-world --lib --tests`:
  857 passed, zero failures, 136 ignored native/opt-in tests. Includes thirteen
  spike integration tests plus optional-guard editor editing/copying, existing
  scheduler/cancellation coverage and save/copy checks.
- `cargo clippy -p bri-events -p bri-ui -p bri-sim -p bri-client -p bri-world
  -p bri-net --all-targets --locked -- -D warnings`: passed.
- `cargo build --release --locked -p bri-client --bin bri-client
  -p bri-addon-import --bin bri-import-addon -p bri-net --bin bri-server`:
  passed locally in 3m55s.
- `bri-client --check content artifacts/rule-workshop-check-state`: 14 maps,
  966 brick definitions, all 35 save pictures; no Add-On health problems.
  No window/audio device opened.
- Native opt-in `bri-sim --test v20_events`: two tests passed;
  native UI `--test field_flow`: three tests passed.
- `cargo test -p bri-ui authored_wrench_offscreen -- --ignored`: passed;
  bounded actual-pack 640×480 renders under `artifacts/ui-native-wrench`.
  Inspection caught an Explain/Clear overlap; the header button was shortened
  and repositioned. No operating-system input or interactive playtesting.
- `git diff --check` and Python syntax check: passed.

Failures resolved: off-grid recipe centers now account for brick half extents;
catalog dummy-parameter validation no longer rejects blank variable defaults
before authoring; class checks prevent unrelated numeric IDs from supplying
condition values. Test fixtures install vehicle/package definitions before
players join, and advance the event clock before synthetic activations. Classic
UI JSON omits empty optional guards. Clippy issues were corrected in touched
paths. Existing plain events and import remain separate from the experiment.

Windows packaging uses the existing release workflow dispatched with
`publish=false`. The branch refuses publication. The private CI content draft
is stale and the addon-bundle draft absent, so `tools/rule_workshop_content.py`
recovers content/87 credited originals from the verified v0.1.15 Windows release
(SHA256 `fdccaf6af617e8ae48e860c3851310f9b5045234f1248fa4fdc43d9edf3b5dad`).
The local recovery run passed. No draft was refreshed or published. Physics
showcase packages ship enabled for this lab; existing user settings can override
those defaults.

Remaining handoff work at this entry: run the branch Windows workflow, verify
its startup/package/zip checks, download the artifact and record that evidence.
All interactive judgment belongs to Maxwell. Main has not been changed.
