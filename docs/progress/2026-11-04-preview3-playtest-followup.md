# 2026-11-04 Preview.3 playtest follow-up

Maxwell accepts NPC gameplay and requests no further NPC expansion beyond clear
design defects. The release preparation's uncommitted NPC experiments were
archived outside Git and reverted to the previously checked implementation.
Their diagnostic results remain evidence, not gameplay objectives.

Destruction is not accepted: Pirate World still feels stiff, vertical and
intermittently delayed, with missed reactions to the player's flying body.
The actual preview.3 client hash is unchanged:
`e30384626d73ad04dbbf3e533d6c507bf6490e80941c50dd77212ce8d565e3b8`.
This rules out an accidentally replaced executable, not every content/runtime
difference. Prior probes measured fallback debt, dropped cues and post-contact
spin. Actor interaction fixtures explicitly set the section to landed, omitting
the intact-flight phase. The intact section's motion path queries Building
static geometry; actor interactions enter the independent chunk-motion frame.
The existing angular correction begins at first static contact, while overflow
still has translation-only motion. Those limitations are relevant to the report;
passing those probes did not establish responsive, natural Pirate World motion.
Next correction must examine visible onset, motion during flight, actor contact
across phases and budget behavior together, without manufacturing random spin.

The supplied Speedkart report identifies both hands leaving the wheel for a
one-handed gun. A concrete interface fault was found: `image_support` can derive
a possible foregrip for any modeled weapon, including a fallback position;
`release_seat_holds` treated that availability as proof both hands were busy.
Avatar IK already gates actual foregrip use on the Both pose. Grip release now
uses the declared hand occupancy alone, removing the redundant support boolean
and the assertion that encoded the wrong one-handed behavior. Right, left,
both and stowed occupancy coverage remains. This edit is formatted and passes
diff whitespace checks; compilation, affected checks and rendered verification
remain pending. Nothing in the running preview was modified.

Head movement under mouse/strafe steering and vehicle balance remain source
comparison work. The native Jeep declares pitch/roll forces; apparent chassis
tilt alone does not prove a changing center of mass. These should be traced
through generic seat presentation and vehicle controls rather than keyed to the
Speedkart name. No speculative physical or head-animation change is included.

Maxwell is playtesting: no local builds, GPU suites, full gate or benchmarks were
started for this follow-up. Release publication and the destruction merge remain
held pending the corrected candidate, required checks and gameplay acceptance.
