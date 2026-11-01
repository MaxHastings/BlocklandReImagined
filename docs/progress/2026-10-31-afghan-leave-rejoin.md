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

Subsequently ran both actual saves with a fresh import of the pinned Slayer ZIP
in an isolated content copy. Both loaded with no set-aside bricks. The owner
left, the bots remained alive, seven previously present bots moved more than
half a unit during two simulated seconds without the owner, and the owner
rejoined in each save. The previous Slayer policy fails the same probe because
the bots' bodies disappear. The probe now has an explicit `--leave-rejoin`
mode and uses normal session commands, without writing the source saves.
Reports: `v043-afghan-leave-continue-rejoin.json` and
`v043-afghan-zombies-leave-continue-rejoin.json` in the private planning
evidence. This is one-host headless evidence, not a multiplayer or long-run
test. `cargo clippy --locked --profile playtest -p bri-net --bin
bot_scenario_probe -- -D warnings` passed.

The separate reported slow objective pursuit and its saved-world before/after
evidence are recorded in `2026-10-31-afghan-stair-route-feedback.md`.
