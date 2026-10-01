//! Mini-games, teams and score as scripts see and change them, and the
//! bricks a game's rules care about (team spawns, flag stands): what
//! Slayer-style team games are built from.
use super::*;
use crate::ops::{GameRule, MAX_DROP_SECONDS, MAX_SCORE, MAX_TEAMS, TeamOp};
use crate::report::{ColumnChange, Report};
use bri_package::setting::SettingValue;

/// One mini-game as scripts see it (`minigames()`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinigameView {
    pub id: u64,
    pub title: String,
    /// The player who runs it, or `None` for the server's own (a game
    /// mode's).
    pub owner: Option<u64>,
    pub members: Vec<u64>,
    /// Counts up by one at every reset.
    pub round: u64,
    pub teams: Vec<TeamView>,
    pub friendly_fire: bool,
    pub ally_same_color: bool,
    /// A rule ended the round (`end_round`); the next reset starts another.
    pub round_over: bool,
    /// Its player type and start tools (`playerDatablock`, `startEquip0`
    /// to `4`), an empty id for an empty slot.
    #[serde(default)]
    pub player_type: String,
    #[serde(default)]
    pub loadout: Vec<String>,
    /// Points for killing a player (`points_KillPlayer`), which the
    /// engine gives the killer at every kill in the game.
    #[serde(default)]
    pub points_kill_player: i64,
    /// All of its own settings as the Mini-Game window shows them (title,
    /// invite_only, respawn_ms, enable_wand and the rest).
    #[serde(default)]
    pub settings: serde_json::Value,
    /// The server's default game (`set_default_minigame`).
    #[serde(default)]
    pub default: bool,
    /// Its v20 colour (0 to 9), and the paint colour rules gave it.
    #[serde(default)]
    pub color: u8,
    #[serde(default)]
    pub paint_color: Option<u8>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamView {
    pub id: u64,
    pub name: String,
    /// The team's paint palette index.
    pub color: u8,
}
/// A brick as scripts see it (`bricks(kind)`, `brick(id)`): #{ id, kind,
/// x, y, z, turns, min, max, color, owner, game, name, item }.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrickView {
    pub id: u64,
    /// Its definition: `namespace:brick/name` or `v20/brick/<datablock>`.
    pub kind: String,
    /// The middle of the brick.
    pub position: [f32; 3],
    /// Clockwise quarter turns seen from above.
    pub turns: u8,
    /// Its box.
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Its paint palette index.
    pub color: u8,
    /// The player (BL_ID) who owns it; 0 for the world's own.
    pub owner: u64,
    /// The mini-game whose bricks it is (its owner runs that game, or plays
    /// in it when it uses every player's bricks): v20's `minigameCanUse`.
    pub game: Option<u64>,
    /// Its name (`setNTObjectName`), or empty.
    pub name: String,
    /// The item it holds out (`setItem`), or empty.
    pub item: String,
}
/// An item a package's rules put in the world (`drops()`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DropView {
    pub id: u64,
    pub item: String,
    pub position: [f32; 3],
    /// What `drop_item` kept with it.
    pub data: Option<serde_json::Value>,
}
/// Most bricks `bricks(kind)` returns.
pub const MAX_BRICKS_LISTED: usize = 4096;

