//! What the engine does for the `bots` operations
//! (`bri_package_runtime::ops::bots`).
use super::*;

impl Perform for ops::AddBot {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::AddBot {
            game,
            team,
            kind,
            name,
        } = self;
        session.add_rules_bot(package, game, team, &kind, &name)
    }
}
impl Perform for ops::RemoveBot {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RemoveBot { bot } = self;
        session.remove_rules_bot(package, bot)
    }
}
impl Perform for ops::RestBot {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RestBot { bot, rest } = self;
        session.rest_rules_bot(package, bot, rest)
    }
}
impl Perform for ops::BotTool {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::BotTool { bot, slot } = self;
        session.rules_bot_tool(package, bot, slot)
    }
}
