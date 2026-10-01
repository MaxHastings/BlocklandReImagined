//! Add-Ons' wrench event inputs and outputs (`behaviour.json`
//! `brick_inputs` and `brick_outputs`, v20's `registerInputEvent` and
//! `registerOutputEvent`): Slayer_CTF's `onFlagPickedUp`, Slayer's
//! `setTeamControl` and the like. Builders wire them in the wrench like
//! the engine's own. The Add-On's rules fire its inputs with
//! `fire_brick_input`, and the rows run as the brick owner's, under the
//! same budgets and trust; a row that runs one of its outputs calls its
//! `on_brick_output`.
use super::super::events::{entity, id};
use super::*;
use bri_events as ev;
use bri_package_runtime::content::BRICK_INPUT_TARGETS;

impl Session {
    /// The running Add-Ons' inputs, as the event catalog lists them. Sent
    /// to players so their wrench offers them too.
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
        self.fire_package_input(brick, &declared, player);
        Ok(())
    }

    /// The running Add-Ons' outputs, as the event catalog lists them. Sent
    /// to players so their wrench offers them too.
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
    /// input, row }`, `client` being whoever set the row off, or `()`. The
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
        let source = dispatch.source.index;
        let owner = self
            .simulation
            .state()
            .bricks
            .get(&source)
            .map(|b| b.owner);
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
        info.insert("class".into(), class.into());
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
