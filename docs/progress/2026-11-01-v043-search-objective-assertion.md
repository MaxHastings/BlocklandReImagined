# Search/objective integration assertion

The extended-gate scenario `loss_of_sight_search_reacquires_and_actually_wins_elimination`
failed because it required the diagnostic string `unchecked-space probe` to
appear. A trace at offset zero showed the bot did lose sight, then reacquired
the human and caused the canonical credited death and round win; it simply did
not enter that particular diagnostic stage before reacquisition.

The scenario now measures actual bot movement toward its dated last-known
position while the target is hidden (at least 0.5 units), instead of requiring
that implementation-specific phase label. It retains its checks that hidden
evidence neither moves nor refreshes, the enemy is really reacquired, the
objective names the correct life, and the canonical death and round win occur.
Both translated layouts pass. All four `bot_search_objectives` cases, formatting
and changed-target Clippy passed with the playtest profile on Windows. This
proves this scenario's pursuit outcome; it is not proof that every search
frontier or building can be explored.
