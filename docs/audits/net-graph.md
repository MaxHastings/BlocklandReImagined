# Net graph and performance overlay

Branch `claude/net-graph`, 2026-09-28. This adds two things: v20's own net
graph, and a modern performance overlay that goes beyond it. Both are
drawn above every screen the way v20 added `NetGraphGui` to the Canvas.
While hidden, neither draws, samples, nor writes a GPU timestamp.

## What v20 had

Source: `.research/v20-dso/client/ui/allClientGuis-Vanilla.gui` lines
1274–1520 and `allClientScripts-Vanilla.cs` lines 3116, 5866 and 15373, read
on this PC. The `.gui` and `.cs` files in the reference install
(`E:\…\Blockland v20\base\client`) have only the Ctrl+N bind. The GUI and
script come from the decompiled DSOs.

- **Key:** `moveMap.bind(keyboard, "ctrl n", toggleNetGraph)`. It is
  remappable as "Toggle NetGraph" in the Gui section of Options > Controls.
- **`NetGraphGui`:** a 640x480 transparent control with `horizSizing = "left"`,
  so it keeps its distance from the right edge. A `GuiGraphCtrl` at "432 5",
  200x200, has no background.
- **`NetGraph::updateStats`:** reschedules itself every 32 ms. Each time it
  adds one datum to each of six plots and sets six labels:

| Plot | Label and profile | Colour | Source |
|---|---|---|---|
| 0 | Ghosts Active (`NetGraphGhostsActiveProfile`, 436 156) | white | `getGhostsActive()` |
| 1 | Ghost Updates (536 156) | red | `$Stats::netGhostUpdates`, reset each sample |
| 2 | Bits Sent (436 170) | green | `$Stats::netBitsSent` |
| 3 | Bits Received (536 170) | blue, white outline | `$Stats::netBitsReceived` |
| 4 | Latency (436 184) | cyan | `getPing()` |
| 5 | Packet Loss (536 184) | black, white outline | `getPacketLoss()` |

  `NetGraph.matchScale(2, 3)` gives bits sent and received one shared scale.
  Every profile is Arial 14 with `doFontOutline`. Before the first sample,
  the labels show their bare names.
- **Disconnect:** calls `NetGraph.cancel()`. The graph stays on the canvas.

## What we do

### Net graph (Ctrl+N)

`bri_ui::models::perf::NetGraph` keeps the last 200 samples, newest first,
which is `GuiGraphCtrl`'s `MaxDataPoints`. `screens::perf::net_graph` draws
it at the authored place, right-anchored. It draws each plot as a
one-pixel line, newest sample at the right edge, scaled to its largest value
× 1.05 as `GuiGraphCtrl` did, with bits sent and received on one scale. The
labels use the converted `NetGraph*Profile` styles (Arial 14 cached font,
their colours and outlines) at their authored positions, centred in their
18-pixel height. Before a sample arrives they read "Latency", "Packet Loss"
and so on.

Ctrl+N comes from the converted default binds and v20's remap entry, so
rebinding in Options > Controls already works. The `netgraph` console
command toggles it too. The UI now owns the toggle. The old
`GameAction::ToggleNetGraph` and the one-line "FPS / Ping / Players" text
stand-in are gone.

What the six numbers mean here:

- **Ghosts Active:** replicated objects the client holds: players' poses,
  vehicles and Add-On entities.
- **Ghost Updates:** replica updates applied since the last sample. Each
  world delta and each pose, vehicle or orb datagram counts one
  (`bri_net::client::LinkProbe`). This is per message. Torque counted per
  object inside a packet.
- **Bits Sent / Received:** UDP bytes × 8 since the last 32 ms sample, from
  QUIC's own counters (`quinn::Connection::stats`), so headers and ACKs are
  included. Torque's numbers were per packet. Its client sent about one
  packet per 32 ms, so the scale is comparable.
- **Latency:** QUIC's smoothed RTT in milliseconds.
- **Packet Loss:** packets QUIC declared lost, as a percentage of packets
  sent in the last 4 seconds.

Label text is not clipped to its authored extent, only to the screen, so
long numbers stay readable. v20 would cut "Bits Received: 12345" at 100
pixels.

### Performance overlay (F3, not in v20)

