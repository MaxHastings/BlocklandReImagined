# Familiar events without the old limits

Maxwell's 2026-09-26 feedback from a longtime Blockland creator adds concrete
quality-of-life requirements to the complete vanilla alpha. The reference editor
and event vocabulary remain recognizable. The native implementation should not
reproduce restrictions that made elaborate multiplayer creations impractical.

## Requirements

- Support substantially more than 100 events on a brick through editing,
  validation, transport, execution and save/reload. A larger array alone is not
  acceptance: long lists need usable scrolling and editing, and limits must be
  visible and consistent across every layer.
- Execute a finite zero-delay chain in stable authored order within the current
  simulation event phase when its execution budget permits. Do not insert a
  mandatory 33 ms delay at every relay hop. Author-requested delays still apply;
  the simulation's tick resolution must be documented.
- Define ordering and mutation visibility explicitly. Ordinary dependent events
  should not need arbitrary sleeps to observe an earlier event's completed
  change. Physics-dependent operations may need a documented phase boundary;
  the editor/runtime should explain it instead of relying on trial and error.
- Bound execution and queue memory. A nonrecursive work queue, per-origin and
  global budgets, stable continuation ordering and loop diagnostics should keep
  an infinite relay cycle from freezing everyone. Deferred work must remain
  observable, cancellable and attributed to its source. Admission failures must
  be explicit and atomic; never silently lose a tail of an event list.
- Measure eight independent players with active areas, representative vanilla
  bots and event bursts. Attribute server time to events, AI, physics and
  networking; report queue depth, oldest-work age and per-origin throttling.
  Establish actual limits on recorded hardware and retain regression workloads.

## Architecture to preserve for larger game modes

Prefer indexed event targets, bounded scheduled jobs and spatial relevance over
full-world scans or a heavyweight per-bot script loop. Stagger expensive AI
decisions while retaining responsive movement and authoritative combat. Separate
network interest from simulation activity: a bot outside one player's view may
still matter to another player or a global quest. Any sleep/reduced-update policy
needs explicit wake rules and correctness tests for projectiles, aggro, timers,
ownership and players entering an area.

Avoid unlimited-performance claims. Parties, progression, quests, boss encounters
and an open-world RPG are motivating future modes, not an added requirement to
ship an RPG inside this alpha. Expose stable entity/event interfaces and make
bottlenecks diagnosable so those modes can be built later without replacing the
core. Mod gameplay runs through the package runtime, not through brick events
(see `architecture/package-runtime.md`).

Support authored fade-out/despawn as an effects capability: visual opacity and
entity lifetime/collision are separate decisions, replicated consistently. Keep
the original default disappearance/death behavior unless a mode chooses fading.
Fade-out is a requested extension; its exact scripting/editor interface remains
to be designed alongside player/bot effects rather than hardcoded into all deaths.

## Current implementation versus this requirement

The native per-brick admission bound is 1,024 events (`MAX_EVENTS_PER_BRICK` in
`bri-world`). A full list passes validation, transport, save/reload and ordered
zero-delay execution; an over-limit edit rejects atomically.

`bri-events` runs the vanilla catalog (16 inputs, 65 outputs), including relays,
named targets and `cancelEvents`. It uses a nonrecursive work queue with
per-origin and global budgets, stable due-tick/order sorting at 120 Hz and loop
demotion for runaway zero-delay chains, without an artificial 33 ms delay per
hop. See `crates/events/README.md`. The host session runs it with default
limits. The event runtime has no profiling yet.
