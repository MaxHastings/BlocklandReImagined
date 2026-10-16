# Pixel wheel display-density correction

Base: published v0.3.1 `5956e3a383a1c241bd5192a3aa8cc4f358f2e503`.
Root-owned branch: `codex/v032-linux-wheel-feedback`.

Pinned winit's Wayland and precise macOS paths convert logical scrolling pixels
to physical pixels. Client treated physical Y as logical units before its
40-pixel step accumulator. At display scale 2 the same logical gesture therefore
produced two inventory/paint steps. This is a demonstrated defect, not proof of
ZaRR's physical backend or device: CachyOS is reported, but session type, device
and display/UI scale remain unknown.

Extracted the existing event mapping without changing baseline semantics, then
executed new regressions. Two failed on the original mapping (session 36098,
exit 101), including actual Ui paint swatch 6 instead of 7 at display scale 2 / UI
scale 1. Corrected PixelDelta to logical Y using the actual window scale before
accumulation. LineDelta units/sign and existing fractional accumulation remain
unchanged. Invalid display factors return no movement without poisoning state.

Validation on final source:

- New targeted tests: 3 passed, exit 0 (56943), including repeated density
  invariance, fractional gestures across density changes, line/sign and invalid
  scale controls, and actual Ui selection with two paint divisions across
  display/UI scales.
- Existing legacy X11 integration: 3 passed, 0 failed.
- Client quick: exit 0; format and 587 passed / 0 failed / 76 existing ignored,
  63 seconds.
- Client focused: exit 0; format, all-target Clippy with warnings denied and
  587 passed / 0 failed / 76 existing ignored, 148 seconds. Chain session 90316.

Logs remain untracked in this owned worktree as
`target-wheel-followup-{red,green,x11,quick,focused}.log`.
Only platform.rs and this evidence note belong to the commit. Required Windows
CI and combined candidate validation remain pending; no integrated pass or
physical reporter resolution is claimed. Recovered v20 scrollInventory direction
matches the current negative-to-next convention; no global reversal/debounce.
No original content, published archive, preview, input protocol or dependency
was changed. No interactive gameplay was performed.