F3 cycles **compact → expanded → off**. Ctrl+F3 saves a capture. Both are
new rows after "Toggle NetGraph" in Options > Controls ("Toggle Performance
Overlay", "Save Performance Capture"). Both are unbound in v20. The console
has `perf` and `perfcapture`.

The overlay is a dark panel in the top-right corner. It sits under the net
graph when both show, and package HUD panels anchored top-right move down
below both. It uses the Lucida Console 12 cached font, so digits don't jump
around. UI scale is an integer (2× at 1080p, 4× at 4K), so it stays crisp
and legible at any resolution. See the renders below.

- **Compact:** FPS and mean frame time over about the last second, CPU and
  GPU milliseconds, and a 120-frame bar graph coloured green (60 FPS or
  better), yellow (30 FPS or better) or red.
- **Expanded:** adds the worst frame and the swapchain wait. The graph grows
  to 240 frames, with the CPU's share of each bar in blue and guide lines at
  60 and 30 FPS. It also shows:
  - Server: ticks per second, then mean and worst step time.
  - Net: ping, loss, then KB/s and packets/s each way.
  - World: bricks, players, vehicles and entities.
  - Memory: working set and private bytes.
  - Add-On script time per package, per tick, busiest six.
  - The GPU's name.

Where each number comes from:

| Figure | Source |
|---|---|
| Frame time | Present to present, in `platform::Runner::render` |
| CPU | Main-thread work since the last present: UI update, `App::tick`, pump, then recording the frame's commands up to submit |
| Wait | Swapchain acquire plus `present`. VSync and a GPU-bound frame show up here. |
| GPU | `perf::GpuFrameTimer`: an empty compute pass writes a timestamp at the start and at the end of the frame's encoder. This needs only `TIMESTAMP_QUERY`, which the device already requests for Add-On layers. It shows "n/a" where the GPU has no timestamps. It reads back asynchronously, a few frames late. |
| Server | `bri_net::server::ServerPerf`, refreshed once a second by the host loop around `Session::step`. **Host only.** A player who joined another computer's server sees "another computer hosts". |
| Add-On scripts | `Session::take_package_script_time`: wall time around each `Runtime::call`, per package, averaged per step |
| Net | The same `LinkProbe` as the net graph, sampled every 32 ms while expanded |
| Memory | `K32GetProcessMemoryInfo` (working set, `PrivateUsage`) |

Cost while hidden: `Ui::draw` checks two fields. `App::update_perf` returns
straight away and resets the sampler. The platform writes no timestamps,
drops the GPU timer and skips `frame_timed`. Hiding the overlay clears its
history. The only always-on cost is on the host: an `Instant` pair around
each simulation step and each package script call, and a mutex write once a
second.

### Capture (Ctrl+F3)

This writes `captures/perf-<unix seconds>.json` under the state directory,
next to `screenshots/`. The bottom print names the file, and the console
echoes its full path. Schema `bri-perf-capture/1` contains:

- version, time and logical resolution;
- the summary and the slower figures;
- the 240 frames, newest first;
- the net graph's 200 samples when it is showing, otherwise the overlay's
  latest one.

It works whether the overlay is showing or not. With the overlay hidden the
frame history is empty.

## Wire and engine boundaries

- **No protocol change.** Server tick and script time reach only the host's
  own overlay, through `ServerHandle::perf`. Sending them to joined players
  would need a message and a protocol bump. That is left for later.
- **Engine crates** gain mechanisms only: `LinkProbe`/`LinkSample` and
  `ServerPerf` in `bri-net`, and per-package script time in `bri-sim`. The
  net graph, overlay, key binds, capture and GPU frame timer live in
  `bri-ui` and `bri-client`.
- The client's `windows-sys` gains the `Win32_System_ProcessStatus`
  feature. `Cargo.lock` is unchanged.

## Saved controls

Saved binds replace the defaults wholesale, so a player who saved controls
before this change would never get F3. `BindMap::add_missing_extras` binds
each new command, at load, only when the command has no binding and its
default key is free. A player who already put something on F3 keeps it, and
the overlay stays unbound until they pick a key. The tradeoff: a player who
deliberately clears the overlay's key (Options > Clear) gets F3 back on the
next start if F3 is still free.

## Tests

- `bri-ui` `models::perf`: 200-sample history, `matchScale`, per-second
  rates, mode cycle, one-second summary.
- `bri-ui` `screens::perf`: right-anchored plots with the newest at the edge
  and the 1.05 headroom; frame bars coloured by frame rate.
- `bri-ui` `tests/net_graph.rs`:
  - Ctrl+N toggles in game and drops samples while hidden.
  - F3 cycles and Ctrl+F3 asks for a capture.
  - The remap list order and key labels (F3, CTRL F3, CTRL N).
  - Saved controls gain the keys only where they are free, and rebinding
    works.
  - The overlays draw nothing when hidden.
- `bri-ui` `tests/net_graph.rs` `overlays_render_offscreen` (ignored; needs
  content and a GPU): renders both overlays at 1024x768, 1920x1080@2x and
  3840x2160@4x to `artifacts/perf-overlay/`. Run it with
  `BRI_UI_PACK=<content>/ui-pack-003 cargo test -p bri-ui --test net_graph -- --ignored`.
- `bri-client` `perf`: link counter differences, the loss window, process
  memory, frame timing units.
- `bri-net` `server`: one-second `PerfWindow` summary (rate, mean, worst,
  script time sorted).

Not verified here: the net graph and overlay in a live game window. Per
AGENTS.md that is Max's playtest. What to check:

1. Host a game and press Ctrl+N. Six live lines should appear top right,
   with labels.
2. Press F3 twice. The expanded panel should show 120 tps and the Add-On
   script times.
3. Join from a second PC. It should show "another computer hosts", with
   network figures.
4. Press Ctrl+F3. A file should appear in `captures/`.