fn team_map(t: &TeamView) -> Dynamic {
    map([
        ("id", Dynamic::from_int(t.id as i64)),
        ("name", t.name.clone().into()),
        ("color", Dynamic::from_int(i64::from(t.color))),
    ])
}
fn minigame_map(g: &MinigameView) -> Dynamic {
    map([
        ("id", Dynamic::from_int(g.id as i64)),
        ("title", g.title.clone().into()),
        (
            "owner",
            g.owner
                .map_or(Dynamic::UNIT, |o| Dynamic::from_int(o as i64)),
        ),
        (
            "members",
            Dynamic::from_array(
                g.members
                    .iter()
                    .map(|m| Dynamic::from_int(*m as i64))
                    .collect(),
            ),
        ),
        ("round", Dynamic::from_int(g.round as i64)),
        (
            "teams",
            Dynamic::from_array(g.teams.iter().map(team_map).collect()),
        ),
        ("friendly_fire", g.friendly_fire.into()),
        ("ally_same_color", g.ally_same_color.into()),
        ("round_over", g.round_over.into()),
        ("player_type", g.player_type.clone().into()),
        (
            "loadout",
            Dynamic::from_array(g.loadout.iter().map(|i| i.clone().into()).collect()),
        ),
        ("points_kill_player", Dynamic::from_int(g.points_kill_player)),
        ("settings", super::to_dynamic(&g.settings)),
        ("default", g.default.into()),
        ("color", Dynamic::from_int(i64::from(g.color))),
        (
            "paint_color",
            g.paint_color
                .map_or(Dynamic::UNIT, |c| Dynamic::from_int(i64::from(c))),
        ),
    ])
}
fn game_rule(game: &Dynamic, rule: GameRule) -> Fallible<()> {
    push(Op::SetGameRule {
        game: id(game)?,
        rule,
    })
}
fn flag_of(v: &Dynamic, what: &str) -> Fallible<bool> {
    v.as_bool()
        .map_err(|_| format!("{what} is true or false").into())
}
fn settings_patch(map: Map) -> Fallible<serde_json::Value> {
    let json = super::to_json(&Dynamic::from_map(map))?;
    if !json.is_object() {
        return fail("mini-game settings are a map");
    }
    Ok(json)
}
pub fn brick_map(b: &BrickView) -> Dynamic {
    let [x, y, z] = position(b.position);
    map([
        ("id", Dynamic::from_int(b.id as i64)),
        ("kind", b.kind.clone().into()),
        x,
        y,
        z,
        ("turns", Dynamic::from_int(i64::from(b.turns))),
        ("min", super::point3(b.min)),
        ("max", super::point3(b.max)),
        ("color", Dynamic::from_int(i64::from(b.color))),
        ("owner", Dynamic::from_int(b.owner as i64)),
        (
            "game",
            b.game
                .map_or(Dynamic::UNIT, |g| Dynamic::from_int(g as i64)),
        ),
        ("name", b.name.clone().into()),
        ("item", b.item.clone().into()),
    ])
}
pub(super) fn optional_id(value: &Dynamic) -> Fallible<Option<u64>> {
    if value.is_unit() {
        Ok(None)
    } else {
        id(value).map(Some)
    }
}
fn palette_index(value: &Dynamic) -> Fallible<u8> {
    match value.as_int() {
        Ok(c) if (0..=255).contains(&c) => Ok(c as u8),
        _ => fail("a colour is a palette index, 0 to 255"),
    }
}
fn teams(value: Array) -> Fallible<Vec<TeamOp>> {
    if value.len() > MAX_TEAMS {
        return fail(format!("at most {MAX_TEAMS} teams"));
    }
    value
        .into_iter()
        .map(|t| {
            let t = t
                .try_cast::<Map>()
                .ok_or("a team is #{ name, color } (and its `id` to keep it)")?;
            let name = t
                .get("name")
                .and_then(|n| n.clone().into_string().ok())
                .ok_or("a team needs a `name`")?;
            let color = palette_index(t.get("color").ok_or("a team needs a `color`")?)?;
            let id = t.get("id").map(optional_id).transpose()?.flatten();
            Ok(TeamOp { id, name, color })
        })
        .collect()
}
fn set_teams(game: Dynamic, list: Array, options: Map) -> Fallible<()> {
    let flag = |key: &str| -> Fallible<bool> {
        options.get(key).map_or(Ok(false), |v| {
            v.as_bool()
                .map_err(|_| format!("`{key}` is true or false").into())
        })
    };
    for key in options.keys() {
        if !["friendly_fire", "ally_same_color"].contains(&key.as_str()) {
            return fail(format!(
                "set_teams options are friendly_fire and ally_same_color, not `{key}`"
            ));
        }
    }
    push(Op::SetTeams {
        game: id(&game)?,
        teams: teams(list)?,
        friendly_fire: flag("friendly_fire")?,
        ally_same_color: flag("ally_same_color")?,
    })
}
fn setting_dynamic(value: SettingValue) -> Dynamic {
    match value {
        SettingValue::Bool(b) => b.into(),
        SettingValue::Int(n) => Dynamic::from_int(n),
        SettingValue::Text(t) => t.into(),
    }
}
fn setting_value(value: Dynamic) -> Fallible<Option<SettingValue>> {
    if value.is_unit() {
        Ok(None)
    } else if let Ok(b) = value.as_bool() {
        Ok(Some(SettingValue::Bool(b)))
    } else if let Ok(n) = value.as_int() {
        Ok(Some(SettingValue::Int(n)))
    } else if let Ok(t) = value.into_string() {
        Ok(Some(SettingValue::Text(t)))
    } else {
        fail("a setting's value is true or false, a whole number, text, or () for its default")
    }
}
fn read_setting(game: &Dynamic, team: Option<&Dynamic>, key: &str) -> Fallible<Dynamic> {
    let game = Some(id(game)?);
    let team = team.map(id).transpose()?;
    with_world(|world, _| {
        world
            .setting(game, team, key)
            .map(setting_dynamic)
            .map_err(Into::into)
    })
}
fn write_setting(game: &Dynamic, team: Option<&Dynamic>, key: &str, value: Dynamic) -> Fallible<()> {
    if !bri_package::setting::is_setting_ref(key) {
        return fail(format!("`{key}` is not a setting key"));
    }
    push(Op::SetSetting {
        game: id(game)?,
        team: team.map(id).transpose()?,
        key: key.into(),
        value: setting_value(value)?,
    })
}
fn score(player: &Dynamic, value: &Dynamic, add: bool) -> Fallible<()> {
    let value = value.as_int().map_err(|_| "a score is a whole number")?;
    if value.abs() > MAX_SCORE {
        return fail(format!("a score is at most {MAX_SCORE} either way"));
    }
    push(Op::SetScore {
        player: id(player)?,
        value,
        add,
    })
}

