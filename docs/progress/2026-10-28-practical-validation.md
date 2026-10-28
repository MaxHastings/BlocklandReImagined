# 2026-10-28 Practical validation and warm playtests

Maxwell explicitly replaced the mandatory exhaustive merge gate and requested
aggressive iteration-cost cleanup. Private preview 9 is already delivered;
PRs #382 and #384 are merged. Do not run heavy local validation while he plays.

Changes:
- AGENTS.md now separates private playtest delivery, scoped merge checks and
  deliberate extended/release validation. Preserve history/protocol safeguards.
- gate.py defaults to exact-commit tool/docs/focused checks. `--full` retains
  the workspace, ignored-content, corpus and performance path. Release tooling
  requests it explicitly. Scoped checks never create extended-pass receipts.
- Focused checks default to direct consumers, with deeper selection available.
  UI/effect/chaos matrices require explicit integration targets locally.
- CI builds each shard's exact package/target selection. Previously a single
  `--workspace --lib` request linked every workspace unit harness on each shard,
  including other shards' units. Identical target selections share one graph.
- Disable 39 empty CLI/probe libtest harnesses, verified against the completed
  a26f9e097353 gate log and sources without unit-test/module hooks. Actual tools
  still build/lint. Workspace remains 44 crates; the apparent 555 crates were
  primarily test executables. No Rust assertions or gameplay code removed.
- Eight measured GPU matrix targets move to nightly/manual/extended coverage:
  metal/mirror sprites, many-room effects, GPU contract, screen sweep, minigame
  screens, admin screens and net graph. Retain fast UI/effect input/state cases.
- Normal main CI uses the bounded PR tier; nightly/manual retains full CI.

Evidence: 123 Python tool tests passed in 6 seconds, documentation checks passed,
locked Cargo metadata parsed. Metadata target count drops 562 -> 523. Ordinary
CI targets drop 312 -> 280 (empty harnesses plus eight GPU matrices). Checked
all four planned shards against actual metadata: exactly the assigned targets,
no foreign or duplicate unit harnesses. Compatible selections now pack into
three Cargo graphs per shard (one library graph, two other target groups),
instead of 17-18 feature-resolving package invocations. These are planning counts, not a claim
of measured wall-clock speedup. Platform CI is pending. No local Rust/GPU/full
build was started during Maxwell's playtest.

Loading is a separate unresolved runtime task. Maxwell's screenshot reports
41.453 s map preparation, including 21.399 s hashes (540.5 MB), 10.053 s brick
catalogs, 6.346 s world collision and 3.230 s animations. Existing source already
has timestamp/length hash reuse and per-content definition caches; trace misses
and invalidation before adding caches or bypassing verification. Acceptance
targets: <=5 s first map/server in a fresh process, <=3 s repeat, through playable
control. These targets have not been established.
