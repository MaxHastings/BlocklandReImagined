# 2026-10-31 Friend-host restart log audit

Read the supplied archive without executing or extracting its contents. The
private evidence copy under `planning-2026-10-30/diagnostics/friend-server-restarts`
is byte-identical to `E:/Downloads/some logs that had random restarts.zip`:
14,179 bytes, SHA256
`6dcaa2ca3d9ef0cda0d745b430e73a24b9b9e1c0cc68f6332e3f8450ed650ebf`.
Original files remain untouched. The logs identify Linux v0.4.2 `93c1439da`,
Vulkan and an NVIDIA RTX 5090 Laptop GPU.

The 668-line `client-20261031-051821.stderr.log` shows a hosted NYC day map,
reachable NAT-PMP hosting, then at line 164:
`The Wayland window connection failed; starting again on X11.` A new log
session/startup follows immediately, then another map load and server start.
This is concrete evidence of a full client-process relaunch interrupting the
in-process host, not an authoritative minigame restart. The log does not show
whether recovery/rehosting required an operator action, and does not establish
a cause for every other reported restart. Five short session logs contain only
version headers; the two stdout files are empty.

The exact v0.4.2/current source `system_libraries::relaunch_on_x11` handles any
winit `EventLoopError::ExitFailure` in a Wayland session with X11 available by
launching a new executable with Wayland environment removed. This runs after
`platform::run` returns, even when gameplay/hosting was already active. The
fallback was originally intended for a failure just after the first frame.
It cannot preserve the old process's live simulation or connections.

The archive does not record the underlying Wayland error. Pinned winit 0.30.13
sets exit code 1 without logging the error if its connection flush fails;
its dispatch-error branch does log. A flush failure is therefore a possible
explanation for the sparse diagnostic, not a proven compositor/driver cause.
Do not blame an add-on, promise an OS-driver fix, or treat another process
restart as successful session recovery.

Other anomalies: imported map lighting/object fallbacks and the known missing
Bot_Town Bot_Hole dependency are explicit compatibility warnings. Optional
ALSA/JACK device/backend warnings appear without a fatal audio error. Map and
graphics rebuilds include individual long frames (up to roughly 370 ms), while
later sustained samples report about 60 FPS. No explicit simulation-step panic,
GPU-memory exhaustion or final fatal server error is present in the supplied
long log. None of these warnings establishes the restart cause.

Next supported correction: make backend fallback/loss diagnostics explicit and
avoid presenting a runtime process relaunch as transparent hosting recovery.
Startup-only fallback and an explicit backend preference deserve evaluation;
separating host lifetime from the display process is a broader architectural
option, not a proven prerequisite for this release. Native Linux validation
is required for backend changes; Windows-only checks cannot establish them.
