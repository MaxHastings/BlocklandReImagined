# Bot spawn event team context

The existing core Player `setTeam` event can target the `Bot` slot; it assigns
an actual minigame team and is not equivalent to Bot_Hole's relationship-mode
event. A real event test initially showed `onBotSpawn -> Bot -> setTeam` was
rejected because that input has no activating player or minigame slot. The
rule now falls back to the target player's actual minigame when the trigger
has no game context. The existing source-owner and target-membership checks
remain authoritative.

The bot-team fixture passes a spawn-event team assignment after a replacement
life, retaining the team over later reconciliation. Its first attempt failed
because the fixture tried respawning before the existing 0.2-second cooldown;
waiting for that cooldown exposed the actual missing-context error. A separate
negative test confirms neither a foreign target game nor an explicit mismatched
game bypasses ownership/membership. Commands: `cargo test --locked --profile
playtest -p bri-chaos --test bot_team` (2), the focused sim unit
`target_game_fallback_cannot_change_another_owners_game` (1), and
`cargo clippy --locked --profile playtest -p bri-sim -p bri-chaos --lib
--test bot_team -- -D warnings` all pass. Initial spawn before a bot joins a
game is not established by this replacement-life test; Bot_Hole's relationship
mode and legacy event compatibility remain separate.
