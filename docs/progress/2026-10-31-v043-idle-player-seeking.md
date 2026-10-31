# Optional idle player seeking

Maxwell clarified the requested tracking option: a zombie-game builder can
choose whether otherwise idle bots move toward the nearest player. Added the
native brick event `setBotSeekPlayersWhenIdle(bool)`, default off. It picks the
nearest eligible human through the existing bot-enemy/game/life policy, with a
stable owner-ID tie break, and supplies only a navigation destination during
idle wandering. Normal sight and combat decisions remain separate. It releases
the idle route when disabled or when no eligible player remains, and does not
fight return-home while an eligible idle destination exists. A bot already near
the destination stops short instead of trying to occupy the player's body.
The override is scoped to the current brain life; builders can set it from
`onBotSpawn` to apply it to later lives.

An actual builder-event fixture with sight disabled first reproduced the
return-home conflict (distance 16.52 to 14.37 after five seconds); suppressing
that idle return while seeking fixed it without weakening the distance check.
The same test checks no attack without sight and return-home after disabling.
Separate cases check nearer eligible player selection and ignoring a nearer
player outside the game. The nearest-player fixture initially put the new player
at the game's configured spawn, not the test's join coordinate; the fixture now
sets that spawn explicitly. All ten bot-builder cases passed before the final
outside-game case was added; that final case passed independently. Grouped
formatting, `cargo clippy --locked --profile playtest -p bri-events -p bri-sim
-p bri-chaos --lib --test bot_builder -- -D warnings` and `git diff --check`
passed. The fixture reported its normal missing-hand warnings; an earlier run
also produced a near-target stall warning before adding the arrival stop.

Follow-up: the nearest-player fixture now moves that player with ordinary inputs,
checks the bot follows, kills that player and checks the bot redirects toward
the remaining live game member. All 11 `cargo test --locked --profile playtest
-p bri-chaos --test bot_builder` cases and scoped `bri-chaos --test bot_builder`
Clippy passed. This covers that death and moving-target scenario, not every
reconnection or team transition.

This is a first scoped implementation, not full navigation acceptance: team
changes, competing objectives, saved spawn rows and network
observers still need direct checks. The earlier selected-specific-player design
remains a separate unimplemented idea, not a claim made for this option. Large
bot-count cost also needs measurement; the opt-in selection scans live peers.
