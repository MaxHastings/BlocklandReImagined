//! Add-Ons' wrench event inputs and outputs (`behaviour.json`
//! `brick_inputs` and `brick_outputs`, v20's `registerInputEvent` and
//! `registerOutputEvent`): Slayer_CTF's `onFlagPickedUp`, Slayer's
//! `setTeamControl` and the like. Builders wire them in the wrench like
//! the engine's own. The Add-On's rules fire its inputs with
//! `fire_brick_input`, and the rows run as the brick owner's, under the
//! same budgets and trust; a row that runs one of its outputs calls its
//! `on_brick_output`.
use super::super::events::{InputExtra, entity, id};
use super::*;
use bri_events as ev;
use bri_package_runtime::content::BRICK_INPUT_TARGETS;

/// An Add-On input that follows one of the engine's (`follows`): Slayer's
/// `onPlayerTouch(Team1)` after `onPlayerTouch`.
#[derive(Clone, Debug)]
pub(in crate::session) struct Follower {
    /// The engine's input, as the host catalog names it.
    native: String,
    package: String,
    input: String,
}

impl Session {
    /// What the running Add-Ons add to the event catalog. Sent to players
    /// so their wrench offers it too.
    pub fn package_brick_events(&self) -> ev::Extension {
        ev::Extension {
            inputs: self.package_brick_inputs(),
            targets: self.package_brick_targets(),
            outputs: self.package_brick_outputs(),
        }
    }

