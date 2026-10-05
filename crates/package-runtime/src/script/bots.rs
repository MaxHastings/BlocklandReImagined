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

/// A kind as scripts see it. Its first names are counted, not listed
/// (`bot_first_name(kind, i)` reads one): a kind's whole list can be more
/// text than one script value holds (`MAX_SCRIPT_TEXT`).
fn kind_map(k: &BotKindView) -> Dynamic {
    map([
        ("id", k.id.clone().into()),
        ("name", k.name.clone().into()),
        ("first_names", Dynamic::from_int(k.first_names.len() as i64)),
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
    // A longer name keeps its first characters, as a player's does: a
    // throw here would discard everything else the call did.
    let name: String = text("name")?
        .trim()
        .chars()
        .take(MAX_BOT_NAME_CHARS)
        .collect();
    let name = name.trim_end().to_owned();
    if name.is_empty() {
        return fail("a bot needs a name");
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
        with(|i| {
            let snapshot = i.snapshot.clone();
            snapshot
                .bot_kinds
                .iter()
                .map(|k| view_in(i, None, || kind_map(k)))
                .collect::<Fallible<Array>>()
        })
    });
    // One kind by id, or `()`: what rules that want one kind ask, so the
    // answer is the same size however many kinds the Add-Ons provide.
    engine.register_fn("bot_kind", |kind: &str| {
        with(|i| {
            Ok(i.snapshot
                .bot_kinds
                .iter()
                .find(|k| k.id == kind)
                .map_or(Dynamic::UNIT, kind_map))
        })
    });
    // First name `i` (from 0) of a kind, or `()`.
    engine.register_fn("bot_first_name", |kind: &str, i: i64| {
        with(|inv| {
            Ok(inv
                .snapshot
                .bot_kinds
                .iter()
                .find(|k| k.id == kind)
                .and_then(|k| usize::try_from(i).ok().and_then(|i| k.first_names.get(i)))
                .map_or(Dynamic::UNIT, |n| n.clone().into()))
        })
    });
    // How many bots the server runs at once, from spawn bricks and rules
    // together; `bots()` lists those it runs now.
    engine.register_fn("bot_limit", || MAX_BOTS as i64);
    // The most characters a bot's name has; `add_bot` keeps that many of
    // a longer one.
    engine.register_fn("bot_name_limit", || MAX_BOT_NAME_CHARS as i64);
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
