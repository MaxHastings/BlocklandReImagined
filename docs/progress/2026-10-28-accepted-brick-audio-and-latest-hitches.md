# 2026-10-28 Accepted brick gain and latest Preview 11 hitches

Maxwell accepted the +6 dB playtest tuning. Preserved all eleven byte-identical
playtested WAVs in packages/brick-destruction-audio, updated provenance and the
preparation tool so later regeneration retains the gain. The tool rejects PCM
overflow rather than clipping. Existing voice limits, spatial ranges and profile
balance remain unchanged. These are accepted source edits; not yet merged.

Validation: compared every sample with round(previous_sample * 10^(6/20)),
checked format/duration unchanged, no saturated samples, matched the delivered
playtest WAVs, and compiled the Python tool's syntax. No engine build required.
No heavy validation or gameplay automation while Maxwell is playing.

Newest gameplay log: Preview 11 logs/session-20261028-211351.log, exact engine
9f1eba5ef. Six detailed slow gameplay frames at lines 164, 168, 171, 177, 178,
180 take 50.6-64.4 ms. All run four debris fixed steps with 1195-1683 loose
bodies and 55-64 chunk bodies. Brick debris costs 25.8-36.8 ms; simulation
alone costs 20.1-33.9 ms, network handling only 0.5-1.4 ms. This independently
confirms dense client debris simulation as the recurring CPU bottleneck in
these recorded frames. Bot warnings are not bot CPU measurements. Rate-limited
long-frame lines cannot establish the cost of every frame or rule out CPU
contention. The initial 471.6 ms map/GPU-calibration hitch is separate.
Parsed evidence: artifacts/zombie-team-jets/preview11-latest-user-hitches.json.

Code in BrickDebris::advance uses shared 1/120-second steps, capped at four per
frame. Slower frames therefore multiply costly pile simulation with catch-up
work. Body count bounds do not guarantee affordable contacts/solver work.
Next performance correction should examine pile contacts/sleeping and static
invalidation wake-ups while retaining visible material and timely motion.
The logs do not separately attribute Rapier cost to specific contact/solver
subsystems, so that narrower cause remains unmeasured. No performance fix or
new engine build is claimed by this audio/content update.
