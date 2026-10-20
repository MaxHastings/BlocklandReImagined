> **Historical note. Written 2026-10-19 (the file's date in the cloud project folder, `audits/test-suite-under-10-plan-2026-10-19.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Plan: gate under 10 minutes of tests and under 10 of compile

Max's target (2026-10-19): tests under 10 min, compile under 10 min, about a 90% cut of the full suite.

## Where the time is today (measured)

The two newest PC gate runs (`.bri-gate/logs/b138622a993d.log`, `a4b55e6dc601.log`, 2026-10-17/18) end with a phase line:

`tool-tests 7s, build 1041s, content-check 2s, clippy 352s (alongside the tests), tests 1178s, retries 96s, total 2324s`

So the gate is **39 min: 17 min build, 20 min tests, 1.6 min retries**. The 2 h 29 min in pipeline-timing.md is a different thing (a whole-workspace `cargo test` on a PC shared with lane builds), and the "50 min compile by inference" was not measured: the gate's compile is 17 min, and in both runs all 670 units were compiled from scratch (the warm gate on 2026-10-07 built in 81-267 s).

## Why the build is cold: the target dir fills with debug files

`.bri-gate/target/debug/deps` held 17.6 GB of PDBs and 8.5 GB of test executables in just the first 2,000 files, against a 40 GB cap past which `trim_target` deletes the whole target dir. Every bri-client test executable is 138-148 MB (it links the whole client). One generation of a gate build is therefore most of the cap, so about every second run starts cold. (The wipe message goes to stdout, not the log, so that it fires is inferred. The sizes are measured.)

## Steps, with projected minutes

| # | Step | Branch | Build (min) | Tests (min) | Basis |
|---|---|---|---|---|---|
| 0 | Today | | 17.3 | 19.6 | gate log |
| 1 | No debug info in dev, and prune only superseded test executables instead of wiping the target dir | `claude/test-build-size` (45bfb29) | **3-4.5 warm** (17 only after a toolchain change or a deliberate wipe) | 19.6 | warm builds measured 81-267 s on 2026-10-07; cloud: exe 376 MB to 200 MB, link 4.7 s to 3.1 s |
| 2 | Cut list: delete 6, shrink 10, real-content copies out of the per-landing gate | `claude/test-cost-cut` (f4ea458) | same | **about 7-9** | 3,060 of 5,843 binary-seconds removed (52%). The 3 heavy slots (bri-client, bri-render) carry 3,200 binary-seconds, so 1,060 s today; the cut takes about 2,900 of the savings from them, which leaves the heavy pool well under 5 min and the longest remaining binaries (crossover_soak 129 s, multiplayer, vehicle_first_person) the floor |
| 3 | Raise `HEAVY_JOBS` from 3 once the heavy pool is lighter | gate.py | | **about 5-6** | 6 slots ran the old pool in 820 s but GPU contention slowed binaries; with half the GPU work this should hold. Needs one PC run to confirm |
| 4 | Replace pixel-only render probes with checks on the scene data | next | | about 4-5 | candidates: `app_item_render`, `held_items_render`, `hud_regions_render`, `body_attachments_render`, `player_types_render`, `horse_riding_render`, `shadow_render`, `mirror_render`. About 35 binary-seconds each after step 2; this removes the shader compile and the offscreen frame wait. Needs a rewrite per test, and each needs a GPU to prove the replacement |

Step 1 is what takes **compile under 10** (3-4.5 min warm). Steps 2 and 3 take **tests under 10** (5-9 min). Step 4 is margin. Clippy (6 min) runs alongside the tests and is not on the critical path unless tests fall below it; at that point clippy (352 s) becomes the floor and wants `-j` and a shared target with the build.

## Things I looked at and did not do

- **Folding each crate's integration tests into one binary.** Link time is ~3 s per bri-client binary on Linux (85 binaries, 4 CPU-minutes). Windows is slower but unmeasured. The disk problem folding solves is already solved by step 1. A fold also has a real cost: every file's tests take turns on one GPU device and a few own fixed ports, so the gate would have to run one module per process (list the tests, group by module) to keep today's isolation, and that cannot be proved without a GPU. Ask for one PC measurement first: `cargo test --no-run -p bri-client --timings`, then read the link share. If links are over 20% of the build, fold bri-client only.
- **A shared shader/pipeline cache.** Every App boot builds its pipelines (`create_render_pipeline`, 17 sites); tests in a binary share one `Headless` device but each boots its own App. wgpu's `PipelineCache` is Vulkan-only, so on DX12 only the driver's cache applies. The `App::load` time is not measured anywhere I can read; time it on the PC before building anything. Step 4 removes the need for most boots.

## Tests I would keep even though they break the budget

- `net/crossover_soak` (129 s): the only real end-to-end check of four clients, sixteen bots, download-join and prediction over an impaired link. It sets the floor of the test phase on its own; if Max wants under 5 min, move it to the release gate.
- `client/save_corpus` (62 s): already runs only when saving, loading or the `.bls` converter changes.
- The performance budget (32 bots, 30 s, release build): not in the phase line above, so I could not cost it. It needs a release build and is the one step that is not debug; check its time on the next run.

## Risks

- Step 1 removes function names from backtraces in test failures (file:line in a panic is unaffected). Release builds keep their line tables.
- Step 2 moves real-content runs to `gate.py --release-content`. A bug visible only on real v20 data surfaces at the release gate instead of per landing.
- `bot_soccer_match` now plays one seed. If its aggregate checks fail, restore two.
