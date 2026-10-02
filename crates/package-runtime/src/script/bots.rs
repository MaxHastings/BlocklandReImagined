//! Bots a mini-game's rules add (Slayer's `addBotToGame` and Preferred
//! Player Count): which bot kinds the enabled Add-Ons provide, and adding,
//! resting and removing the package's own bots. Their brain is the
//! engine's; which kind plays, on which team, is the rules'.
use super::*;
use crate::ops;
use crate::ops::{MAX_BOT_NAME_CHARS, MAX_BOTS};

/// A bot kind an enabled Add-On provides, as scripts see it
/// (`bot_kinds()`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotKindView {
    pub id: String,
    pub name: String,
    /// First names its bots may go by (a name list such as v20's
    /// `getRandomFirstName`), possibly none.
    #[serde(default)]
    pub first_names: Vec<String>,
}

fn kind_map(k: &BotKindView) -> Dynamic {
    map([
        ("id", k.id.clone().into()),
        ("name", k.name.clone().into()),
        (
            "first_names",
            Dynamic::from_array(k.first_names.iter().map(|n| n.clone().into()).collect()),
        ),
    ])
}

fn add_bot(game: Dynamic, options: Map) -> Fallible<()> {
    for key in options.keys() {
        if !["kind", "team", "name"].contains(&key.as_str()) {
            return fail(format!(
                "add_bot options are kind, team and name, not `{key}`"
            ));
        }
    }
    let text = |key: &str| -> Fallible<String> {
        match options.get(key) {
            Some(v) if v.is_string() => Ok(v.clone().into_string()?),
            _ => fail(format!("add_bot needs `{key}`: text")),
        }
    };
    let kind = text("kind")?;
    let name = text("name")?;
    if name.trim().is_empty() || name.chars().count() > MAX_BOT_NAME_CHARS {
        return fail(format!(
            "a bot's name is 1 to {MAX_BOT_NAME_CHARS} characters"
        ));
    }
    let team = match options.get("team") {
        None => None,
        Some(t) => games::optional_id(t)?,
    };
    push(Op::AddBot(ops::AddBot {
        game: id(&game)?,
        team,
        kind,
        name,
    }))
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("bot_kinds", || {
        with(|i| Ok(i.snapshot.bot_kinds.iter().map(kind_map).collect::<Array>()))
    });
    // How many bots the server runs at once, from spawn bricks and rules
    // together; `bots()` lists those it runs now.
    engine.register_fn("bot_limit", || MAX_BOTS as i64);
    // `add_bot(game, #{ kind, name, team })`: a bot of `kind` named `name`
    // joins `game`, on `team` when given. It joins next, when the
    // operations run; the rules hear it as a member who joined.
    engine.register_fn("add_bot", add_bot);
    engine.register_fn("remove_bot", |bot: Dynamic| {
        push(Op::RemoveBot(ops::RemoveBot { bot: id(&bot)? }))
    });
    // `bot_tool(bot, slot)`: that tool slot in its hand, or () to put
    // its tools away.
    engine.register_fn("bot_tool", |bot: Dynamic, slot: Dynamic| {
        let slot = if slot.is_unit() {
            None
        } else {
            Some(
                slot.as_int()
                    .ok()
                    .and_then(|s| u8::try_from(s).ok())
                    .ok_or("a tool slot is 0 or more, or ()")?,
            )
        };
        push(Op::BotTool(ops::BotTool {
            bot: id(&bot)?,
            slot,
        }))
    });
    engine.register_fn("rest_bot", |bot: Dynamic, rest: bool| {
        push(Op::RestBot(ops::RestBot {
            bot: id(&bot)?,
            rest,
        }))
    });
}