    /// The running Add-Ons' inputs, as the event catalog lists them.
    pub fn package_brick_inputs(&self) -> Vec<ev::InputDef> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (package, behaviour) in host.catalog.behaviours() {
            for input in &behaviour.brick_inputs {
                let mut targets = vec![("Self".to_owned(), "fxDTSBrick".to_owned())];
                targets.extend(
                    BRICK_INPUT_TARGETS
                        .iter()
                        .filter(|(slot, _)| input.targets.iter().any(|t| t == slot))
                        .map(|(slot, class)| ((*slot).to_owned(), (*class).to_owned())),
                );
                out.push(ev::InputDef {
                    id: format!("{package}:{}", input.name),
                    class_name: "fxDTSBrick".into(),
                    name: input.name.clone(),
                    targets,
                    source: package.clone(),
                    source_line: 0,
                });
            }
        }
        out
    }

    /// `fire_brick_input`: run the rows on `brick` wired to `input`, one of
    /// `package`'s own inputs. Without a wrench event catalog (a host that
    /// runs no events) nothing happens.
    pub(in crate::session) fn package_fire_brick_input(
        &mut self,
        package: &str,
        brick: BrickId,
        input: &str,
        player: Option<OwnerId>,
    ) -> Result<()> {
        let host = self.packages.as_ref().context("No packages are enabled")?;
        let declared = host
            .catalog
            .behaviours()
            .find(|(id, _)| *id == package)
            .and_then(|(_, b)| {
                b.brick_inputs
                    .iter()
                    .find(|i| i.name.eq_ignore_ascii_case(input))
            })
            .map(|i| i.name.clone())
            .with_context(|| format!("`{input}` is not one of `{package}`'s brick_inputs"))?;
        ensure!(
            self.simulation.state().bricks.contains_key(&brick),
            "No such brick"
        );
        if let Some(p) = player {
            ensure!(self.peers.contains_key(&p), "No such player");
        }
        self.fire_package_input(brick, &declared, player, Default::default());
        Ok(())
    }

    /// The running Add-Ons' targets, as the event catalog lists them.
    pub fn package_brick_targets(&self) -> Vec<ev::TargetDef> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (package, behaviour) in host.catalog.behaviours() {
            for target in &behaviour.brick_targets {
                out.push(ev::TargetDef {
                    id: format!("{package}:{}", target.name),
                    name: target.name.clone(),
                    class_name: target.class.clone(),
                    from: target.from.clone(),
                    package: package.clone(),
                    source: package.clone(),
                    source_line: 0,
                });
            }
        }
        out
    }

    /// `fire_game_input`: run the rows wired to `input`, one of `package`'s
    /// own inputs, on every brick of mini-game `game` (the bricks whose
    /// owner's group plays in it), in brick order.
    pub(in crate::session) fn package_fire_game_input(
        &mut self,
        package: &str,
        game: u64,
        input: &str,
        player: Option<OwnerId>,
        killer: Option<OwnerId>,
    ) -> Result<()> {
        let host = self.packages.as_ref().context("No packages are enabled")?;
        let declared = host
            .catalog
            .behaviours()
            .find(|(id, _)| *id == package)
            .and_then(|(_, b)| {
                b.brick_inputs
                    .iter()
                    .find(|i| i.name.eq_ignore_ascii_case(input))
            })
            .map(|i| i.name.clone())
            .with_context(|| format!("`{input}` is not one of `{package}`'s brick_inputs"))?;
        let game = bri_minigames::GameId(game);
        self.minigames.game(game).context("No such mini-game")?;
        for p in player.iter().chain(&killer) {
            ensure!(self.peers.contains_key(p), "No such player");
        }
        // Programs edited this tick may listen to it now.
        self.sync_event_programs(&BTreeSet::new());
        let Some(world) = self.events.world.as_ref() else {
            return Ok(());
        };
        let mut bricks: BTreeSet<BrickId> = world
            .listeners(&declared)
            .into_iter()
            .map(|id| id.index)
            .collect();
        bricks.extend(self.dirty.iter());
        let state = self.simulation.state();
        let bricks: Vec<BrickId> = bricks
            .into_iter()
            .filter(|b| {
                state.bricks.get(b).is_some_and(|b| {
                    self.game_of(self.brick_group_owner_for(b.owner, Some(game))) == Some(game)
                })
            })
            .collect();
        let extra = InputExtra {
            game: Some(game),
            killer,
        };
        for brick in bricks {
            self.fire_package_input(brick, &declared, player, extra);
        }
        Ok(())
    }

    /// The rows of events `owner` sent for `brick` that every Add-On with
    /// `on_event_row` keeps; the others are taken out, and their reasons
    /// returned to tell the player. A wrench send runs this before it
    /// applies the rows (v20's `serverCmdAddEvent`).
    pub fn review_event_rows(
        &mut self,
        owner: OwnerId,
        brick: BrickId,
        rows: &mut Vec<ev::Row>,
    ) -> Vec<String> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let hooks: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| b.on_event_row)
            .map(|(id, _)| id.clone())
            .collect();
        if hooks.is_empty() || rows.is_empty() {
            return Vec::new();
        }
        let Some(catalog) = self.event_catalog().cloned() else {
            return Vec::new();
        };
        let mut refused = Vec::new();
        let mut keep = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let Some(view) = row_view(&catalog, index, row) else {
                keep.push(true);
                continue;
            };
            let mut kept = true;
            for package in &hooks {
                let answer = self.run_package(
                    package,
                    "on_event_row",
                    vec![
                        Dynamic::from_int(owner as i64),
                        Dynamic::from_int(brick as i64),
                        Dynamic::from_map(view.clone()),
                    ],
                    Budget::Command,
                    Some(owner),
                    None,
                    None,
                );
                self.charge_work(package);
                match answer {
                    Ok(a) if a.as_bool() == Ok(false) => {
                        refused.push(format!("You may not use the {} event.", row.output));
                        kept = false;
                    }
                    Ok(a) if a.is_string() => {
                        // A chat line's worth, as a player's own.
                        let reason = a.into_string().unwrap_or_default();
                        refused.push(reason.chars().take(REASON_CHARS).collect());
                        kept = false;
                    }
                    _ => {}
                }
                if !kept {
                    break;
                }
            }
            keep.push(kept);
        }
        let mut keep = keep.into_iter();
        rows.retain(|_| keep.next().unwrap_or(true));
        refused
    }

    /// The running Add-Ons' inputs that follow one of `base`'s inputs.
    pub(in crate::session) fn package_followers(&self, base: &ev::Catalog) -> Vec<Follower> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (package, behaviour) in host.catalog.behaviours() {
            for input in &behaviour.brick_inputs {
                let Some(follows) = &input.follows else {
                    continue;
                };
                // A host whose catalog has no such input never sets it
                // off, so nothing follows it.
                let Some(native) = base.input(follows) else {
                    continue;
                };
                out.push(Follower {
                    native: native.name.clone(),
                    package: package.clone(),
                    input: input.name.clone(),
                });
            }
        }
        out
    }

    /// `owner` set `input` off on `brick`: each Add-On with inputs that
    /// follow it, wired on this brick, is asked with
    /// `on_brick_input(input, brick, player)` and may answer with one of
    /// them to run as well, set off by the same player (Slayer's
    /// `onPlayerTouch(Team2)` for a Team 2 member).
    pub(in crate::session) fn follow_input(&mut self, brick: BrickId, input: &str, owner: OwnerId) {
        if !self
            .events
            .follows
            .iter()
            .any(|f| f.native.eq_ignore_ascii_case(input))
        {
            return;
        }
        let Some(world) = self.events.world.as_ref() else {
            return;
        };
        let mut asked: Vec<&str> = Vec::new();
        for f in &self.events.follows {
            if f.native.eq_ignore_ascii_case(input)
                && !asked.contains(&f.package.as_str())
                && world
                    .activation_count(id(brick), &f.input)
                    .is_ok_and(|n| n > 0)
            {
                asked.push(&f.package);
            }
        }
        let asked: Vec<String> = asked.into_iter().map(str::to_owned).collect();
        for package in asked {
            let answer = self.run_package(
                &package,
                "on_brick_input",
                vec![
                    input.to_owned().into(),
                    Dynamic::from_int(brick as i64),
                    Dynamic::from_int(owner as i64),
                ],
                Budget::Think,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(answer) = answer else {
                continue;
            };
            if answer.is_unit() {
                continue;
            }
            let name = answer.into_string().unwrap_or_default();
            let follower = self.events.follows.iter().find(|f| {
                f.package == package
                    && f.native.eq_ignore_ascii_case(input)
                    && f.input.eq_ignore_ascii_case(&name)
            });
            match follower.map(|f| f.input.clone()) {
                Some(follower) => {
                    self.fire_package_input(brick, &follower, Some(owner), Default::default())
                }
                None => {
                    let d = Diagnostic::error(
                        "brick_events.follows",
                        format!(
                            "on_brick_input answered `{name}`, not one of its inputs that \
                             follow `{input}`"
                        ),
                    )
                    .at(package.clone());
                    if let Some(host) = self.packages.as_mut() {
                        note(host, d);
                    }
                }
            }
        }
    }

    /// The running Add-Ons' outputs, as the event catalog lists them.
    pub fn package_brick_outputs(&self) -> Vec<ev::OutputDef> {
        let Some(host) = self.packages.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (package, behaviour) in host.catalog.behaviours() {
            for output in &behaviour.brick_outputs {
                // The two schemas are written alike, so a parameter reads
                // as the catalog's own.
                let params = output
                    .params
                    .iter()
                    .filter_map(|p| serde_json::to_value(p).ok())
                    .filter_map(|p| serde_json::from_value(p).ok())
                    .collect();
                out.push(ev::OutputDef {
                    id: format!("{package}:{}:{}", output.class, output.name),
                    class_name: output.class.clone(),
                    name: output.name.clone(),
                    params,
                    append_client: false,
                    source: package.clone(),
                    source_line: 0,
                    package: Some(package.clone()),
                });
            }
        }
        out
    }

    /// A row ran `call`, one of an Add-On's outputs: its
    /// `on_brick_output(output, target, params, info)` carries it out.
    /// `target` is the brick, player or minigame the row aims at (players
    /// by their player number); `info` is `#{ brick, owner, client, class,
    /// target, base, input, row }`, `client` being whoever set the row off,
    /// or `()`. A row aimed at one of the Add-On's `brick_targets` gets the
    /// target's base entity, `info.target` its name, `info.class` its class
    /// and `info.base` the base entity's class; otherwise `info.target` is
    /// `()` and `info.class` and `info.base` are the target's class. The
    /// rules may answer with one of their own inputs, `"onTeamCheckTrue"`
    /// or `#{ input, rows: [first, last] }`, and the brick's rows that
    /// listen to it run next (only rows `first..=last` with `rows`).
    pub(in crate::session) fn package_output(
        &mut self,
        dispatch: &ev::Dispatch,
        call: &ev::PackageCall,
    ) -> ev::Apply {
        let Some(declared) = self.packages.as_ref().and_then(|host| {
            host.catalog
                .behaviours()
                .find(|(id, _)| **id == call.package)
                .map(|(_, b)| b.clone())
        }) else {
            return ev::Apply::Rejected(format!("Add-On `{}` is not running", call.package));
        };
        let class = match dispatch.target.class {
            ev::Class::Brick => "fxDTSBrick",
            ev::Class::Player => "Player",
            ev::Class::Client => "GameConnection",
            ev::Class::MiniGame => "MiniGame",
            _ => return ev::Apply::Rejected("not a target Add-On outputs act on".into()),
        };
        // An Add-On target's rows act on its base entity; the rules find
        // what the target stands for.
        let derived = match &dispatch.derived {
            Some(name) => match declared
                .brick_targets
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name))
            {
                Some(target) => Some(target),
                None => {
                    return ev::Apply::Rejected(format!(
                        "`{name}` is not one of `{}`'s brick_targets",
                        call.package
                    ));
                }
            },
            None => None,
        };
        let source = dispatch.source.index;
        let owner = self.simulation.state().bricks.get(&source).map(|b| b.owner);
        let Some(owner) = owner else {
            return ev::Apply::Rejected("the brick is gone".into());
        };
        let client = dispatch
            .client
            .map(|c| c.id.index)
            .filter(|c| self.peers.contains_key(c));
        let mut info = bri_package_runtime::rhai::Map::new();
        info.insert("brick".into(), Dynamic::from_int(source as i64));
        info.insert("owner".into(), Dynamic::from_int(owner as i64));
        info.insert(
            "client".into(),
            client.map_or(Dynamic::UNIT, |c| Dynamic::from_int(c as i64)),
        );
        match derived {
            Some(target) => {
                info.insert("class".into(), target.class.clone().into());
                info.insert("target".into(), target.name.clone().into());
                info.insert("base".into(), class.into());
            }
            None => {
                info.insert("class".into(), class.into());
                info.insert("target".into(), Dynamic::UNIT);
                info.insert("base".into(), class.into());
            }
        }
        info.insert("input".into(), dispatch.input.clone().into());
        info.insert("row".into(), Dynamic::from_int(i64::from(dispatch.row)));
        let params: bri_package_runtime::rhai::Array = call
            .params
            .iter()
            .map(|v| match v {
                ev::Value::Int(n) => Dynamic::from_int(*n),
                ev::Value::Float(f) => Dynamic::from_float(f64::from(*f)),
                ev::Value::Bool(b) => Dynamic::from_bool(*b),
                ev::Value::Text(t) => t.clone().into(),
                ev::Value::Color(c) => Dynamic::from_int(i64::from(*c)),
                ev::Value::Vector(v) => Dynamic::from_array(
                    v.to_array()
                        .map(|c| Dynamic::from_float(f64::from(c)))
                        .to_vec(),
                ),
                ev::Value::Datablock(d) => d.clone().map_or(Dynamic::UNIT, Into::into),
                ev::Value::Rows(_) => Dynamic::UNIT,
            })
            .collect();
        let answer = self.run_package(
            &call.package,
            "on_brick_output",
            vec![
                call.output.clone().into(),
                Dynamic::from_int(dispatch.target.id.index as i64),
                Dynamic::from_array(params),
                Dynamic::from_map(info),
            ],
            Budget::Think,
            client,
            None,
            None,
        );
        self.charge_work(&call.package);
        let answer = match answer {
            Ok(answer) => answer,
            Err(diagnostic) => return ev::Apply::Rejected(diagnostic.message),
        };
        if answer.is_unit() {
            return ev::Apply::Applied;
        }
        // The answer names one of the package's own inputs to run next.
        let (input, rows) = if let Some(map) = answer.read_lock::<bri_package_runtime::rhai::Map>()
        {
            let input = map.get("input").and_then(|i| i.clone().into_string().ok());
            let rows = map.get("rows").and_then(|r| {
                let r = r.read_lock::<bri_package_runtime::rhai::Array>()?;
                let n = |i: usize| r.get(i)?.as_int().ok().and_then(|n| u16::try_from(n).ok());
                (r.len() == 2).then(|| Some((n(0)?, n(1)?))).flatten()
            });
            (input, rows)
        } else {
            (answer.clone().into_string().ok(), None)
        };
        let Some(input) = input.and_then(|name| {
            declared
                .brick_inputs
                .iter()
                .find(|i| i.name.eq_ignore_ascii_case(&name))
        }) else {
            return ev::Apply::Rejected(format!(
                "`{}` answered with something not one of its brick_inputs",
                call.output
            ));
        };
        let mut slots = std::collections::BTreeSet::new();
        for slot in &input.targets {
            slots.extend(ev::Slot::parse(slot));
        }
        let mut trigger = ev::Trigger::new(id(source), input.name.clone(), dispatch.origin);
        trigger.rows = rows.map(|(a, b)| (a.min(b), a.max(b)));
        if let Some(client) = client {
            self.player_targets(source, &slots, client, &mut trigger);
        } else if slots.contains(&ev::Slot::MiniGame)
            && let Some(game) = self.game_of(owner)
        {
            trigger
                .targets
                .insert(ev::Slot::MiniGame, entity(ev::Class::MiniGame, game.0));
        }
        self.owner_targets(source, &slots, &mut trigger.targets);
        ev::Apply::Chain(trigger)
    }
}

