//! Add-On rules over chat and kill lines: `on_chat` restyles or reroutes a
//! player's line (Slayer's Team Display Mode, dead talking), and
//! `on_death_message` changes the line a death prints (team-coloured names,
//! `(Teamkill)`, Bonus Kills' `(Killing Spree | 5)`, hidden lines). The
//! engine keeps rate limits, filters and the sender's own text; the rules
//! only decide how a line looks and who reads it.
use super::game_hooks::declaring;
use super::*;
use bri_package_runtime::rhai::{ImmutableString, Map};

/// Longest line rules may send.
const MAX_LINE: usize = 1024;

/// What `on_chat` made of a line.
pub(in crate::session) enum ChatAnswer {
    /// The engine sends it as usual.
    Engine,
    /// Not sent (the rules told the sender why, if they gave a reason).
    Dropped,
    /// Sent as written to these players (all, with `None`).
    Line {
        line: String,
        to: Option<Vec<OwnerId>>,
    },
}

/// What `on_death_message` made of a kill line.
pub(in crate::session) enum DeathLine {
    Engine,
    Hidden,
    Changed {
        victim: Option<String>,
        killer: Option<String>,
        /// Shown as the victim's own death, without the killer (Slayer's
        /// Hide Kills).
        hide_killer: bool,
        suffix: String,
        line: Option<String>,
        to: Option<Vec<OwnerId>>,
    },
}

/// Text a player typed, made safe to put in a rules line: no colour codes
/// or markup of their own.
pub(in crate::session) fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !(0xE000..0xE010).contains(&(*c as u32)))
        .map(|c| match c {
            '<' => '\u{2039}',
            '>' => '\u{203A}',
            c => c,
        })
        .collect()
}

fn text_of(v: &Dynamic) -> Option<String> {
    v.clone().try_cast::<ImmutableString>().map(|s| {
        s.chars()
            .filter(|c| *c != '\n' && *c != '\r')
            .take(MAX_LINE)
            .collect()
    })
}

impl Session {
    fn recipients(&self, v: Option<&Dynamic>) -> Option<Vec<OwnerId>> {
        let list = v?.clone().into_array().ok()?;
        Some(
            list.iter()
                .filter_map(|p| p.as_int().ok())
                .filter_map(|p| u64::try_from(p).ok())
                .filter(|p| self.peers.contains_key(p))
                .collect(),
        )
    }

    /// `on_chat(player, info)`: `info` is `#{ text, plain, name, clan_prefix,
    /// clan_suffix, team }`, with `plain` and the name parts safe to put in
    /// a line.
    pub(in crate::session) fn package_chat(
        &mut self,
        owner: OwnerId,
        text: &str,
        team: bool,
    ) -> ChatAnswer {
        let Some(host) = self.packages.as_ref() else {
            return ChatAnswer::Engine;
        };
        let hooks = declaring(host, |b| b.on_chat);
        let Some(peer) = self.peers.get(&owner) else {
            return ChatAnswer::Engine;
        };
        if hooks.is_empty() {
            return ChatAnswer::Engine;
        }
        let mut info = Map::new();
        info.insert("text".into(), text.to_owned().into());
        info.insert("plain".into(), plain(text).into());
        info.insert("name".into(), plain(&peer.name).into());
        info.insert("clan_prefix".into(), plain(&peer.clan.prefix).into());
        info.insert("clan_suffix".into(), plain(&peer.clan.suffix).into());
        info.insert("team".into(), team.into());
        for package in hooks {
            let reply = self.run_package(
                &package,
                "on_chat",
                vec![
                    Dynamic::from_int(owner as i64),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(reply) = reply else {
                continue;
            };
            if reply.is_unit() {
                continue;
            }
            if reply.clone().try_cast::<bool>() == Some(false) {
                return ChatAnswer::Dropped;
            }
            if let Some(reason) = text_of(&reply) {
                self.notify(owner, Notice::Chat(reason));
                return ChatAnswer::Dropped;
            }
            if let Some(map) = reply.clone().try_cast::<Map>()
                && let Some(line) = map.get("line").and_then(text_of)
            {
                return ChatAnswer::Line {
                    line,
                    to: self.recipients(map.get("to")),
                };
            }
            self.hook_warning(
                &package,
                format!(
                    "on_chat must return (), false, a reason or #{{ line, to }}, not {}",
                    reply.type_name()
                ),
            );
        }
        ChatAnswer::Engine
    }

    /// `on_death_message(victim, killer, info)`: `info` is `#{ kind, type,
    /// victim_name, killer_name, line }`, `line` the engine's.
    pub(in crate::session) fn package_death_message(
        &mut self,
        victim: OwnerId,
        killer: Option<OwnerId>,
        kind: &str,
        type_name: &str,
        line: &str,
    ) -> DeathLine {
        let Some(host) = self.packages.as_ref() else {
            return DeathLine::Engine;
        };
        let hooks = declaring(host, |b| b.on_death_message);
        if hooks.is_empty() {
            return DeathLine::Engine;
        }
        let name = |o: Option<OwnerId>| {
            o.and_then(|o| self.peers.get(&o))
                .map_or(Dynamic::UNIT, |p| p.name.clone().into())
        };
        let mut info = Map::new();
        info.insert("kind".into(), kind.to_owned().into());
        info.insert("type".into(), type_name.to_owned().into());
        info.insert("victim_name".into(), name(Some(victim)));
        info.insert("killer_name".into(), name(killer));
        info.insert("line".into(), line.to_owned().into());
        let killer_arg = killer.map_or(Dynamic::UNIT, |k| Dynamic::from_int(k as i64));
        for package in hooks {
            let reply = self.run_package(
                &package,
                "on_death_message",
                vec![
                    Dynamic::from_int(victim as i64),
                    killer_arg.clone(),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                None,
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(reply) = reply else {
                continue;
            };
            if reply.is_unit() {
                continue;
            }
            if reply.clone().try_cast::<bool>() == Some(false) {
                return DeathLine::Hidden;
            }
            if let Some(map) = reply.clone().try_cast::<Map>() {
                return DeathLine::Changed {
                    victim: map.get("victim").and_then(text_of),
                    killer: map.get("killer").and_then(text_of),
                    hide_killer: map
                        .get("hide_killer")
                        .and_then(|v| v.as_bool().ok())
                        .unwrap_or(false),
                    suffix: map.get("suffix").and_then(text_of).unwrap_or_default(),
                    line: map.get("line").and_then(text_of),
                    to: self.recipients(map.get("to")),
                };
            }
            self.hook_warning(
                &package,
                format!(
                    "on_death_message must return (), false or a map, not {}",
                    reply.type_name()
                ),
            );
        }
        DeathLine::Engine
    }
}
