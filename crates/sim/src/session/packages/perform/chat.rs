//! What the engine does for the `chat` operations
//! (`bri_package_runtime::ops::chat`).
use super::*;

impl Perform for ops::Tell {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::Tell { player, text } = self;
        ensure!(session.peers.contains_key(&player), "No player {player}");
        if caller == Some(player) {
            session.take_reply_line(package, player)?;
        } else {
            session.take_chat_line(package, caller)?;
        }
        session.notify(player, Notice::Chat(text));
        Ok(())
    }
}
impl Perform for ops::Broadcast {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package,
            caller,
            tick,
            ..
        } = cx;
        let ops::Broadcast { text } = self;
        let _ = tick;
        session.take_chat_line(package, caller)?;
        session.system_chat(text);
        Ok(())
    }
}
impl Perform for ops::TellMinigame {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::TellMinigame { game, text, except } = self;
        let members = session.minigame_members(game)?;
        session.take_chat_line(package, caller)?;
        for owner in members.into_iter().filter(|o| Some(*o) != except) {
            session.notify(owner, Notice::Chat(text.clone()));
        }
        Ok(())
    }
}
impl Perform for ops::TellPlayers {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::TellPlayers { players, text } = self;
        session.take_chat_line(package, caller)?;
        for owner in players {
            if session.peers.contains_key(&owner) {
                session.notify(owner, Notice::Chat(text.clone()));
            }
        }
        Ok(())
    }
}
impl Perform for ops::PrintMinigame {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PrintMinigame {
            game,
            text,
            seconds,
            bottom,
        } = self;
        let members = session.minigame_members(game)?;
        session.take_cue(package)?;
        let notice = if bottom {
            Notice::Bottom {
                text,
                seconds,
                hide_bar: false,
            }
        } else {
            Notice::Center { text, seconds }
        };
        for owner in members {
            session.notify(owner, notice.clone());
        }
        Ok(())
    }
}
impl Perform for ops::ShowReport {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::ShowReport { player, report } = self;
        session.package_show_report(package, player, report)
    }
}
impl Perform for ops::MessageBox {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::MessageBox {
            player,
            title,
            text,
        } = self;
        session.take_cue(package)?;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(player, Notice::MessageBox { title, text });
        Ok(())
    }
}
impl Perform for ops::Ask {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Ask {
            player,
            title,
            text,
            command,
        } = self;
        session.take_cue(package)?;
        ensure!(session.peers.contains_key(&player), "No such player");
        let declared = session
            .packages
            .as_ref()
            .and_then(|host| host.catalog.packages.get(package))
            .and_then(|p| p.behaviour.as_ref())
            .and_then(|b| b.commands.iter().find(|c| c.name == command))
            .is_some_and(|c| c.args.is_empty() && !c.tool_only);
        ensure!(
            declared,
            "ask's command `{command}` must be one of the package's own, with no arguments, that players may send"
        );
        session.notify(
            player,
            Notice::Question {
                title,
                text,
                package: package.into(),
                command,
            },
        );
        Ok(())
    }
}
impl Perform for ops::PlantError {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::PlantError { player, error } = self;
        use crate::simulation::PlantFailure as F;
        session.take_cue(package)?;
        ensure!(session.peers.contains_key(&player), "No such player");
        let failure = match error.as_str() {
            "overlap" => F::Overlap,
            "float" => F::Float,
            "stuck" => F::Stuck,
            "buried" => F::Buried,
            "too_far" => F::TooFar,
            // Planting too soon, as the engine's own plant rate
            // refuses it.
            _ => F::Limit,
        };
        session.notify(player, Notice::PlantError(failure));
        Ok(())
    }
}
impl Perform for ops::Print {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Print {
            player,
            text,
            seconds,
            bottom,
            hide_bar,
        } = self;
        session.take_cue(package)?;
        let notice = if bottom {
            Notice::Bottom {
                text,
                seconds,
                hide_bar,
            }
        } else {
            Notice::Center { text, seconds }
        };
        match player {
            Some(player) => {
                ensure!(session.peers.contains_key(&player), "No such player");
                session.notify(player, notice);
            }
            None => {
                let everyone: Vec<OwnerId> = session.peers.keys().copied().collect();
                for owner in everyone {
                    session.notify(owner, notice.clone());
                }
            }
        }
        Ok(())
    }
}
