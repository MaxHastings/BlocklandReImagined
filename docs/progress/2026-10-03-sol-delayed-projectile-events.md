# 2026-10-03 Scheduled native projectile outputs

Sol High owns the narrow events/projectile lane on `codex/v0.2.2-hardening`.
Root owns release integration, wire/protocol/manifests and shared build scheduling.
No interactive gameplay, original content, commits or Cargo profile changes.

## Finding and implementation

Immediate `onProjectileHit` responses already execute synchronously inside the
weapon collision query. Scheduled rows previously received no Projectile target;
the Session event host also reported projectiles dead/forbidden and rejected
Projectile intents. Delayed output support was therefore only stored data.
The existing event scheduler already supplies delay ordering, budgets, source
and client validity, cancellation and IF evaluation at execution time.

The adapter now captures the **original projectile ID and contact normal** at
its real input. A transient per-origin observation is retained only while that
existing scheduler origin has pending/held jobs, bounded by its origin limit.
No new executor or public context framework was introduced. A small internal
Dispatch authored-delay field avoids repeating a synchronous immediate response
when its ordinary scheduler dispatch is postponed by a work budget. Dispatch
is not a serialized/network type; wire and content schemas are unchanged.

Native runtime responses share the existing redirect arithmetic and mutation,
Delete uses ordinary removal, and Explode places the live original in the
existing explicit-explosion queue. No substitute projectile is spawned.

## Chosen semantics

- Zero-delay rows retain the existing first-response-at-contact behavior.
  Immediate IF remains unsupported and invalid guarded rows cannot reflect a
  projectile unconditionally. All supported delayed rows use the same scheduler.
- A delayed action acts only on its original still-live projectile. Normal
  collision, expiry, earlier Delete/Explode and disconnect cleanup may retire it.
  No action extends its life while waiting, and newer shots are never targets.
- Bounce reflects **current due-time velocity** around that input's captured
  contact normal. Redirect uses the current speed when normalized, or the
  authored vector otherwise. Later contacts do not replace the earlier normal.
  Successful redirects retain the existing age-reset/unstick response semantics.
- Delete removes it in the event phase. Explode retires it there and the existing
  explicit explosion phase processes its current position/source at the next
  weapon tick, using canonical effects, damage and safety rules.
- IF observes current due-time state with the existing query vocabulary;
  unavailable observations fail normally. Same-due outputs preserve the normal
  authored ordering, so later operations observe earlier velocity/removal.
- Existing Explain traces specifically identify an expired original projectile.
  Cancellation and source removal retain the scheduler's existing behavior.

## Verification state

Nine content-free real-projectile fixtures are prepared: unchanged first immediate
response, delayed Delete with live-target IF, current-speed ordered Redirect,
delayed Bounce, IF after a scheduled color change (pass/skip), natural impact
retirement with a newer shot untouched, natural lifetime expiry, canonical
queued Explode and immediate/delayed IF admission. Tests use a normally equipped
and fired authored gun, actual brick collision and ordinary Session steps.
No direct event dispatch, contact injection or immortality workaround is used.

Source formatting/whitespace checks pass. Owned paths are
`crates/events/src/{catalog,model,runtime}.rs`,
`crates/weapons/src/runtime.rs`,
`crates/sim/src/session/{events,weapons,rules}.rs` and
`crates/sim/tests/projectile_events.rs`. One dependent default initializer in
`session/packages/brick_events.rs` is coordinated with root.

## Focused validation complete

The first real-projectile test run compiled, but eight fixtures failed before
any shot because the one-stud-deep wall centre did not align with the ordinary
build grid. Its centre was corrected from z=-4 to z=-4.25; no admission,
collision, lifetime or expected outcome was weakened.

- `cargo test -p bri-sim --test projectile_events --locked -- --nocapture`:
  **9 passed**, 0.04 s; `/tmp/bri-v022-sol-projectile-events.log`.
- `cargo test -p bri-events --test runtime --locked`: **24 passed**, 3 existing
  ignored cases; `/tmp/bri-v022-sol-projectile-event-runtime.log`.
- `cargo test -p bri-weapons --test runtime --test tactical_seams --locked`:
  **32 + 23 passed**, 25 existing ignored runtime cases;
  `/tmp/bri-v022-sol-projectile-weapon-runtime.log`.
- `cargo clippy -p bri-events -p bri-weapons --lib --locked -- -D warnings` and
  `cargo clippy -p bri-sim --test projectile_events --locked -- -D warnings`:
  **pass**; `/tmp/bri-v022-sol-projectile-{core,sim}-clippy.log`.

This lane is source-frozen for root integration. Root still owns full-gate,
ignored/content regressions and platform packaging; Maxwell owns interactive
acceptance. Immediate responses remain the contact query's first authored
response, while delayed actions use the existing scheduler's due-time path.

Independent review briefly suspected guarded zero-delay rows could be admitted
and skipped. Complete catalog review withdrew that finding: validation explicitly
rejects immediate Projectile IF rows, and the focused fixture verifies rejection.
The supported immediate contract remains first response at contact; conditional
responses use the existing delayed scheduler. No executor change was made for
an unreachable input. This distinction must remain visible in creator admission.
