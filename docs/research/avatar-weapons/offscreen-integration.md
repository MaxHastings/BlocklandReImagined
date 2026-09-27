# Offscreen original held-item verification

The ignored integration test `crates/client/tests/app_item_render.rs` drives the
normal `App` through `HostGame`, `GameAction::Look`, `UseTool`, and `UnUseTool`.
It loads the converted native v20 content tree, hosts Bedroom over loopback,
uses the null audio backend, and renders the regular App scene into a headless
wgpu target. No window or desktop/game input is used.

Run it from the workspace root with:

```powershell
cargo test -p bri-client --test app_item_render -- --ignored --nocapture
```

The test used a 640×480 target on an NVIDIA GeForce RTX 4070 SUPER. First-person
camera samples are fixed at `(yaw, pitch)` = `(π, 0.08)`, `(π/2, 0.28)`, and
`(π, -0.35)` radians. `GameAction::Look` is a mouse delta, so the test converts
those targets to deltas and accounts for the game's inverted screen-Y pitch.
At every angle it captures empty hands, then Hammer/Wrench/Printer, waits for
the replicated tool selection, and ticks App so its actual mount projection is
current. Every item frame has one visible mounted instance, one cached model,
and one geometry slot. Pixel changes against empty hands range from 13,597 to
20,323 of 307,200 pixels. The larger values are first-person models across the
image region and make a passing result meaningful even when camera aim points
at another part of the map.

The third-person sample leaves the local actor visible, enables the normal zoom
control at a 12-degree FOV to frame the avatar, selects Wrench, and compares
normal App renders with empty hands. It changes 206 pixels around the avatar's
held hand (one mounted instance/model/slot); `wrench-third-person-crop.png` is a
nearest-neighbor enlargement of the corresponding original frame region. The
observed item is at the authored hand mount; no guessed item offsets or hand
positions are used in the test.

After deselection the world-item projection reports zero visible item instances
and the frame returns to the empty-hand baseline within the test's 20-pixel
allowance for the live session. The selected third-person frame is byte-identical
after `gpu_stopped()` followed by `gpu_ready()` on the same offscreen device
(307,200 equal pixels). Captures and the numeric `report.json` are written under
ignored `artifacts/native-world-items/`; no reference image/golden is updated.

Inspected captures:

- `hammer-front.png`, `wrench-front.png`, `printer-quarter.png`: first-person
  world renders with their original pack geometry/materials.
- `wrench-third-person.png` and `wrench-third-person-none.png`: actual regular
  third-person scene, selected versus empty hands.
- `wrench-third-person-diff.png` and `wrench-third-person-crop.png`: pixel
  difference and enlarged hand area for checking the hand attachment.
- `wrench-third-person-reset.png`: render following GPU resource recreation.

The `Look` camera-angle samples establish first-person visibility under yaw and
pitch changes. The third-person sample is a static attachment/render check, not
a traversal or animation playtest. The parent/authoritative App path continues
to own actor animation and remote interpolation behavior.
