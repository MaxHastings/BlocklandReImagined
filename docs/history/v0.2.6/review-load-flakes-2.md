> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-load-flakes-2.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: load-flake fixes, round 2 (8b2e3398)

Branch claude/fix-mirror-recursion-dv7x1g, one commit 8b2e3398925673ba6224c77683090211e2915cbb on d068f06e. Test-only, plus one failure message in net/client.rs.

Verdict: PASS. Merge 8b2e3398925673ba6224c77683090211e2915cbb. The proof is the next gate, as the coordinator said: these are reasoned, not reproduced.

## (1) Soak lighting wait (app_soak.rs:847)
- Before: `run.until(..., 120 s)` counted GAME ticks while lighting loads on worker threads. Under the gate's 16 workers the host keeps ticking while the workers are starved, so the bar measured the wrong clock.
- Now: it waits for `map_lighting_settled()` itself, with the shared `wait::STALL` (300 s, support/wait.rs:31) as the only bound. This does not loosen the claim. The claim is "lighting finishes loading"; how many game seconds it takes on a loaded machine was never a product requirement.
- Can't hang: the loop is bounded by 300 s of wall time from its start (stricter than `until`'s "no progress for 300 s"). The failure prints which lighting part is missing (probes.rs `map_lighting_parts`).
- Nit, no change needed: `source_loading` became pub(super) only for the message.

## (2) Held-pipelines frame (app/tests.rs ~1704)
- Before: "a frame took < 2 s", a wall-time proxy for "no frame waited on the compile".
- Now: the compile is held, so a frame that waited on it would block until the hold ends (recv_timeout 300 s). It would then return with the pipelines built, and `!scene_pipelines_ready()` fails. That proves the exact claim, not a proxy, so this is stricter, not looser.
- Can't hang: the hold ends at 300 s, and the outer loop has the same 300 s guard.
- Lost coverage: the old 2 s bar also caught a frame blocking on something OTHER than the compile. That was never this test's claim, so it's acceptable. The cost is that a real regression takes about 300 s to fail instead of 2 s.

## (3) bundled_in_game failure message (net/client.rs:903)
- Only the message changed: it now gives the sent tick, the newest pose tick and the count of unread events. No change to what the test proves. That's fine as a step to pin the cause on the next failure, but this test is still a retry-only flake until that message tells us why.

## Rocket past a wall at 28x load
- Projectile hits sweep the whole segment every step (weapons/runtime.rs:3733 `q.sweep(p.position, end, ...)`, sweep_box for shaped ones at :2822/:2835). So a big step under lag can't skip a wall by itself; it isn't classic tunnelling.
- If it's real, it's something else, and from the code I can't tell which. Two places to look (inferred):
  (a) collision the sweep queries lagging behind the bricks drawn (chunk colliders after a load or change);
  (b) the hit landing but the brick knock-out being skipped or deferred under load.
- Players could hit either one under lag, and the 10-05 report has the same shape, so it's worth chasing. But it needs evidence first: keep the soak's replay/seed when it happens again (bri-replay exists). Not a merge blocker for this commit.

## Delta 43ce22e9 (on 8b2e3398): PASS. Merge 43ce22e9 instead of 8b2e3398.
- New test a_rocket_knocks_out_respawned_bricks_again (sim/tests/brick_damage.rs): synthetic fixture (content variant #[ignore]d as usual), fixed ticks only (`respawned` within 60*HZ ticks, then 240 ticks), no Instant/sleep/wall clock. Hermetic and deterministic. Passes here.
- It proves respawned bricks are solid on the server. The "client 60-tick step artefact" explanation is the lane's; the test backs the server side of it.
