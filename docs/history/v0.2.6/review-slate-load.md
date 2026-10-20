> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-slate-load.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: Slate load "freeze" (26e86779 on 498d7f58, claude/fix-slate-load-s5hahr)

Verdict: **passes. Merge exactly 26e86779.** It makes the wait honest and logs where a load sat. It doesn't make the wait shorter, and the cause is still inferred. The shorter-wait fix belongs in v0.2.7.

## 1. Correctness, wire

- **No wire change.** `bri_progress::Stage` has no serde derive (`crates/progress/src/lib.rs:18`), and `bri-net` uses it only in-process to report into the client's shared `Progress`. No protocol-changes file is needed.
- **`show_entry_wait`** (net_events.rs ~885-915):
  - First entry: it shows BUILDING BRICKS until `world_source` exists, then COMPILING SHADERS until `scene_pipelines_ready()`. It does nothing before `a.ready` or after `a.entered`.
  - Map change: COMPILING SHADERS only once the new map's scene is in. It re-`begin`s only when the stage actually changes, so it doesn't reset every tick.
  - `scene_pipelines_ready` is false while `gpu_restart` is pending or the renderer's worker is still building (probes.rs:8-19). It also covers the restart `render.rs:1384-1395` triggers when a map's effective shadow settings differ from the ones the renderer was built with (see 3).
- **Session log:** one line per stage change, with the previous stage's ms, then `IN GAME (after N ms)`. It logs only on a stage change, so there's no spam.
  - Nit: `log_entered` runs in two places (net_events.rs ~418 for a map change, ~952 on first entry). Its `take()` makes a second call print nothing, so it is safe.
- **Test:** it now asserts the held compile reads COMPILING SHADERS. The note says it failed before the change and passes after. Good.

## 2. Is the evidence enough for v0.2.6?

- **Enough for this change, yes.** The fix is correct whatever the cause: a wait on a local compile must not look like a finished network bar. It's in code being changed, and players hit it, so it fits the finish line.
- **The cause is plausible and fits every symptom:**
  - The first map sits on a full bar.
  - Esc leaves it.
  - The next map (Bedroom) loads at once, because the scene pipelines finished compiling in the background meanwhile. Leaving drops the scene, so the next join is a fresh entry against the already-built renderer.
  - 223 s on FXC was seen on 10-06.
  
  But it is **inferred**. The friend's log is the proof. The new log lines (`Compiled scene pipelines in N ms`, `Shader compiler: FXC`, and the per-stage times) will settle it next time.
- **A bounded real fix: yes, but it belongs in v0.2.7, not v0.2.6.**
  - **A persisted pipeline cache** helps only on Vulkan. wgpu's `PipelineCache` is Vulkan-only, and the slow FXC case is DX12, where the cost is the HLSL compile wgpu does before the driver. So a pipeline cache alone wouldn't fix this friend's case (inferred from wgpu's feature set; I didn't check the pinned wgpu version).
  - **The real levers, in order of payoff:**
    1. Compile only the variant in use first (this sample count and these shadow settings), and the rest later.
    2. When DX12 refuses DXC's shaders, prefer Vulkan with a persisted pipeline cache over falling back to FXC.
    3. Fewer scene shader variants.
  - Each is a change to the renderer and backend, not a small patch. Under the finish line it's v0.2.7: the player can now see what's happening, and nothing is broken.

## 3. Was a Slate-only trigger ruled out?

- **Partly.** The lane ruled out Slate-specific content and network paths on an NVIDIA PC: main and v0.2.5, single player, LAN and internet hosts, all three lighting modes, and a guest that had to download Add-Ons. None of them stall.
- **Not ruled out (GPU-dependent, so this PC can't show it):** the first map whose effective lighting (`light_volume.mode` to shadow settings) differs from what the renderer compiled at startup forces a **second full compile** (render.rs:1384-1395). Slate, as the usual first map, could trigger that on a GPU where Bedroom wouldn't. That's still "the compile", and this PR shows COMPILING SHADERS for it too. But it would mean the wait is per-map, not once per session.
  - Ask the lane for one line in the note: does entering Slate from the menu restart the scene pipelines on this PC (is a second "Compiled scene pipelines" line logged)? If so, add "compile the startup renderer with the settings the first map will use" to the v0.2.7 list. It is cheap and would halve the worst case.
- Bedroom loading after Esc doesn't tell the two apart. The friend's log will.
