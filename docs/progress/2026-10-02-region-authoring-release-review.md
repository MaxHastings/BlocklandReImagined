# 2026-10-02 Detection region authoring and release review

Maxwell requires visible detection regions and understandable authoring before
v0.2.0 ships. The previous 276713960 candidate and Windows archive are superseded;
neither is the final release. Root continues to own combined integration,
validation and publication. The full alpha contract and Maxwell's interactive
testing boundary remain unchanged.

## Creator behavior

The normal, sound and vehicle Wrenches offer **Detection region**. Ordinary
bricks start with a collapsed section; region listeners or an authored custom
size open it automatically. Custom width (X), height (Y) and depth (Z) are world
units, centered on the brick. Send stores the property immediately. Cancel
discards the draft. No setup event, activation or row deletion is needed.
`setRegionSize` remains an event action for rules that resize regions at runtime.

The editor uses the inspected brick's actual automatic footprint, including
quarter-turn rotation and the four-unit automatic height. It does not invent
an arbitrary starting custom size. Dimensions must be finite, greater than zero
and at most 100 per axis; malformed drafts cannot send. Region size belongs to
the inspected brick rather than remembered Copy locks. Fill Wrench preserves
each destination's dimensions. Authority requires full trust for a size change
and validates the whole edit atomically. The existing saved brick property
already persists dimensions; the changed Wrench command has a protocol marker.

Hammer, Wrench, printer, wand and brick equipment reveal region outlines using
the same visibility convention as invisible bricks. Putting tools away hides
them. Cyan indicates enabled region rules, pale cyan the selected draft or a
size without region inputs, gray disabled region rules, and orange a listener
beyond the 256-region observer budget. Disabled supported rows still reserve
an observed region for occupancy queries; preserved unsupported rows do not.
Presentation is bounded to 512 boxes with the selected brick always included.
The cache follows replicated brick edits/removals and clears on GPU/session
changes. Both observation and presentation use world::regions::bounds; this
prevents a separate visual approximation from drifting from detection.

The panel fits the supported 640 × 480 logical minimum alongside the original
left column and Send/Events/Cancel buttons. It explains tool visibility, region
events, colors and Send/Cancel. Original generated layouts remain unchanged.
Creator guides and test cards describe direct authoring, credited players and
the distinction between a MiniGame reset and deleting physical example bricks.

## Proactive review and verification

All three lane handoffs received a creator/usability review. Concrete fixes:

- Workshop creation follows server MiniGame defaults and omits unavailable
  optional starting items, with visible feedback. A regression preserves
  available equipment and server respawn policy. Previously a synthetic host
  rejected `/rulelab hill` with an unknown-item error before any region existed.
- Failure to open the Add-Ons folder now reaches a visible error with the path.
- Blockhead Bot's Add-On description reflects opt-in ground crews, pushing and
  current limits. Bot architecture and Add-On join documentation no longer
  describe implemented work as absent/unmerged. Explain's owner/admin boundary
  is stated in the creator guide. A full bot inspector remains future work.

Focused evidence (Homebrew Python on PATH; development debug info/incremental
disabled; no visible game or interactive input):

- UI Wrench suite: 16 passed including synthetic and original-layout offscreen
  rendering, direct dimensions and malformed-value refusal. The three rendered
  Wrench variants were inspected at 640 × 480.
- Authority dimension/trust/atomicity/save-roundtrip regression: passed.
- Outline geometry/lifecycle and observer-budget regressions: 2 passed.
- Normal-App offscreen detection probe: both synthetic and real-content cases
  passed. Hammer/Wrench/printer upload visible cyan edges; empty hands and
  disconnect clear them. An 8 × 5 × 8 editor draft moves actual GPU edges without
  changing the replica; Cancel restores the previous pixels.
- Workshop rule suite: 26 passed, including the unavailable equipment/default
  policy regression. Logs: `/tmp/bri-workshop-rules-final.log`,
  `/tmp/bri-region-preview.log`, `/tmp/bri-wrench-ux-final.log` and the earlier
  focused authority/outline logs. PNG evidence is ignored local output and must
  not be committed with original art.

## Windows failures and retained coverage

Completed CI 37078737677 exceeded its 60-minute job budget. Build, formatting,
tool tests and warnings-denied clippy passed. Its full log also showed three
test failures: bundled_in_game exceeded the 600-second binary watchdog, the
held-basketball probe timed out waiting for a projectile, and a vehicle smoothing
unit test tried to resolve a generated-content role on a content-free runner.
The gate's binary watchdog and all acceptance assertions remain in place; the
overall Windows job budget is now 120 minutes for serial software-GPU coverage.

The bundle test now imports all 45 pinned stand-ins once per binary and copies
an immutable installed fixture into independent scenario folders. Both enabling
workflows, every host rule and all networked gameplay actions remain tested.
Both tests passed locally in 81.82 seconds. Shutdown is bounded and gameplay
phases have labelled error context. Log: `/tmp/bri-bundled-fixture-final.log`.

The basketball probe awaits the replicated Charge/Armed states and Armed before
release, instead of relying on a fixed 0.6-second delay. Its projectile and
ball-leaves-hand assertions remain, with tick/image/projectile diagnostics.
Synthetic and real-content probes passed in 9.56 seconds.
Log: `/tmp/bri-held-items-final.log`.

Vehicle smoothing always tests synthetic car/tank/horse and the checked-in
plane, and additionally retains the five authored v20 vehicle cases whenever
content exists. Missing content is checked before role lookup; present invalid
content still fails. Both seeds and all existing correction/pop/rough-frame/
whip thresholds remain unchanged. All 18 runs passed locally in 1.09 seconds
(`/tmp/bri-smoothing-all-content.log`). A newly tried generic synthetic carpet
produced eight corrections with zero pop/rough frames; its invented hover and
flight tuning differs from the authored carpet. It is excluded from the new
stand-in matrix, with original carpet and plane coverage retained; this does
not establish a distinct prediction bug and merits a separate fixture audit.

A lane accidentally edited primary's bundle test during diagnosis. Root
preserved that untested patch at `/tmp/bri-content-thread-untested-bundle.patch`;
the final fixture change is a separately reviewed implementation that retains
the two independent test cases. Root restored only that lane's primary edit
after verifying it matched the preserved patch. The unrelated Mac setup entry
is preserved.

Workspace formatting and all-targets warnings-denied clippy for client, sim,
UI, world and importer pass. The first lint run flagged the new outline module's
test block preceding its implementation; moving tests to the end resolved it.
Windows CI 37086470219 then found a missing end-of-file newline left
by that move. Root corrected the file and restarts validation/builds from the
corrected commit; the nonpublishing 37086471497 build was cancelled. No runtime
assertion or formatter requirement is bypassed.

Pending: commit the final reviewed source, run the complete local gate and new
Windows CI, then build and validate fresh Windows/macOS/Linux assets from that
same immutable commit. The older 321-binary gate receipt does not certify these
new edits. The unavailable private saves corpus remains explicitly uncovered;
automated/offscreen evidence does not certify subjective interactive feel.
