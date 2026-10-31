# 2026-10-30 Release plan second review

Maxwell requested another assumption/context pass before implementation. The
[release plan](../plans/2026-10-30-connected-sessions-bots-and-lighting.md) is updated,
still pending approval. No runtime changes, builds, interactive sessions, replayed
simulation, GPU suites or gameplay automation ran.

## Confirmed decisions and added reports

- Global voice inside a minigame includes living and dead members together. This
  changes current policy, which gates dead-to-living speech in every mode.
- Loading voice/chat follows an authorized destination game, otherwise the lobby.
  A new channel picker is not a prerequisite; private/team authority remains real.
- Use subtle material detail preserving the original toy-like look.
- Hammer flicker: uphill on 45-degree ModTer bricks, hammer held without swinging,
  ragdolls enabled, no corpses present. This does not establish the separate death
  collision report's cause.
- Add persistent movement delay after long ordinary play, cleared by Ctrl-K/respawn
  or F8/F7. Walking/jetting seem affected; camera response is uncertain. Networking
  was Maxwell's guess, not a diagnosis. Host-versus-guest occurrence remains unpinned.
- Add destroyed-brick light/emitter following and item-spawner suspension/restoration.
- Add issue 415 and Robbin's source/custom shader references. Their descriptions
  and implementation proposals remain evidence to assess, not architecture commands.
- Bot_Town source was temporarily unavailable and Maxwell asked to hold it. He then
  supplied `E:/Downloads/Bot_Town.zip`, superseding the hold. Issue 414 is active.

## Saved worlds and navigation

Read the five existing BRI-BUILD files' packed MessagePack columns as well as
headers, following `world/src/build.rs` and `packed.rs`. No save was written.
The cube has three intrinsic Zombie Hole bricks and one 16x cube. The staircase
has three holes, 75 2x8 bricks and predominantly 0.6-unit successive rises, plus
two 0.8 transitions in its stored order. The mattress house has four zombie holes;
the tower has three; the confined house has one. Those counts are source data,
not verification of clearance, sight, reachable routes or live bot state.

`nav.rs` already models floor height, subcell standing offsets, motor-measured
reach, shared work budgets and partial searches. This rules out assuming BRI
simply lacks all multi-level navigation. The plan now requires tracing sampling,
search exhaustion, cost/heuristic, smoothing and following before proposing a
replacement navigation system. Saves do not preserve the whole enabled Add-On
environment; reproduce the actual combined setup and a native-Zombie control.

## Audio context

Read installed v0.4.2 weapons/ambience definitions and decoded only the requested
audio files without playback. Python wave/soundfile/numpy measurements:

| File | Sample peak dBFS | RMS dBFS |
| --- | ---: | ---: |
| tool-swish-a.wav | -12.00 | -26.88 |
| tool-swish-b.wav | -12.00 | -25.42 |
| tool-swish-c.wav | -12.00 | -27.58 |
| indoor-base.ogg | -15.84 | -30.87 |
| outdoor-base.ogg | -10.71 | -31.58 |

All requested increases have sample headroom individually. This does not measure
true peak or the combined limiter/mixer result. Weapon sound profile validation
caps `volume` at 1, and current profiles already use 1. Regenerate the approved
derivative waveforms through existing recipes instead of merely writing invalid
profile gains or expanding the engine unnecessarily. Current user-base policy is
+12 dB total relative to the accepted loops; the new +4 request implies +16 total.
Sword Fire has no sound; its separate Activate draw sound can remain intact.

## BadDream and issue 415

Read issue attachments in memory through bounded HTTPS requests; original ZIP
hashes are in the plan. A source-guided DIF prefix reader rejected the extended
layout and validated non-extended surface references/PNG structure. Original
`bd2.dif` has one detail, 117 surfaces, five gsfstatic surfaces with flags 16,
two lightmaps, one animated light, four states, 116 state-data entries and a
3,312-byte state buffer. The converter currently skips those animation arrays.
Associations, timing, flags and influence on actors remain to decode; the count
alone does not establish the correction.

GitHub connector read issue 415 and its empty comment thread. Its shader pair
loader is a proposal. BRI's `content::atmosphere` already has direct/ambient/shadow
colors, fog and vignette; `client-sandbox` has WGSL validation. Their actual
end-to-end correctness needs inspection, rather than a duplicate GLSL loader.

The default fragment file selects/blends cascades using `shadowSplitCount` and
depth cutoffs, so four cascades do not mean only four sun-cast shadows. The custom
file uses luminance-derived pseudo-bump contrast, which is related custom evidence,
not a stock requirement or a universal height rule for arbitrary diffuse art.
The decompiled client source was read through the connector at blob
`baec6536d0e7d07f60b2159ef3dc9e54599a493e`, including presets around lines
6112–6199 and environment GUI variables around 21532–21738. It corroborates script
controls, not engine internals. Original files remain outside Git.

Located the existing Steam installation through local libraryfolders metadata:
`S:/SteamLibrary/steamapps/common/Blockland`, app 250340, installed build 5096106.
Both supplied default shaders match that installation byte-for-byte. It is
supplementary read-only v21 evidence; the designated E: v20 content reference stays.

