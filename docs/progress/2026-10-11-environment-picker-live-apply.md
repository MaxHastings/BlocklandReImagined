# 2026-10-11 Environment colour pickers apply live

Max reported that the Environment window's sliders auto-apply to the server
(v0.2.9) but the colour picker sub-dialogs (Shadow Color, Sun Flare, Ambient
and Direct light, Fog, Sky, Vignette) did not.

**Cause.** `ColorPicker` kept its own colour and wrote it to the draft only on
Done, so the revision the Environment window's live-send watches never moved
while the picker was open.

**Change.** Each slider move in the picker now writes the colour to the draft
(`crates/ui/src/screens/environment.rs`), so the existing settle delay, one
request in flight and rejected-change rules apply unchanged. The picker keeps
the draft as it was when opened; Cancel, Escape and the close box restore it
(`EnvironmentModel::restore_draft`), and the Environment window then sends the
revert if the host already holds the cancelled colour. Done keeps the value.

**Evidence.** `environment_changes_go_to_the_host_live_once_they_settle` now
also moves a picker slider (one request, picker still open) and cancels (one
request back to the earlier colour). `cargo test -p bri-ui` passes except the
offscreen GPU render tests, which fail identically on main in this container
(no GPU). Max's gate is the full check.