fn drop_map(d: &DropView) -> Dynamic {
    let [x, y, z] = position(d.position);
    map([
        ("id", Dynamic::from_int(d.id as i64)),
        ("item", d.item.clone().into()),
        x,
        y,
        z,
        (
            "data",
            d.data
                .as_ref()
                .and_then(|v| rhai::serde::to_dynamic(v).ok())
                .unwrap_or(Dynamic::UNIT),
        ),
    ])
}
fn vector(value: &Dynamic, what: &str) -> Fallible<[f32; 3]> {
    let parts = value
        .clone()
        .into_array()
        .map_err(|_| format!("`{what}` is [x, y, z]"))?;
    match parts.as_slice() {
        [x, y, z] => Ok([float(x)?, float(y)?, float(z)?]),
        _ => fail(format!("`{what}` is [x, y, z]")),
    }
}
/// A report cell as text: numbers as they print, `()` blank.
fn cell(value: &Dynamic) -> String {
    if value.is_unit() {
        String::new()
    } else {
        value.to_string()
    }
}
/// A report from a script's map, its cells written as text.
fn report_from(mut report: Map) -> Fallible<Report> {
    if let Some(sections) = report.get_mut("sections")
        && let Some(mut sections_list) = sections.write_lock::<Array>()
    {
        for section in sections_list.iter_mut() {
            let Some(mut section) = section.write_lock::<Map>() else {
                continue;
            };
            let Some(rows) = section.get_mut("rows") else {
                continue;
            };
            let Some(mut rows) = rows.write_lock::<Array>() else {
                continue;
            };
            for row in rows.iter_mut() {
                let Some(mut row) = row.write_lock::<Map>() else {
                    continue;
                };
                if let Some(cells) = row.get_mut("cells")
                    && let Some(mut cells) = cells.write_lock::<Map>()
                {
                    for value in cells.values_mut() {
                        *value = cell(value).into();
                    }
                }
            }
        }
    }
    rhai::serde::from_dynamic(&Dynamic::from_map(report))
        .map_err(|e| format!("show_report: {e}").into())
}
/// `drop_item(item, #{ at, velocity, paint, data, seconds })`.
fn drop_with(item: &str, options: Map) -> Fallible<()> {
    for key in options.keys() {
        if !["at", "velocity", "paint", "data", "seconds"].contains(&key.as_str()) {
            return fail(format!(
                "drop_item options are at, velocity, paint, data and seconds, not `{key}`"
            ));
        }
    }
    let at = options.get("at").ok_or("drop_item needs `at`: [x, y, z]")?;
    let data = match options.get("data") {
        None => None,
        Some(d) if d.is_unit() => None,
        Some(d) => Some(
            rhai::serde::from_dynamic::<serde_json::Value>(d)
                .map_err(|_| "`data` must be plain values: numbers, text, arrays, maps")?,
        ),
    };
    let seconds = match options.get("seconds") {
        None => None,
        Some(s) => match s.as_int() {
            Ok(s) if (1..=i64::from(MAX_DROP_SECONDS)).contains(&s) => Some(s as u32),
            _ => return fail(format!("`seconds` is 1 to {MAX_DROP_SECONDS}")),
        },
    };
    push(Op::DropItem {
        item: item.into(),
        position: vector(at, "at")?,
        velocity: options
            .get("velocity")
            .map(|v| vector(v, "velocity"))
            .transpose()?
            .unwrap_or([0.0; 3]),
        paint: options.get("paint").map(palette_index).transpose()?,
        data,
        seconds,
    })
}
fn wear(
    player: Dynamic,
    image: Dynamic,
    slot: Dynamic,
    paint: Option<u8>,
    keep: bool,
) -> Fallible<()> {
    let slot = match slot.as_int() {
        Ok(s @ 2..=3) => s as u8,
        _ => {
            return fail("worn image slots are 2 and 3 (0 is the hand: mount_image(player, image))");
        }
    };
    push(Op::WearImage {
        player: id(&player)?,
        slot,
        image: if image.is_unit() {
            None
        } else {
            Some(
                image
                    .into_string()
                    .map_err(|_| "an image is a string like \"pkg:image/flag\", or ()")?,
            )
        },
        paint,
        keep,
    })
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("minigames", || {
        with(|i| {
            Ok(i.snapshot
                .minigames
                .iter()
                .map(minigame_map)
                .collect::<Array>())
        })
    });
    engine.register_fn("minigame", |game: Dynamic| {
        with(|i| {
            let game = id(&game)?;
            Ok(i.snapshot
                .minigames
                .iter()
                .find(|g| g.id == game)
                .map_or(Dynamic::UNIT, minigame_map))
        })
    });
    engine.register_fn("set_teams", |game: Dynamic, list: Array| {
        set_teams(game, list, Map::new())
    });
    engine.register_fn("set_teams", set_teams);
    engine.register_fn("set_team", |player: Dynamic, team: Dynamic| {
        push(Op::SetTeam {
            player: id(&player)?,
            team: optional_id(&team)?,
        })
    });
    engine.register_fn("set_score", |player: Dynamic, value: Dynamic| {
        score(&player, &value, false)
    });
    engine.register_fn("add_score", |player: Dynamic, value: Dynamic| {
        score(&player, &value, true)
    });
    // Add-On settings (`behaviour.json` `settings`).
    engine.register_fn("setting", |game: Dynamic, key: &str| {
        read_setting(&game, None, key)
    });
    engine.register_fn("setting_text", |game: Dynamic, key: &str| {
        let game = id(&game)?;
        with_world(|world, _| world.setting_text(game, None, key).map_err(Into::into))
    });
    engine.register_fn("team_setting_text", |game: Dynamic, team: Dynamic, key: &str| {
        let (game, team) = (id(&game)?, id(&team)?);
        with_world(|world, _| world.setting_text(game, Some(team), key).map_err(Into::into))
    });
    // How a setting is declared (its title and category for an
    // announcement, whether it is quiet), or ().
    engine.register_fn("setting_info", |key: &str| {
        with_world(|world, _| Ok(world.setting_info(key).map_or(Dynamic::UNIT, |v| to_dynamic(&v))))
    });
    // What the host keeps for these rules between games and restarts.
    engine.register_fn("host_data", |key: &str| {
        with_world(|world, _| Ok(world.host_data(key).map_or(Dynamic::UNIT, |v| to_dynamic(&v))))
    });
    engine.register_fn("set_host_data", |key: &str, value: Dynamic| {
        if !crate::ops::valid_host_key(key) {
            return fail(format!("`{key}` is not a key: lower-case letters, digits and _"));
        }
        let value = if value.is_unit() {
            None
        } else {
            let json: serde_json::Value = rhai::serde::from_dynamic(&value)?;
            Some(json)
        };
        push(Op::SetHostData {
            key: key.into(),
            value,
        })
    });
    // The lines of one of these rules' data files (`data` provides).
    engine.register_fn("data_lines", |file: &str| {
        with_world(|world, _| {
            Ok(world.data_lines(file).map_or(Dynamic::UNIT, |lines| {
                Dynamic::from_array(lines.into_iter().map(Into::into).collect())
            }))
        })
    });
    engine.register_fn("minigame_snapshot", |game: Dynamic| {
        let game = id(&game)?;
        with_world(|world, _| {
            Ok(world
                .minigame_snapshot(game)
                .map_or(Dynamic::UNIT, |v| to_dynamic(&v)))
        })
    });
    engine.register_fn("restore_minigame", |game: Dynamic, snapshot: Dynamic| {
        let snapshot: serde_json::Value = rhai::serde::from_dynamic(&snapshot)?;
        push(Op::RestoreMinigame {
            game: id(&game)?,
            snapshot,
        })
    });
    engine.register_fn("revive_bricks", |game: Dynamic| {
        push(Op::ReviveBricks { game: id(&game)? })
    });
    engine.register_fn("team_setting", |game: Dynamic, team: Dynamic, key: &str| {
        read_setting(&game, Some(&team), key)
    });
    // An RTB preference by its v20 global, `()` when no running Add-On
    // declares it.
    engine.register_fn("pref", |name: &str| {
        if !bri_package::setting::is_pref_global(name) {
            return fail(format!("`{name}` is not a $Pref::Server:: name"));
        }
        with_world(|world, _| Ok(world.pref(name).map_or(Dynamic::UNIT, setting_dynamic)))
    });
    engine.register_fn("server_setting", |key: &str| {
        with_world(|world, _| {
            world
                .setting(None, None, key)
                .map(setting_dynamic)
                .map_err(Into::into)
        })
    });
    engine.register_fn("set_setting", |game: Dynamic, key: &str, value: Dynamic| {
        write_setting(&game, None, key, value)
    });
    engine.register_fn(
        "set_team_setting",
        |game: Dynamic, team: Dynamic, key: &str, value: Dynamic| {
            write_setting(&game, Some(&team), key, value)
        },
    );
    engine.register_fn("end_round", |game: Dynamic, winners: Map| {
        let ids = |key: &str| -> Result<Vec<u64>, Box<EvalAltResult>> {
            match winners.get(key) {
                None => Ok(Vec::new()),
                Some(v) if v.is_unit() => Ok(Vec::new()),
                Some(v) => v
                    .clone()
                    .into_array()
                    .map_err(|_| format!("end_round's `{key}` is a list of ids"))?
                    .iter()
                    .map(id)
                    .collect(),
            }
        };
        for key in winners.keys() {
            if !["teams", "players"].contains(&key.as_str()) {
                return Err(format!("end_round's winners are teams and players, not `{key}`").into());
            }
        }
        push(Op::EndRound {
            game: id(&game)?,
            teams: ids("teams")?,
            players: ids("players")?,
        })
    });
    engine.register_fn("reset_minigame", |game: Dynamic| {
        push(Op::ResetMinigame { game: id(&game)? })
    });
    engine.register_fn("set_default_minigame", |game: Dynamic, on: Dynamic| {
        game_rule(&game, GameRule::Default(flag_of(&on, "the default")?))
    });
    engine.register_fn("set_minigame_color", |game: Dynamic, paint: Dynamic| {
        let paint = if paint.is_unit() {
            None
        } else {
            Some(palette_index(&paint)?)
        };
        game_rule(&game, GameRule::PaintColor(paint))
    });
    engine.register_fn(
        "set_minigame_region",
        |game: Dynamic, min: Array, max: Array| {
            game_rule(
                &game,
                GameRule::Region(Some([super::vector(&min)?, super::vector(&max)?])),
            )
        },
    );
    engine.register_fn("set_minigame_region", |game: Dynamic, _none: ()| {
        game_rule(&game, GameRule::Region(None))
    });
    engine.register_fn("set_keep_scores", |game: Dynamic, keep: Dynamic| {
        game_rule(&game, GameRule::KeepScores(flag_of(&keep, "keeping scores")?))
    });
    // Whether leaving the game clears a member's event objects and
    // schedules and respawns their vehicles (v20's), or keeps them
    // (Slayer's `removeMember`).
    engine.register_fn("set_cleanup_on_leave", |game: Dynamic, on: Dynamic| {
        game_rule(
            &game,
            GameRule::Cleanup {
                leave: flag_of(&on, "cleaning up on leaving")?,
            },
        )
    });
    engine.register_fn("set_claims_bricks", |game: Dynamic, on: Dynamic| {
        game_rule(&game, GameRule::ClaimsBricks(flag_of(&on, "claiming bricks")?))
    });
    engine.register_fn("set_name_distance", |game: Dynamic, distance: Dynamic| {
        let distance = if distance.is_unit() {
            None
        } else {
            Some(
                distance
                    .as_int()
                    .ok()
                    .and_then(|d| u32::try_from(d).ok())
                    .filter(|d| *d <= 8192)
                    .ok_or("a name distance is a whole number from 0 to 8192, or ()")?,
            )
        };
        game_rule(&game, GameRule::NameDistance(distance))
    });
    engine.register_fn("set_minigame", |game: Dynamic, settings: Map| {
        game_rule(&game, GameRule::Settings(settings_patch(settings)?))
    });
    engine.register_fn("end_minigame", |game: Dynamic| game_rule(&game, GameRule::End));
    engine.register_fn("create_minigame", |owner: Dynamic, settings: Map| {
        let mut settings = settings;
        let paint = match settings.remove("paint_color") {
            None => None,
            Some(v) if v.is_unit() => None,
            Some(v) => Some(palette_index(&v)?),
        };
        push(Op::CreateMinigame {
            owner: optional_id(&owner)?,
            settings: settings_patch(settings)?,
            paint,
        })
    });
    engine.register_fn("place_member", |player: Dynamic, game: Dynamic| {
        push(Op::PlaceMember {
            player: id(&player)?,
            game: optional_id(&game)?,
        })
    });
    engine.register_fn("set_brick_item", |brick: Dynamic, item: Dynamic| {
        push(Op::SetBrickItem {
            brick: id(&brick)?,
            item: if item.is_unit() {
                None
            } else {
                Some(
                    item.into_string()
                        .map_err(|_| "an item is a string like \"pkg:weapon/flag\", or ()")?,
                )
            },
        })
    });
    let fire = |brick: Dynamic, input: &str, player: Dynamic| {
        push(Op::FireBrickInput {
            brick: id(&brick)?,
            input: input.to_owned(),
            player: if player.is_unit() {
                None
            } else {
                Some(id(&player)?)
            },
        })
    };
    engine.register_fn("fire_brick_input", fire);
    engine.register_fn("fire_brick_input", move |brick: Dynamic, input: &str| {
        fire(brick, input, Dynamic::UNIT)
    });
    let optional = |p: Dynamic| -> Fallible<Option<u64>> {
        if p.is_unit() {
            Ok(None)
        } else {
            Ok(Some(id(&p)?))
        }
    };
    let fire_game = move |game: Dynamic, input: &str, player: Dynamic, killer: Dynamic| {
        push(Op::FireGameInput {
            game: id(&game)?,
            input: input.to_owned(),
            player: optional(player)?,
            killer: optional(killer)?,
        })
    };
    engine.register_fn("fire_game_input", fire_game);
    engine.register_fn(
        "fire_game_input",
        move |game: Dynamic, input: &str, player: Dynamic| {
            fire_game(game, input, player, Dynamic::UNIT)
        },
    );
    engine.register_fn("fire_game_input", move |game: Dynamic, input: &str| {
        fire_game(game, input, Dynamic::UNIT, Dynamic::UNIT)
    });
    engine.register_fn("drop_item", drop_with);
    engine.register_fn("remove_drop", |drop: Dynamic| {
        push(Op::RemoveDrop { drop: id(&drop)? })
    });
    // Text floating over one of this package's dropped items in a palette
    // colour (`setShapeName`), or () to take it away.
    engine.register_fn("name_drop", |drop: Dynamic, text: Dynamic, color: Dynamic| {
        push(Op::NameDrop {
            drop: id(&drop)?,
            text: if text.is_unit() { None } else { Some(text.to_string()) },
            color: palette_index(&color)?,
        })
    });
    engine.register_fn("name_drop", |drop: Dynamic, text: Dynamic| {
        if !text.is_unit() {
            return fail("name_drop(drop, text, colour); name_drop(drop, ()) takes the name away");
        }
        push(Op::NameDrop { drop: id(&drop)?, text: None, color: 0 })
    });
    // A score report in its own window (`show_report(p, #{ title, banner,
    // columns: [#{ key, title }], sections: [#{ title, rows: [#{ key,
    // name, color, cells: #{ column: value } }] }] })`), closed with
    // `hide_report(p)`.
    engine.register_fn("show_report", |player: Dynamic, report: Map| {
        push(Op::ShowReport {
            player: id(&player)?,
            report: Some(Box::new(report_from(report)?)),
        })
    });
    engine.register_fn("hide_report", |player: Dynamic| {
        push(Op::ShowReport {
            player: id(&player)?,
            report: None,
        })
    });
    // A game's report column by key: retitled and filled by row key
    // (`report_column(g, "kills", "Flag Pick-ups", #{ "player:3": 2 })`),
    // or taken out with `report_column(g, key, ())`.
    engine.register_fn(
        "report_column",
        |game: Dynamic, key: &str, title: Dynamic, cells: Map| {
            push(Op::ReportColumn {
                game: id(&game)?,
                change: ColumnChange {
                    key: key.into(),
                    title: Some(title.to_string()),
                    cells: cells
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), cell(&v)))
                        .collect(),
                },
            })
        },
    );
    engine.register_fn("report_column", |game: Dynamic, key: &str, title: Dynamic| {
        if !title.is_unit() {
            return fail("report_column(game, key, title, cells); report_column(game, key, ()) takes the column out");
        }
        push(Op::ReportColumn {
            game: id(&game)?,
            change: ColumnChange {
                key: key.into(),
                title: None,
                cells: BTreeMap::new(),
            },
        })
    });
    engine.register_fn("drops", || {
        with_world(|world, _| Ok(world.drops().iter().map(drop_map).collect::<Array>()))
    });
    engine.register_fn(
        "mount_image",
        |p: Dynamic, image: Dynamic, slot: Dynamic| wear(p, image, slot, None, false),
    );
    // The fourth argument is a paint, or `#{ paint, keep }`: `keep: true`
    // lets no other Add-On replace or take off the image while it is worn.
    engine.register_fn(
        "mount_image",
        |p: Dynamic, image: Dynamic, slot: Dynamic, paint: Dynamic| {
            if let Some(options) = paint.clone().try_cast::<Map>() {
                for key in options.keys() {
                    if !matches!(key.as_str(), "paint" | "keep") {
                        return fail(format!("mount_image takes `paint` and `keep`, not `{key}`"));
                    }
                }
                let paint = match options.get("paint") {
                    None => None,
                    Some(p) if p.is_unit() => None,
                    Some(p) => Some(palette_index(p)?),
                };
                let keep = match options.get("keep") {
                    None => false,
                    Some(k) => k.as_bool().map_err(|_| "`keep` is true or false")?,
                };
                return wear(p, image, slot, paint, keep);
            }
            let paint = if paint.is_unit() {
                None
            } else {
                Some(palette_index(&paint)?)
            };
            wear(p, image, slot, paint, false)
        },
    );
    // Every brick of one kind, lowest id first, at most MAX_BRICKS_LISTED.
    engine.register_fn("bricks", |kind: &str| {
        with_world(|world, _| {
            Ok(world
                .bricks_of(kind, MAX_BRICKS_LISTED)
                .iter()
                .map(brick_map)
                .collect::<Array>())
        })
    });
    engine.register_fn("brick", |brick: Dynamic| {
        let brick = id(&brick)?;
        with_world(|world, _| Ok(world.brick(brick).as_ref().map_or(Dynamic::UNIT, brick_map)))
    });
    // A value kept on a brick (a v20 brick's dynamic field): this
    // package's own `key`, or another's as `namespace:key`; () when none.
    engine.register_fn("brick_field", |brick: Dynamic, key: &str| {
        let brick = id(&brick)?;
        with_world(|world, _| {
            Ok(world
                .brick_field(brick, key)
                .map_or(Dynamic::UNIT, |v| to_dynamic(&v)))
        })
    });
    engine.register_fn("set_brick_field", |brick: Dynamic, key: &str, value: Dynamic| {
        push(Op::SetBrickField {
            brick: id(&brick)?,
            key: key.into(),
            value: if value.is_unit() {
                None
            } else {
                Some(to_json(&value)?)
            },
        })
    });
    engine.register_fn("set_brick_color", |brick: Dynamic, color: Dynamic| {
        push(Op::SetBrickColor {
            brick: id(&brick)?,
            color: palette_index(&color)?,
        })
    });
    engine.register_fn("set_zone_period", |zone: i64, period_ms: i64| {
        push(Op::SetZonePeriod {
            zone: u32::try_from(zone).map_err(|_| "a zone is its index in behaviour.json's zones")?,
            period_ms: u32::try_from(period_ms).map_err(|_| "a zone's period is 10 to 10000 ms")?,
        })
    });
    // The avatar pack's choices by slot, and its faces and decals, in the
    // pack's order (`$pref::Avatar::Hat` 6 is the seventh hat).
    engine.register_fn("avatar_choices", || {
        with_world(|world, _| {
            let mut out = Map::new();
            for (slot, names) in world.avatar_choices() {
                out.insert(
                    slot.into(),
                    Dynamic::from_array(names.into_iter().map(Dynamic::from).collect()),
                );
            }
            Ok(out)
        })
    });
    // The paint palette: `[r, g, b, a]` from 0 to 1 for each colour index
    // (`getColorIDTable`).
    engine.register_fn("palette", || {
        with_world(|world, _| {
            Ok(world
                .palette()
                .iter()
                .map(|c| Dynamic::from_array(c.iter().map(|v| Dynamic::from_float(f64::from(*v))).collect()))
                .collect::<Array>())
        })
    });
}
