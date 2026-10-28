# 2026-10-28 Preview 11 listening gain and user hitch review

Maxwell requested +6 dB for the new recorded destruction sounds for his playtest.
Applied an audio-only adjustment inside the delivered Preview 11 folder at
`E:/Downloads/Destruction-preview-2026-10-28-r11-bots/BlocklandReImagined-v0.3.7-destruction-preview.11-windows`.
All eleven snap/contact/landing PCM16 recordings were multiplied by 10^(6/20).
Verified RMS increases within 0.01 dB and no clipped samples; lengths, sample
rates, authored profile balance, pitch, range and voice limits remain unchanged.
Updated package SOURCES.json, PREVIEW-NOTES, MANIFEST hashes and the outer receipt.
Original audio package and engine/main unchanged; this is a local listening trial.
Reversible backup and script are in ignored artifacts/zombie-team-jets.
Restart required because the running game caches sounds.

Packaged bri-client --check with isolated state passed: 21 maps, 1396 brick
definitions, 35 save pictures, 14 modern lightmaps; two existing unrelated
Butterfly Knife health warnings. No interactive playback or gameplay performed.

Also inspected Preview 11 session-20261028-210700.log after Maxwell reported
90+ FPS dropping toward 30. Ten detailed gameplay hitches show 553-1001 loose
debris bodies plus grouped chunks. Eight run four 1/120-second physics steps.
Several spend 26.6-37.7 ms in brick debris; simulation alone reaches 31.6 ms.
Code confirms the per-frame accumulator catches up to four steps, multiplying
CPU cost as frames lengthen. This supports debris physics as the main recurring
cost, not proof that every hitch has that cause. A 120.1 ms frame separately
spends 84.8 ms in poll_network (includes local event processing, not a latency
measurement). Another has 10.8 ms of fade uploads. Bot route warnings do not
measure bot time and cannot establish bots as the frame-rate culprit.

Focused one-minute summary records 244 frames below 30 FPS, median 13.8 ms,
slowest 1% 48.1 ms. GPU median 6.9 ms, slowest 1% 18.2 ms, worst 67.4 ms:
there are occasional GPU spikes too, but recurring CPU debris cost is explicit.
Long-frame lines are rate limited; do not infer exhaustive frequency from them.
Parsed evidence: artifacts/zombie-team-jets/preview11-user-hitches.json.
No performance code changed in this listening adjustment. Next correction should
target dense-pile simulation/catch-up and separately instrument event processing;
preserve visible material and avoid returning to delayed/slowed debris motion.
