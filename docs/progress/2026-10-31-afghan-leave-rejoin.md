# Afghan creator leave and rejoin

Maxwell reported that leaving the saved TDM Afghan or TDM Afghan with Zombies
minigame makes bots disappear and rejoining says he is already in it; he also
reported bots idling or taking too long to pursue objectives. The saved headers
were read without modifying the saves. Both contain Slayer CTF, two teams with
preferred bot count eight and no explicit `reset_when_empty` setting. The
installed Slayer package's default is true: its last-human-leaves handler
removes rules bot bodies and pauses the round, even while the owner wanted the
bot battle to continue. Changed the imported Slayer policy's default to false;
an explicitly enabled setting still retains its pause/reset behavior. A fresh
Slayer import is required for this change to appear in a packaged build.

Separately, a Slayer-authorized join used the management helper that prefers
owned game over current membership for its duplicate-member check. This falsely
rejected an absent owner rejoining their own encounter. The check now uses the
actual membership; management ownership semantics are unchanged. A session
regression passes for duplicate actual membership, leave/rejoin, and returning
from another game. A saved-build Slayer fixture with default settings passes
leave, live-bot survival, and rejoin; a separate explicit reset-when-empty
fixture confirms that opting into the old behavior still removes bot bodies.
The targeted commands were `cargo test --locked --profile playtest -p bri-sim
--lib granted_join_uses_membership_instead_of_ownership` and the two focused
`bri-addon-import --test slayer` cases. The synthetic tests are not yet an
actual replay of either Afghan save; preview.2 does not contain these fixes.

The reported slow objective pursuit is a separate open regression. Both saves
have bot-filled CTF teams; reproduce their initial objectives, planning and
movement under the actual saved world and compare with an earlier build after
the active play session. The new test also emitted a stall warning in its
synthetic battle, but that alone does not establish the Afghan cause.
