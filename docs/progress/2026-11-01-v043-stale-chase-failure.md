# Reconsider a chase after its target moves beyond an old failure

In the open-door scenario the collision journal correctly reported both the
old and open door at tick 166. An independent fresh search from the bot's
stuck position found a crouching route to the visible target. The bot itself
had settled with a current chase goal but an `out_of_reach` marker for an old
target position more than `CHASE_SLACK` away. Its pursuit logic recognized the
old failure as stale and called `set_pursuit_goal`, but the goal already equaled
the current target, so `set_goal` did nothing and left the bot settled forever.

When that specific stale-marker case is settled, pursuit now clears the old
failure and settlement before requesting the current goal. A new unit test
covers the equal-current-goal/stale-failure case; the existing test confirms
that a materially unchanged unreachable target does not repeatedly replan.
The actual door opens and the bot goes through in the focused integration
scenario. All eight `bot_extras`, six `bot_hardening`, ten `bot_routes`, four
`bot_search_objectives`, both real Beta16 tower CTF variants, formatting and
changed-target Clippy passed in the Windows playtest profile.

With this correction and the level-route edge guard together, headless replays
of the supplied cube, staircase and balcony saves loaded every brick and killed
the target at sampled ticks 10921, 2401 and 3366 respectively. The same-setup
control without the edge guard measured 10921, 2401 and 3366. This is bounded
scenario evidence, not a claim that every door or pursuit is solved. Full
release-profile performance, the extended gate and platform CI remain required.
