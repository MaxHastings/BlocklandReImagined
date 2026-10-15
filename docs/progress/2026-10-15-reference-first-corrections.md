# 2026-10-15 Reference-first corrections after Maxwell playtest

Branch: codex/v031-remaining-capabilities. Maxwell rejected the private slice's
presentation: misplaced first-person flashes, casing position/direction, turret
aim, particle blending/occlusion, stock-tool body substitutions, locked Chief
FOV, weapon hold and HUD appearance, and sliding locomotion over hills.
Improved Warthog handling was reported; keyboard precedence is retained.

The replacement loop is original reference, source inspection, shared mechanism,
matched rendered comparison, and human visual/feel acceptance. Tests are only
regression protection. Maxwell requested no more test runs in this correction
pass. No new automated tests were run for these changes. Compilation is not
visual acceptance. Do not publish or claim slice readiness from prior probes.

Source candidates implemented:
- Image.first_person_nodes: a bounded authored map from logical attachment names
  to the local first-person model's nodes. World lookup remains unchanged.
- Item effect transforms preserve the socket's roll instead of reconstructing
  only an emission direction. The private model has separate first-person nodes.
- Vehicle attachment socket lookup uses the attachment's yaw/pitch and sway, with
  look-clip sampling where authored. Vehicle ejection uses ejectPoint when present.
- Whole attachments without a barrel look clip now receive pitch in both their
  visible model and socket path; previously visible geometry omitted it.
- Bodies without a requested stock-tool action sample the reference tool action's
  socket displacement only. Foreign body bones/vertices keep their own motions.
  Private borrowed punch/throw aliases are removed; no substitute body clip.
- Private Chief FOV override removed from generator, private output and installed
  archetype configuration. This one configuration change applies on reload.

Evidence: cargo check --locked -p bri-client -p bri-vehicles-import passes.
No new executable installed; no game window/input automated. These candidates
still require matching rendered evidence and Maxwell acceptance.

Source audit found all three imported bodies lack ground motion and authored
speeds; playback defaults to 1x. Original source animation frame-info contains
movement. Recover that data before guessing playback speeds. Slope contact and
jump/root transition behavior remains a separate investigation.
Particle import also collapses perpendicular-to-direction into parallel ribbons,
and source blend operations include component-max and alpha-multiply-add that
are not represented. Existing sprite depth testing alone does not close the
reported occlusion defect; inspect ordering, transparent geometry and per-view
selection. Do not patch it by disabling depth or adding arbitrary offsets.

The older 0c877cb3 baseline gate failed chat_links::clicking_a_chat_link_asks_before_opening_it.
Several timing-sensitive cases passed isolated retry; these facts do not certify
presentation or authorize release. New changes and remaining inventory HUD work
are not part of that baseline. Keep release and acceptance incomplete.
