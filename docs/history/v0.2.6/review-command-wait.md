> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-command-wait.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: bundled_in_game "command 13" wait fix

Branch origin/claude/fix-command-wait-reference = **8ff2eb93** (one commit on fd719c94).

## Verdict: PASS. Merge 8ff2eb93.

Ran in the cloud: `bri-net --test loopback a_client_behind_on_its_updates_still_gets_its_reply` passes (12 s).

## Is "newest datagram tick" the right reference?
Yes. Pose and RemotePose `tick` are stamped by the host's stream from the same host tick that `replica.tick` (Delta.tick, replica.rs:524) reads (stream.rs:280-322). The reply sits on the reliable stream behind earlier updates, and every update read before it has a tick no later than the reply's. So `ran = read tick − sent` is at most the ticks the host really spent: request flight, plus how stale `heard` was, plus its answer time.
- `heard` is at most about `OWN_IDLE` (4 × POSE_INTERVAL) behind while the client has a body, because the owner's own pose goes at least that often while still.
- With no pose heard (no body yet), `sent = max(read, heard)` falls back to the old reference, so it is never worse than before.

So a host that answers within COMMAND_TICKS (1200 ticks, 10 s) can't trip the bound, unless the request's network flight alone takes about 10 s; the 60 s stall timeout covers that case anyway.

`heard` is updated before `try_send`, so a datagram dropped because the event queue is full still moves it. That's correct: it records where the host is, not what was read.

## Does the test prove the cause?
Yes. It reads nothing for COMMAND_TICKS + 240 ticks, so the backlog ahead of the reply is longer than the bound. Then it sends a command the host answers at once. On main this fails with the gate's exact message (the lane's evidence), and with the fix it passes. The variable it changes is exactly the reference tick.

One limit, not blocking: on a heavily loaded box the host may tick slower than 120 Hz during the sleep. The backlog could then be under 1200 ticks and the test would pass on main too. It goes weaker, never flaky. The lane's red-on-main run is the proof.

## Atomic ordering
Sound. It's one monotonic `AtomicU64`, written with `fetch_max(Relaxed)` and read with `load(Relaxed)`, and no other data is published through it. Coherence on a single location is all it needs.

## Note (not blocking, inferred)
If the host tick could ever go backwards while a client stays connected (a world reload that resets the tick), `heard` would keep the old high value. The tick bound would then never fire, and only the 60 s stall would. That's weaker, never a false failure. I found no tick reset in server.rs. The lane can confirm in one line.

The failure message now says "sent at host tick …, having read to tick …", which is clearer than before.
