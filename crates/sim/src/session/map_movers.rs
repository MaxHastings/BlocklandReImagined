//! Map movers: static models that move along authored keys (doors), and
//! the collision that moves with them. The engine owns the mechanism, a
//! package the policy: Add-Ons start a mover on a key with `mover_goto`
//! and hold it with `mover_stop` (the `movers` capability); the engine
//! eases it there, stops it when a body is in its way and tells every
//! client its state so all see it at the same pose on the same tick.
use super::*;
use crate::map::MapMover;
use bri_content::mover::{Encroach, MoverState};

/// What `movers()` tells a script about a mover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoverView {
    pub id: String,
    /// The key it rests at, or None between keys.
    pub key: Option<u8>,
    /// The key it left, and the key it is heading to (while moving) or last
    /// reached.
    pub from: u8,
    pub to: u8,
    pub moving: bool,
    /// Held by a body in its way.
    pub blocked: bool,
    /// How far along its motion, 0 to 1 (before easing).
    pub progress: f32,
    /// The key count.
    pub keys: usize,
    /// Where its origin is now.
    pub position: [f32; 3],
    /// Where it is placed at rest.
    pub rest: [f32; 3],
    /// The package's own facts (`MoverDef::params`).
    pub params: BTreeMap<String, String>,
}

/// How near a bot's body must be to a shut door it may open (units).
const BOT_DOOR_REACH: f32 = 0.6;
/// Ticks before a bot asks again for a door that is still shut.
const BOT_DOOR_RETRY: u64 = 120 * 2;

/// The rules command a mover's `bots_open` names (`package:command`), if it
/// does. `"1"` alone only makes the door passable to a bot's route search.
fn bot_door_command(value: &str) -> Option<(&str, &str)> {
    value
        .split_once(':')
        .filter(|(package, command)| !package.is_empty() && !command.is_empty())
}

#[derive(Default)]
pub(super) struct MapMovers {
    /// When each bot last asked for each door, by mover index.
    asked: BTreeMap<(OwnerId, usize), u64>,
    /// Refusals of a bot's door command already printed, by door and
    /// reason: the engine refusing it (a command only a tool may run, a
    /// cool-down) is told once, not every retry.
    refused: BTreeSet<(usize, String)>,
    movers: Vec<MapMover>,
    states: Vec<MoverState>,
    /// The transform each mover's colliders were last put at.
    placed: Vec<Option<glam::Mat4>>,
    /// Frozen state of the last collision pose actually accepted by the host.
    accepted: Vec<MoverState>,
    /// World items an Add-On placed on a mover (`place_item`), by drop: the
    /// mover's index and the item's pose in the mover's frame.
    pins: BTreeMap<u64, (usize, glam::Mat4)>,
}

fn pose(transform: glam::Mat4) -> rapier3d::prelude::Pose {
    let (_, rotation, translation) = transform.to_scale_rotation_translation();
    rapier3d::prelude::Pose::from_parts(
        rapier3d::prelude::Vector::from_array(translation.to_array()),
        rapier3d::prelude::Rotation::from_array(rotation.to_array()),
    )
}

/// How far from a mover's collision a ray's landing may be to count as on
/// it, in units: the ray ends on the surface, so only rounding is allowed.
const MOVER_HIT_GAP: f32 = 0.02;

