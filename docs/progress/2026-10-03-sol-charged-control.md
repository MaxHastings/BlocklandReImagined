# Charged throw continuity

The user reports that a bot sometimes winds up its Spear repeatedly without
throwing, especially while the target moves. The source has two cancellation
paths that fit this symptom: transient turn/range/path readiness clears the
brain's firing flag and cancels a held charged image; failed post-movement
release validation also calls native charge cancellation. Cancellation remounts
the selected image, restarting its authored wind-up. This is source evidence;
an interactive reproduction is outside the agent testing boundary.

The scoped correction separates harmless native holding from shot admission.
Only a descriptor-proven release-only image without cooking can retain its
charge while tracking the same live participant, actor life, slot and native
launch metadata. A temporary aiming/intercept/path/validation-budget wait keeps
the native state and ordinary held trigger. The actual release still passes
the existing post-movement trajectory, collision and ally checks. A failed
release becomes a held wait after identity/authority/ammo validation; real
preemption, target/life/equipment changes and unsupported graphs still cancel.
The gate clears the speculative release and synchronizes the brain's held
button, avoiding a second press that would itself release the image first.
Authored held transitions remain authoritative; no item IDs decide behavior.

Cancellation also uses a finite 128-state trigger-up/timeout/ammo reachability
proof. Wind-up or Armed states whose release can reach `onFire` restart safely
on genuine cancellation. Fire/recovery states that can only reach Ready retain
their authored cooldown, including when the next charge's press arrived early.
The unit test includes a recovery self-loop into Fire as a conservative negative.

The three new content-free actual-control fixtures in `bot_tactics.rs` are:

- `a_moving_target_keeps_native_windup_until_a_valid_throw`: renamed charged
  image, an 84-tick authored wind-up, slow authored turning and ordinary target
  strafe/stop controls. It requires uninterrupted Charge/Armed before real Fire,
  a meaningful Armed hold, observed native projectile and canonical damage.
- `replacing_the_charged_equipment_cancels_without_throwing`: ordinary MiniGame
  loadout replacement during actual Charge, no old charged projectile and
  canonical damage recovery through the replacement weapon.
- `disconnecting_the_charge_target_cancels_without_a_stale_throw`: a real
  participant disconnect during Charge, followed by no old projectile and no
  retained Charge/Armed state.

Source and fixtures are formatted and diff-check clean. Root owns serialized
compilation and regression scheduling; no compile or passing result is claimed
yet. No visible game, gameplay automation, content mutation or performance run
is involved.

Root's independent run `/tmp/bri-v022-charged-combat-regressions.log` passed all
eight tactics fixtures in 0.19 seconds, including the three new cases. The
moving-target chronology was Charge at relative tick 0, Armed at 83 and actual
Fire at 361, with 278 observed Armed-held ticks, a native projectile and real
damage. Equipment replacement and actual disconnection canceled without an old
throw; the existing direct/splash/charge/melee, blast, flight and ammo cases pass.

Final gate review tightened a validated `release_authorized=false` intent to
explicit `HoldCharge`, overriding any independently queued release instead of
merely allowing the prepared button state. A fourth actual-control negative,
`a_queued_release_cannot_fire_while_native_charge_admission_waits`, reaches real
Armed while the never-fired target remains in canonical initial protection,
queues ordinary trigger-up, and requires Armed retained with no native shot.
All actor/target life, hostility, equipment, metadata and ammo checks precede
this hold outcome. The final nine-test rerun and strict checking are pending;
the earlier eight-test result is not substituted for this final-source check.

Independent source review found two final boundaries: native advancement can
follow multiple trigger-up edges synchronously, and trusted release can leave
an Armed state with its button already up before participant cancellation.
The gate now uses the existing bounded reachability proof rather than only the
immediate next script; genuine cancellation uses that proof regardless of the
current button state. The conservative proof includes timeout successors, so
some wind-up ticks may also undergo validation under the unchanged shared
budget; a temporary failure holds safely.

The final source adds a chained-up reachability unit, an actual queued indirect
release while a live target has moved beyond slow turning, and an already-up
Armed/disconnect negative. Source/fixtures are frozen for root's serialized
eleven-fixture and focused-unit verification; these new results were pending at
that checkpoint.

Final-source verification is complete. Root's serialized `bot_tactics` target
passed **11/11** in 0.23 seconds; `/tmp/bri-v022-final-tactics-11.log` includes
the moving-target throw, actual equipment replacement/disconnect, protected
queued release, indirect queued-release trajectory validation and already-up
Armed cancellation, plus the five existing combat cases. The focused
`session::bots::charged_control` library filter passed **2/2** in 0.00 seconds
after a 6.78-second compile; `/tmp/bri-v022-final-charge-units.log` records both
native cooldown distinction and chained trigger-up reachability. Independent
source review closed both final boundary findings. This verifies headless
ordinary controls and canonical damage, without claiming an interactive Spear
playtest or a new performance measurement. Production and fixtures are frozen
for root integration; this final evidence update changes documentation only.
