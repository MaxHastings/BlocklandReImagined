# 2026-10-31 Stacking contact baseline

The standard motor already accepts one living body standing on another in a
headless two-body contact case. After the lower body becomes a corpse, the upper
body falls through it and lands on the floor. The focused `bri-motor` test
`a_living_body_supports_another_until_it_becomes_a_corpse` and warning-free
all-target bri-motor Clippy pass. Diagnostics are `stack-living-support.txt` and
`stack-living-support-clippy.txt` in the planning evidence directory.

This establishes stationary physical support and death revocation only. It does
not establish jumping onto moving bodies, bot cooperation, replicated support,
scaled bodies, low ceilings or the saved cube attack. Next trace bot navigation
and ordinary jump controls against living-body contacts before selecting a bounded
cooperation mechanism. Preserve the corpse collision contract.
