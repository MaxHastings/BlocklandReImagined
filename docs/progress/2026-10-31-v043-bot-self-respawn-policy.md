# Bot self-respawn policy

The existing `respawn` behavior weight now defaults to zero. It governs a
living bot choosing suicide as an escape; it does not change the brick's
`auto_respawn` policy or its ordinary post-death lifecycle. An authored kind
can set a positive `behaviours.respawn` weight to opt back in; the existing
suicide permission and other guards still apply. This follows the approved
requirement that a pathfinding failure not routinely cause self-kills.

The real sealed-brick scenario now has a default-policy case: the trapped bot
stays alive for the 90-second run. The kind configuration test checks both
the default and an explicit positive weight. Both passed. Formatting and
`cargo clippy --locked --profile playtest -p bri-sim -p bri-chaos --lib --test
bot_brain -- -D warnings` passed.

There is a **known remaining failure**, not covered up: the previous
`a_bot_sealed_in_by_bricks_gets_out_eventually` test also failed on the
unchanged HEAD source and original test before this change. With its now-explicit
respawn opt-in, it still fails: its trap score never rises above zero in this
fixture. `BotThought::trap` now exposes the elapsed interval and score for
diagnosis. The controlled fixture's intervals reset while it stays enclosed;
the 10-second no-attempt expiry and the two-unit spatial test need an
outcome-based investigation. Do not lengthen a timer solely to force a pass.
The existing test remains present and failing, so it must be fixed before the
release gate. Private evidence: `v043-sealed-bot-original-baseline.txt`,
`v043-sealed-bot-explicit-current.txt`, and the controlled `...-resets.txt`.
This does not prove the reported house or cube respawn had the same cause.
