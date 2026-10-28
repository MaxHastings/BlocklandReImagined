# 2026-10-28 VoIP review and default NPC speech

PR #387 was reviewed against current main, including audio capture/playout,
QUIC relay, authoritative moderation, presentation, options, player/admin lists
and fresh-release packaging. Maxwell authorized fixes, merge after passing
engineering checks/CI, and a final private Windows playtest from merged main.
He performs all interactive gameplay and microphone acceptance.

The review replaced the custom speech codec with native Opus: mono wideband,
20 ms frames at 24 kbit/s, in-band FEC and bounded packet-loss concealment.
Capture and playout queues have explicit low-latency limits; stale network and
capture frames expire instead of accumulating. The audio callback uses fixed
packets and a bounded ring. Talk-spurt resets, sequence wrap/reordering, silence,
focus/PTT gating and all 31 remote speakers have regression coverage. Device
opening/recovery runs off the game thread; an unavailable selected microphone
never silently opens another device. Voice leaves datagram buffer space for
world/movement traffic. Server mute follows verified identity through reconnect
and rename within the server lifetime. Live voices stop at session changes.

Authored `speak_at` names the actual actor and rejects missing actors/profiles
and looping/music sources. NPC reactions use it. The client's existing audio
handles drive blue names and the additive talk animation, including completion,
replacement, local/server mute, motion and session teardown. Ordinary sound
effects remain independent. Existing main footstep, animation, debris and
cosmetic-backlog behavior was retained when reconciling the older PR.

Audio options now group input, transmit mode, local meter, gain/gate and receive
volume together. Fresh settings default to push-to-talk. The 32-player list has
space for explicit voice/mute states and its backing panel resizes with it.
Administrator controls distinguish voice moderation. The microphone indicator
avoids the visible paint palette. Bounded offscreen native UI checks cover
1024x768 and 1920x1080 at 1x and 2x; this is layout evidence, not gameplay.

Default-list/bundle schema 3 adds generated native media. `npc_popculture` and
its server-only rules companion ship enabled in fresh packages. All 118 pinned
recordings, their processed hashes, current policy and recipe identity are
verified in the external draft Add-On bundle; no recordings enter Git.
Bootstrap can regenerate missing media, and release refresh inputs include the
recipe/catalog/policy. Original recording attribution/redistribution notices
remain explicit in the recipe and generated provenance; they are not evidence
of clearance. Private folders support `-SkipArchive` without skipping package
integrity/default-content checks.

Evidence is retained under ignored `artifacts/voip-review/` during integration:

- `cargo clippy` for audio/client/net/sim/ui/package/package-runtime/admin/
  addon-import, all targets, `--locked -- -D warnings`: passed before final UI
  layout adjustment; final integration recheck pending below.
- Audio with `cpal-output`: 32 unit tests passed, including capture gating,
  burst reset, bounded jitter/FEC/PLC and the 31-remote-speaker mixer.
- Net `voice` (5), `replication` (22), native Add-On `bundle` (5), and runtime
  `npc_reactions` (7) integration tests passed. The real-QUIC 32-client test
  transmits from every participant and verifies identities plus reliable chat;
  a separate seeded loss/jitter/duplication test verifies recovery and chat.
- Python tool suite: 126 tests passed. Printed flaky/known-failure messages are
  gate-tool test fixtures, not failures/retries of this change's tests.
- Documentation check: 75 maintained documents and six generated guide layouts
  passed. Native UI probe rendered 15 frames with no missing textures.
- Initial fixture failures exposed stale default-list assumptions and a test
  script treating a command caller ID as a player view; fixtures were corrected
  without weakening behavior assertions. Earlier mixed server/media fixture
  was split to respect existing package authority boundaries.

Final scoped unit/integration checks, latest-main integration, history/protocol
checks, platform CI, the draft-bundle upload, merge and exact-main packaged
startup verification are recorded in the final handoff. These remain pending
until their actual results are available. This entry does not establish the
whole alpha contract or multiplayer acceptance. No echo cancellation/noise
suppression was added: use headphones for voice activity, and test hardware,
Internet quality, device hot-plug and ordinary gameplay in Maxwell's playtest.

Integration follow-up: the nine-crate quick check passed all 1,789 enabled unit
tests; the selected session/UI integration targets passed 144 tests (8 existing
render/content cases remained ignored), in addition to the 39 network/NPC/bundle
tests above. Final scoped clippy and the 15-frame native UI probe passed. A
fresh private preview passed packaged `--check` with zero Add-On health problems
and was handed to Maxwell at his request, using `1b23a9ad0`; no local GPU suite
or performance campaign runs while he playtests. The uploaded draft bundle was
downloaded again and matched SHA-256
`ad7fe3163e7f8f185f23bb5d676b4495cf87f56b3287df01fe0002a5bc58c17b`.
Windows CI build/import checks and workspace lint passed; Linux shard 3 exposed
two grappling-tool bundle fixtures still writing default-list schema 2. A search
found the same stale schema in the hosted-bundle fixture. All three bundle
fixtures now use `bri_package::defaults::LIST_SCHEMA`, so schema changes do not
silently leave synthetic checkouts behind. This was a deterministic fixture
failure, including CI's automatic isolated retries, not a flaky pass. Corrected
target results and final platform CI are retained in the final handoff receipt.
