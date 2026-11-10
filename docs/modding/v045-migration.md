# PR 442 interface adjustments

These changes apply to the reviewed engine interfaces first proposed in
PR 442. They do not establish acceptance of the separately supplied RenderMan
Add-Ons, whose assets and private test harness were unavailable during review.

- `state_of(package, player, key)` reads only keys declared with
  `"exported": true` in the provider's `behaviour.json` state schema.
  `visible` continues to control which clients receive a value; `server`
  does not expose private state to other packages. For example,
  `"flashlight_on": { "default": false, "visible": "server", "exported": true }`.
- `steer_view(p, at, rate)` and the five-argument form now last one second.
  Renew them while needed or use
  `steer_view(p, at, rate, grip, fov_delta, seconds)` with a 0.1–60 second
  lease. `steer_view(p)` clears immediately. Expiry restores sensitivity and
  FOV offset even if the body/eye is absent. The turn uses one proportional
  yaw/pitch budget; it has no special horizontal-pan preference.
- Replace image `aim_lock_axis: [x, y, z]` with
  `aim_lock: { "axis": [x, y, z], "max_degrees": 60 }`. Choose the limit
  for the authored rig. This aligns the cosmetic wrist with action aim;
  free-look does not steer the hand, and authoritative hit paths are unchanged.
- `rest_bot(bot, true)` retains its original stop-and-hold behavior even
  with an existing follow/patrol intent. Use `rest_bot(bot, true, true)` to
  opt into anchored locomotion with native tactics suspended. This uses
  ordinary movement controls and does not promise obstacle navigation.
- `add_bot` preserves normal stock spawn presentation unless its options
  include `quiet: true`. `muted: true` independently publishes the advisory
  NPC voice flag; reaction Add-Ons must honor it.
- `screen_image` accepts `scale` (0.01–1, default 1) and optional RGBA
  `background` (default none). For the original small image over black, pass
  `scale: 0.5, background: [0.0, 0.0, 0.0, 1.0]`. `images` and `sequence`
  are mutually exclusive. Index keys now use stable namespaced IDs such as
  `support_render:image/render_0`, with sequence prefix
  `support_render:image/render_`. They are independent of install paths;
  `ui-images.json` uses schema 1, at most 128 frames, verified hashes and
  actual dimensions from 1 to 1024 per side, and at most 16 million total pixels per index (64 million across enabled Add-Ons).
  The authoring checker and actual loader share this schema and byte validation.
- Targeted `screen_flash`, `screen_overlay`, `screen_image`, `camera_shake`,
  `steer_view` and `stop_sound` share a bounded allowance of 64 controls per
  package per recipient per second. Broadcast cues retain the package's
  separate 64/s allowance and are charged once. Send meaningful changes or
  lease renewals rather than a control every simulation tick.
- `set_avatar_unlit(p, material, false)` restores normal lighting; the
  two-argument form enables it. At most eight material names are retained.
  Opaque artwork's alpha blends self-lit art over the actual lit body paint,
  without a guessed background color or an implicit interpretation of
  unrelated renderer flags. `stop_sound` also cancels queued matching sounds.

For authoring assets and native rules as one folder/ZIP, see
[co-located Add-Ons](co-located-addons.md). Existing separate packages remain
supported; no private Add-On has been silently bundled with the release.

Kept off-hand intent now survives weapon-runtime checkpoint round trips.
Weapon checkpoint schema is 5; earlier alpha weapon-runtime checkpoints are
refused, with no migration. Ordinary imported BLS builds retain their loader.