impl Session {
    /// Install the map's movers (from `NativeMap::movers`), all at rest at
    /// key 0.
    pub fn set_map_movers(&mut self, movers: Vec<MapMover>) -> Result<()> {
        ensure!(
            movers.len() <= bri_content::mover::MAX_MOVERS,
            "Too many map movers"
        );
        for mover in &movers {
            mover.def.validate()?;
        }
        // A mover whose package says `bots_open` is a door a bot's route
        // search goes through: the package's rules open it for the bot
        // that comes to it.
        for mover in &movers {
            self.simulation.set_bot_passable(
                mover.colliders.clone(),
                mover
                    .def
                    .params
                    .get("bots_open")
                    .is_some_and(|v| v == "1" || bot_door_command(v).is_some()),
            );
        }
        let initial: Vec<MoverState> = movers
            .iter()
            .map(|m| MoverState {
                hidden: m.def.hidden,
                ..MoverState::default()
            })
            .collect();
        for (mover, state) in movers.iter().zip(&initial) {
            if state.hidden {
                self.simulation
                    .set_map_colliders(mover.colliders.clone(), false)?;
            }
        }
        let count = movers.len();
        self.map_movers = MapMovers {
            asked: BTreeMap::new(),
            refused: BTreeSet::new(),
            movers,
            states: initial.clone(),
            placed: vec![None; count],
            accepted: initial,
            pins: BTreeMap::new(),
        };
        Ok(())
    }
    /// A bot at a shut door whose `bots_open` names a rules command sends it,
    /// as the player's Use key would, once and again every few seconds while
    /// the door stays shut. Whether it opens is the rules' own policy.
    pub(super) fn bot_opens_door(&mut self, bot: OwnerId, tick: u64) {
        if self.map_movers.movers.iter().all(|m| {
            m.def
                .params
                .get("bots_open")
                .and_then(|v| bot_door_command(v))
                .is_none()
        }) {
            return;
        }
        // Few bots at a time are within a door's reach: look every few ticks.
        if tick % 8 != bot % 8 {
            return;
        }
        let mut asks = Vec::new();
        for id in self.movers_near_player(bot, BOT_DOOR_REACH) {
            let Some(index) = self.mover_index(&id) else {
                continue;
            };
            let mover = &self.map_movers.movers[index];
            let Some((package, command)) = mover
                .def
                .params
                .get("bots_open")
                .and_then(|v| bot_door_command(v))
            else {
                continue;
            };
            if self.map_movers.states[index].resting_key(tick) != Some(0) {
                continue;
            }
            if self
                .map_movers
                .asked
                .get(&(bot, index))
                .is_some_and(|at| tick < at + BOT_DOOR_RETRY)
            {
                continue;
            }
            asks.push((index, package.to_string(), command.to_string()));
        }
        for (index, package, command) in asks {
            self.map_movers.asked.insert((bot, index), tick);
            let sequence = self.peers[&bot].last_sequence.saturating_add(1);
            // The rules may refuse (a door they keep locked): that is theirs.
            // The engine refusing the command is not, and says so once.
            let full = format!("{package}:{command}");
            let sent = self.command(
                bot,
                sequence,
                Command::Package(PackageCommand {
                    package,
                    command,
                    args: Vec::new(),
                }),
            );
            if let Err(error) = sent
                && self
                    .map_movers
                    .refused
                    .insert((index, format!("{error:#}")))
            {
                bri_console::warn(format!(
                    "bot {bot} could not send {full} at door {}: {error:#}",
                    self.map_movers.movers[index].def.id
                ));
            }
        }
    }
    /// Every mover's state, in map order, for replication.
    pub fn map_mover_states(&self) -> Vec<MoverState> {
        self.map_movers.states.clone()
    }
    fn mover_index(&self, id: &str) -> Option<usize> {
        self.map_movers.movers.iter().position(|m| m.def.id == id)
    }
    /// What a script sees of the map's movers now.
    pub fn mover_views(&self) -> Vec<MoverView> {
        let tick = self.simulation.state().tick;
        self.map_movers
            .movers
            .iter()
            .zip(&self.map_movers.states)
            .map(|(mover, state)| {
                let at = mover.transform(state, tick).w_axis.truncate();
                MoverView {
                    id: mover.def.id.clone(),
                    key: state.resting_key(tick),
                    from: state.from,
                    to: state.to,
                    moving: state.is_moving_at(tick),
                    blocked: state.blocked,
                    progress: state.alpha_at(tick),
                    keys: mover.def.keys.len(),
                    position: at.to_array(),
                    rest: mover.placement.w_axis.truncate().to_array(),
                    params: mover.def.params.clone(),
                }
            })
            .collect()
    }
    /// Movers whose collision lies within `reach` of the body of player
    /// `owner` (touching it, for a reach of a hair), nearest first.
    pub fn movers_near_player(&self, owner: OwnerId, reach: f32) -> Vec<String> {
        self.mover_gaps(owner, reach)
            .into_iter()
            .map(|(_, id)| id)
            .collect()
    }
    /// The shown movers whose collision lies within `radius` of `point`, with
    /// how far and the nearest point of it, nearest first (what a blast at
    /// `point` reaches).
    pub fn movers_within(&self, point: Vec3, radius: f32) -> Vec<(String, f32, Vec3)> {
        let mut found: Vec<(String, f32, Vec3)> = Vec::new();
        for (index, mover) in self.map_movers.movers.iter().enumerate() {
            if self.map_movers.states[index].hidden {
                continue;
            }
            let nearest = mover
                .colliders
                .clone()
                .filter_map(|c| {
                    let closest = self.simulation.map_collider_closest(c, point)?;
                    Some((closest.distance(point), closest))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((gap, closest)) = nearest
                && gap <= radius
            {
                found.push((mover.def.id.clone(), gap, closest));
            }
        }
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        found
    }
    /// The same movers with their gaps (a negative one: the body is in it),
    /// nearest first.
    pub(super) fn mover_gaps(&self, owner: OwnerId, reach: f32) -> Vec<(f32, String)> {
        let Some(peer) = self.peers.get(&owner) else {
            return Vec::new();
        };
        let tuning = peer.player.tuning();
        let state = peer.player.state();
        let height = if state.crouched {
            tuning.crouch_height
        } else {
            tuning.stand_height
        };
        let feet = Vec3::from(state.feet);
        let body = rapier3d::prelude::Pose::translation(feet.x, feet.y + height * 0.5, feet.z);
        let shape = rapier3d::prelude::SharedShape::cuboid(
            tuning.width * 0.5,
            height * 0.5,
            tuning.width * 0.5,
        );
        let mut found: Vec<(f32, String)> = Vec::new();
        for mover in &self.map_movers.movers {
            let mut nearest = f32::INFINITY;
            for collider in mover.colliders.clone() {
                if let Some(distance) =
                    self.simulation
                        .map_collider_distance(collider, &body, shape.as_ref(), reach)
                {
                    nearest = nearest.min(distance);
                }
            }
            if nearest <= reach {
                found.push((nearest, mover.def.id.clone()));
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        found
    }
    /// The package's own facts of mover `id`.
    pub(super) fn map_mover_params(&self, id: &str) -> Option<&BTreeMap<String, String>> {
        let index = self.mover_index(id)?;
        Some(&self.map_movers.movers[index].def.params)
    }
    /// Start mover `id` toward key `key` over `seconds`. False when it
    /// cannot go there now (see [`MoverState::go_to`]).
    /// The id of the mover whose collision `point` lies on (within a hair),
    /// the nearest when several are: what a ray that hit the map struck.
    pub fn mover_at(&self, point: Vec3) -> Option<&str> {
        self.map_movers
            .movers
            .iter()
            .filter_map(|mover| {
                let gap = mover
                    .colliders
                    .clone()
                    .filter_map(|c| self.simulation.map_collider_gap(c, point))
                    .min_by(f32::total_cmp)?;
                (gap <= MOVER_HIT_GAP).then_some((gap, mover.def.id.as_str()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id)
    }
    pub fn mover_goto(&mut self, id: &str, key: u8, seconds: f32) -> Result<bool> {
        let index = self
            .mover_index(id)
            .with_context(|| format!("No map mover called {id}"))?;
        ensure!(
            usize::from(key) < self.map_movers.movers[index].def.keys.len(),
            "Mover {id} has no key {key}"
        );
        ensure!(
            seconds.is_finite() && (0.0..=600.0).contains(&seconds),
            "A mover's travel takes 0 to 600 seconds"
        );
        let tick = self.simulation.state().tick;
        Ok(self.map_movers.states[index].go_to(tick, key, seconds))
    }
    /// Hold world item `drop` at `position` turned to `rotation`; with
    /// `mover` it rides that mover, keeping its place on it.
    pub(super) fn pin_drop(
        &mut self,
        drop: u64,
        position: Vec3,
        rotation: glam::Quat,
        mover: Option<&str>,
    ) -> Result<()> {
        let index = match mover {
            Some(id) => Some(
                self.mover_index(id)
                    .with_context(|| format!("No map mover called {id}"))?,
            ),
            None => None,
        };
        self.weapons.pin_drop(drop, rotation)?;
        self.weapons.set_drop_pose(drop, position, rotation);
        if let Some(index) = index {
            let tick = self.simulation.state().tick;
            let world =
                self.map_movers.movers[index].transform(&self.map_movers.states[index], tick);
            let item = glam::Mat4::from_rotation_translation(rotation, position);
            self.map_movers
                .pins
                .insert(drop, (index, world.inverse() * item));
        }
        Ok(())
    }
    /// Carry the items placed on movers along with them.
    fn carry_pins(&mut self, tick: u64) {
        let pins = &mut self.map_movers.pins;
        pins.retain(|drop, _| self.weapons.drops().any(|d| d.id == *drop));
        for (drop, (index, local)) in &self.map_movers.pins {
            let mover = &self.map_movers.movers[*index];
            let world = mover.transform(&self.map_movers.states[*index], tick) * *local;
            let (_, rotation, position) = world.to_scale_rotation_translation();
            self.weapons.set_drop_pose(*drop, position, rotation);
        }
    }
    /// Show or hide mover `id`: hidden, it is not drawn and not solid
    /// (to bodies, shots and bots alike), though its motion goes on.
    pub fn mover_show(&mut self, id: &str, shown: bool) -> Result<()> {
        let index = self
            .mover_index(id)
            .with_context(|| format!("No map mover called {id}"))?;
        self.map_movers.states[index].hidden = !shown;
        self.map_movers.accepted[index].hidden = !shown;
        self.simulation
            .set_map_colliders(self.map_movers.movers[index].colliders.clone(), shown)
    }
    /// Hold mover `id` where it is.
    pub fn mover_stop(&mut self, id: &str) -> Result<()> {
        let index = self
            .mover_index(id)
            .with_context(|| format!("No map mover called {id}"))?;
        let tick = self.simulation.state().tick;
        let state = &mut self.map_movers.states[index];
        if state.is_moving_at(tick) {
            state.halt(tick, false);
        }
        Ok(())
    }
    /// One tick: settle motions that have arrived, move the colliders of
    /// those moving, and hold any a body stands in the way of.
    pub(super) fn step_map_movers(&mut self) -> Result<()> {
        if self.map_movers.movers.is_empty() {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        for index in 0..self.map_movers.movers.len() {
            let state = self.map_movers.states[index];
            let mover = &self.map_movers.movers[index];
            let target = mover.transform(&state, tick);
            if self.map_movers.placed[index] != Some(target) && !mover.colliders.is_empty() {
                let blocked = state.moving
                    && mover.def.encroach != Encroach::Ignore
                    && self.map_movers.placed[index].is_some()
                    && mover
                        .colliders
                        .clone()
                        .any(|c| self.simulation.map_collider_meets_player(c, &pose(target)));
                if blocked {
                    // The attempted pose intersects a player. Publishing it as the
                    // stopped state would apply it without the moving check next
                    // tick, and clients would immediately predict that bad pose.
                    let held = MoverState {
                        blocked: true,
                        ..self.map_movers.accepted[index]
                    };
                    self.map_movers.states[index] = held;
                    if mover.def.encroach == Encroach::Return && held.rate > 0.0 {
                        let seconds = 1.0 / (held.rate * bri_world::TICKS_PER_SECOND as f32);
                        self.map_movers.states[index].go_to(tick, held.from, seconds);
                    }
                    continue;
                }
                self.simulation
                    .move_map_colliders(mover.colliders.clone(), pose(target));
                self.map_movers.placed[index] = Some(target);
            } else if self.map_movers.placed[index].is_none() {
                self.map_movers.placed[index] = Some(target);
            }
            self.map_movers.states[index].settle(tick);
            let mut accepted = self.map_movers.states[index];
            accepted.halt(tick, false);
            self.map_movers.accepted[index] = accepted;
        }
        self.carry_pins(tick);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{definitions::Definitions, simulation::Simulation};
    use bri_content::mover::{Glide, MoverDef, MoverKey};
    use rapier3d::prelude::*;

    /// A slab lying along +X, hinged at the origin, that swings 90 degrees.
    fn door() -> MapMover {
        MapMover {
            node: 5,
            def: MoverDef {
                id: "door".into(),
                keys: vec![
                    MoverKey::default(),
                    MoverKey {
                        translation: [0.0; 3],
                        rotation: [0.0, -std::f32::consts::FRAC_PI_2, 0.0],
                    },
                ],
                glide: Glide::Linear,
                encroach: Encroach::Ignore,
                spin: None,
                collide: true,
                params: Default::default(),
                sounds: None,
                hidden: false,
            },
            placement: glam::Mat4::IDENTITY,
            colliders: 1..2,
        }
    }

    #[test]
    fn an_item_placed_on_a_door_rides_it_open_and_stays_off_the_pickup_list() {
        let mut s = Session::new(
            Simulation::new(
                bri_world::World::new("Doors".into(), "test".into(), vec![[1.0; 4]; 2]),
                Definitions::default(),
                vec![
                    ColliderBuilder::cuboid(200.0, 0.5, 200.0)
                        .translation(Vector::new(0.0, -0.5, 0.0)),
                    ColliderBuilder::compound(vec![(
                        Pose::translation(1.5, 3.0, 0.0),
                        SharedShape::cuboid(1.5, 3.0, 0.1),
                    )])
                    .position(Pose::IDENTITY),
                ],
            )
            .unwrap(),
        );
        s.set_weapon_pack(
            bri_weapons::Pack::from_json(
                &serde_json::to_vec(&serde_json::json!({
                    "schema_version":5,"id":"charge",
                    "items":{"charge:item/charge":{"ui_name":"Charge","image":"charge:image/charge"}},
                    "images":{"charge:image/charge":{"states":[{"name":"Ready"}]}},
                    "projectiles":{}
                }))
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        s.set_item_bounds(Default::default()).unwrap();
        s.set_map_movers(vec![door()]).unwrap();
        let at = Vec3::new(2.5, 1.0, 0.1);
        let drop = s
            .weapons
            .spawn_drop("charge:item/charge", at, Vec3::ZERO)
            .unwrap();
        s.pin_drop(drop, at, glam::Quat::IDENTITY, Some("door"))
            .unwrap();
        for _ in 0..30 {
            s.step().unwrap();
        }
        let held = s.weapons.drops().find(|d| d.id == drop).unwrap().position;
        assert!(held.distance(at) < 0.01, "pinned, not fallen: {held:?}");
        assert!(!s.weapons.pickup_ready(bri_weapons::ActorId(1), drop));
        // Open the door a quarter turn in a second: the item goes with it.
        assert!(s.mover_goto("door", 1, 1.0).unwrap());
        for _ in 0..130 {
            s.step().unwrap();
        }
        let carried = s.weapons.drops().find(|d| d.id == drop).unwrap().position;
        assert!(
            carried.distance(Vec3::new(-0.1, 1.0, 2.5)) < 0.1,
            "swung with the door: {carried:?}"
        );
    }
}