/// The most of an `on_event_row` reason a player is told.
const REASON_CHARS: usize = 200;

/// A row as `on_event_row` reads it: `#{ index, input, target, class,
/// output, package }`, `class` being the target's class as the input lists
/// it and `package` the Add-On whose output it is, or `()`. Preserved rows
/// have none.
fn row_view(
    catalog: &ev::Catalog,
    index: usize,
    row: &ev::Row,
) -> Option<bri_package_runtime::rhai::Map> {
    if row.preserved.is_some() {
        return None;
    }
    let input = catalog.input(&row.input)?;
    let (target, class) = match &row.target {
        ev::Target::Named(name) => (name.clone(), "fxDTSBrick".to_owned()),
        ev::Target::Slot(slot) => input
            .targets
            .iter()
            .find(|(s, _)| ev::Slot::parse(s) == Some(*slot))?
            .clone(),
        ev::Target::Derived(name) => input
            .targets
            .iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(name))?
            .clone(),
    };
    let (_, output) = catalog
        .row_output(&row.input, &row.target, &row.output)
        .ok()?;
    let mut view = bri_package_runtime::rhai::Map::new();
    view.insert("index".into(), Dynamic::from_int(index as i64));
    view.insert("input".into(), input.name.clone().into());
    view.insert("target".into(), target.into());
    view.insert("class".into(), class.into());
    view.insert("output".into(), output.name.clone().into());
    view.insert(
        "package".into(),
        output.package.clone().map_or(Dynamic::UNIT, Into::into),
    );
    Some(view)
}
