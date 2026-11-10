//! What the engine does for the `bots` operations
//! (`bri_package_runtime::ops::bots`).
use super::*;

impl Perform for ops::AddBot {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        session.check_rules_bot(self.game, self.team, &self.kind)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        session.add_rules_bot(cx.package, &self)
    }
}
impl Perform for ops::RemoveBot {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.own_bot(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RemoveBot { bot } = self;
        session.remove_rules_bot(package, bot)
    }
}
impl Perform for ops::RestBot {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.check_rest_rules_bot(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RestBot {
            bot,
            rest,
            locomotion,
        } = self;
        session.rest_rules_bot(package, bot, rest, locomotion)
    }
}
impl Perform for ops::SetBotVoiceMute {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.check_rest_rules_bot(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetBotVoiceMute { bot, muted } = self;
        let peer = session.peers.get_mut(&bot).context("No such player")?;
        peer.voice_muted = muted;
        Ok(())
    }
}
impl Perform for ops::SetBotFacing {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.check_rest_rules_bot(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetBotFacing { bot, target } = self;
        session.rules_bot_facing(package, bot, target)
    }
}
impl Perform for ops::Obscure {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Obscure {
            at,
            radius,
            seconds,
        } = self;
        session.bot_cloud(Vec3::from(at), radius, seconds);
        Ok(())
    }
}
impl Perform for ops::DazeBot {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.is_bot(self.bot), "No such bot");
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::DazeBot { bot, seconds } = self;
        session.bot_daze(bot, seconds);
        Ok(())
    }
}
impl Perform for ops::BotTool {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.check_rules_bot_tool(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::BotTool { bot, slot } = self;
        session.rules_bot_tool(package, bot, slot)
    }
}
impl Perform for ops::SetBotIntent {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.check_rest_rules_bot(cx.package, self.bot)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SetBotIntent { bot, intent } = self;
        session.set_bot_intent(package, bot, intent)
    }
}