| Issue 415 attachment | Bytes | SHA256 |
| --- | ---: | --- |
| default_frag | 11,982 | `aa32acb2a059d6099d70e1a67168c2670617f780154cb3741df027119d7a7af4` |
| default_vert | 1,357 | `792244f400eeff0361487ef6578f919e9767d61cb80e658ecfbc51cfd7fd44b1` |
| custom_frag | 17,847 | `1d4192188a563923021b20e3cc1b745895a85a230ce8de98c17a0586dc3b91b2` |
| custom_vert | 2,250 | `7ce582b6f252813e66ffaaf23682494ae8b806730a34edc4cf23f284504f1709` |

## Bot Town and destroyed brick attachments

`E:/Downloads/Bot_Town.zip` is 66,470 bytes, SHA256
`c4e9a0123a62486c01a9c1f15c0dfd0ab65bb3ea399fa77379cde7628285ce2d`.
All eight brick definitions explicitly reference `Add-ons/Bot_Hole/4xSpawn.blb`;
all eight icons exist. `server.cs` names Bot_Hole through LoadRequiredAddOn, and
the previously supplied Bot_Hole ZIP contains `4xSpawn.blb`. This validates the
expected input family without relying on the reporter's repaired output. Fresh
import/runtime/checker reproduction remains ahead; no source scripts were executed.

`client/src/effects.rs::attach` constructs effect frames from the static brick
center, while brick_debris stores brick IDs and exposes moving transforms. Its
legacy fake-dead comment is not a valid description of current world state.
`sim/src/item_spawners.rs::reconcile` has no explicit destroyed-state guard.
Trace their full consumers and dirty lifecycle before fixing. Do not conflate
nonrendering bricks with destroyed ones, move gameplay authority with cosmetic
physics, duplicate every fragment's source or teleport already emitted particles.

## Recorded movement evidence

Maxwell identified the v0.4.2 installation logs at E:. Read those text logs and
the current day's match recordings under the default local state. Logs confirm
v0.4.2 `93c1439da` and mainly contain frame/GPU timings, not movement queues or
prediction history. No log excerpt supports calling the reported input delay an
FPS defect.

The October 14 [prior investigation](2026-10-14-host-stall-move-refusals.md) found
the same reset-sensitive symptom, with 1.1M refused move attempts in its older
recording. Current source retains skipped-tick credit `207c819ef` and the separate
discarded-input acknowledgment correction `d7721dd72`.

Read zstd length-prefixed MessagePack calls without rebuilding/replaying a session:

| Recording | Final tick | Failed movement-rate attempts |
| --- | ---: | --- |
| match-131 | 98,009 | 0 |
| match-132 | 471,701 | 257,398, owner 28 |
| match-133 | 334,560 | 0 |
| match-134 | 655,396 | 229 across owners 1, 19, 22, 23 |
| match-135 | 92,883 | 0 |

The reusable extractor rechecked all five inputs and the exact final ticks; none
has a tick rewind. The exact sustained refusal range is tick 263937–454333 in
match-132, recurring across many minutes. Admission
identifies owner 28 as a guest rather than Maxwell's owner-1 host character. This
is a real shared-input failure, not yet proof of his local symptom. Match-134's
229 refusals cluster near tick 417100, not a sustained recording-wide failure.
Counts include redundant attempts and are not unique lost moves or packet loss.

The initial command scan assumed externally tagged commands and found no resets;
that was an analysis error. Corrected inspection uses the command's `kind` field
and finds ordinary suicide/respawn and camera-drop commands. No causal recovery
claim is based on the incorrect scan. Host-recorded calls still do not contain
the client's complete predictor/camera timing state.

| Input | Bytes | SHA256 |
| --- | ---: | --- |
| match-132.brimatch | 25,925,392 | `1cf66209c99a88d19564383aa1008e79b312a55879a0515fdfe937d29f4b744b` |
| match-134.brimatch | 73,136,556 | `7ffb2736a0a59adb26265f77ed2b139afdc37b427f9ac39ed73b39f7ee8e52ac` |
| match-135.brimatch | 4,047,812 | `d99f5cb542ee9f44e161516af42896643bbc047abcb3b51a68385fe923c9d972` |

A private reusable extractor is kept at
`.research/release-plan-second-pass/inspect_recorded_movement.py`; run with Python
and one or more full recording paths. It prints bounded identity-free summaries,
input hashes, affected host/guest flags, exact final ticks and per-minute refusal
bins. The verified five-input output is beside it in
`recorded-movement-summary.jsonl`. This audits recorded outcomes; it does not execute
scripts or simulate play. The all-five check also records 26/22/35/132/3 skipped
ticks respectively, which does not by itself explain owner 28's later depletion.
The implementation milestone starts with the actual failure onset and paired
before/after reset state, retaining anti-speed/flood authority and avoiding periodic
reset workarounds. A synthetic test pass alone will not close this long-run report.

Commands included scoped git show/grep/log, rg over the exact logs, read-only ZIP,
saved-build and recorded-call inspection, waveform statistics and file hashing.
Original game installations, recordings, saves and package choices were not changed.
Only planning notes and an ignored diagnostic helper were written. Product causes
still need finite reproduction; source findings and user decisions are separated.
