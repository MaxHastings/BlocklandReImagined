# 2026-10-11 Soccer scoring test waits for a contested match

`bot_soccer_teams::a_bot_scores_in_its_own_teams_goal_after_save_and_reset`
failed on Linux main (cf9c761) with "no goal in 90 s" and passed on Windows
CI.

Traced every two seconds: both bots worked the ball the whole time, one
toward each goal. The ball passed each goal a few units to one side
(x -2.4 at the north goal, -3 to -5 past the south one) and play went on.
Nothing was stuck and no goal was wrong: two bots contesting one ball just
took longer on Linux floats. The first goal came at 116 s, in the scoring
team's own goal.

The test claims which goal a bot scores in, not how fast. Its bound is now
240 s, about twice the Linux time. All five `bot_soccer_teams` tests pass
locally.

Not done: aiming a pushed ball more precisely at a goal would score sooner.
That is a push-steering change of its own.
