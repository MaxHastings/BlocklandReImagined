# Bot trap detection and vertical progress

The bot trap timer previously compared only horizontal displacement. A bot
that climbed more than the trap radius while remaining over the same point
could therefore retain its old stuck interval and eventually select suicide.
The timer and the guard against immediately repeating a self-respawn now count
vertical displacement when grounded, swimming, or jetting. An ordinary airborne
hop does not reset the timer merely at its apex. Horizontal behavior and the
existing timeout remain in place.

`cargo test --locked --profile playtest -p bri-sim trap_progress_tests --lib`
passed: slow vertical movement clears the interval, while a stationary bot and
one repeatedly hopping in place still become trapped. `cargo clippy --locked
--profile playtest -p bri-sim --lib -- -D warnings` passed. In a 14400-tick
headless replay the house target died at tick 1234. The cube replay still left
the target alive at 10 health, and bot 2 respawned at tick 13339 exactly as in
the previous run. Thus this fixes a demonstrated progress-classification
defect, but does not establish the cause of that respawn or solve cooperative
stacking. Private reports are `v043-house-vertical-progress.json` and
`v043-cube-vertical-progress.json` in the planning diagnostics directory.
