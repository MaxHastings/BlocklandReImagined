# 2026-10-09 Authored sky and exposure-aware Soft Shading

Max rejected Enhanced Sky after playing preview r2 and requested removal from
both server Environment and client Options. Removed its atmosphere settings,
script API, protocol delta, GUI controls, renderer implementation and probe
switches. All maps now retain their authored skies. Soft Shading and AO remain
in Unified/Dynamic; their existing labels stay short and unchanged.

Max also authorized the long-term correction for indoor tint in this PR and
asked that all four review findings be addressed. Work in progress: a generic
geometry-derived skylight exposure mechanism using cached upper-hemisphere
depth maps per view. No map-name conditions or new GUI controls. Closed rooms
retain their ambient colour; openings expose a fraction of the sky. Classic
and the effects-off path retain the original shader. This is direct skylight
visibility, not a claim of full multi-bounce global illumination.

Windows CI error 206 is addressed by checking cargo fmt one workspace package
at a time through tools/check_format.py. Every workspace member is included;
no formatting, test or clippy check is removed. Enhanced-versus-authored sky
tint mismatch is eliminated by the removal of generated skies.

Initial Rust client/render/UI test-target compilation passes. Render acceptance,
moving geometry behavior, MSAA parity, GPU timing and the full gate still need
verification. The packaged r2 is unchanged while source work continues. No
merge, original-content commit or interactive game control occurred.
