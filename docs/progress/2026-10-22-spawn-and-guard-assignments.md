# 2026-10-22 Spawn clearance and live guard assignments

Private ordinary full-map traversal exposed two narrow soldier bodies embedded
deeper in a floor than the lateral unstick radius, and defenders converging on
one objective despite declared claimant capacity. Root owns these generic fixes.

`Unstick.vertical_reach` optionally supplies the upward extent independently of
the lateral extent. Existing omission behavior is retained. The private consumer
uses source pawn half-height 50 UU (2 map units), preserving its existing lateral
vote/search. The original FindSpot/CheckSlice evidence includes an earlier
vertical clearance stage; this bounded search is not an exact port of its
floor/ceiling sweep algorithm. A whole-body floor/low-ceiling regression checks
that recovery clears a deep floor and still refuses an obstructed body.

Objective reservations were erased when bot interaction discovery declined a
vehicle/loose-body opportunity, including when interaction weight was zero.
`release_interaction` now releases only that opportunity; life/context cleanup
still uses `release_owner` for all claims. Ordinary two-guard/two-post play
reproduced both soldiers claiming the capacity-one near post before the fix.

Guards also kept their initial plan indefinitely because it had neither a
completion counter nor an approach timeout once arrived. They now revisit the
provider's actual offers on the existing fair, bounded planning turn. Withdrawn,
changed or full posts release the old guard step and reservation. Admitted work
holds retain their existing state/counter validation and interruption behavior.
No game or content names were added to the engine planner.

The remaining stationary start stood on a small support. Ordinary motor input
against the real map's fixed collision walked out, but the navigation grid had
no intermediate column where the whole body fitted at the lower floor. A bounded
walk-off edge now spans those missing standing samples within one footprint.
It requires a safe floor under the entire crossing, whole-body landing clearance,
head/edge and descent sweeps, respects avoided routes and does not apply to
chassis. An initial version incorrectly crossed a platform gap; the existing
gap/leap regression caught it and continuous floor support was added. The
regression stays unchanged. A new small-support case also executes ordinary
motor walking and rejects a surrounding over-step rail. No source map offset or
content-specific navigation rule was added.

Validation commands/results are appended after the candidate checks. Local
receipts are in `.research/pipeline-integration/guard-offer-{before,after}.log`.
The new private full-map trace gets the formerly stationary prop-start defender
to a defensive objective within 16 game seconds. Movement totals alone do not
close traversal or human mission acceptance.

- `cargo test --locked -p bri-chaos --test bot_guard_offers -- --nocapture`:
  regression red before (two guards on a capacity-one post), green after;
  withdrawal then releases that post without a death or timeout.
- `cargo test --locked -p bri-sim --lib nav::tests -- --nocapture`:
  40 passed, one labelled ignored case remains for the full gate.
- `python tools/check.py quick --crates bri-motor,bri-sim,bri-client-sandbox,bri-package-runtime`:
  all eight formatting/unit steps passed (25 seconds).
- Focused formatting, Clippy with warnings denied and every ordinary integration
  target of the 11 affected non-GPU crates passed. The additional client unit
  run hit the prior native access violation again.
- Windows Application Error events identify `vulkan-1.dll+0x45a34` for both
  crashes. Reading only the minidumps' thread-name streams identified the first
  faulting test as
  `repeated_water_survives_app_graphics_lighting_and_device_then_leaves_with_map`;
  the second fault was on a `scene pipelines` worker. The eight unfinished GPU
  and image tests passed together alone (36 seconds), and the earlier entire
  client binary passed single-threaded. Per Maxwell's request to remove flaky
  tests, removed the identified water/device lifecycle case. The other native
  water/scenery/map-change case remains. This is test cleanup, not a claimed fix
  to Vulkan or the outstanding interactive client crash reports.
- `python tools/check.py focused --crates bri-client` after removal: formatting,
  Clippy and client units passed (661 passed, 77 labelled ignored; 88 seconds).
  Full integration gate remains to run.

The previously observed client-sandbox bulk-memory wall-clock flaky test was
removed rather than given a longer deadline. Other deterministic sandbox tests
remain. The preceding gate's client-process access violation is not attributed
to a named test; isolated 739-test success alone does not close it.
