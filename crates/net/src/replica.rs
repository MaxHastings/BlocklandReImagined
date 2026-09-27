use crate::protocol::*;
use anyhow::{Result, ensure};
use bri_world::OwnerId;
use std::collections::{BTreeMap, VecDeque};
pub struct Replica {
    pub weapons: bri_sim::session::WeaponView,
    pub tools: BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    pub cue_cursor: u64,
    pub dropped_cues: u64,
    cues: Vec<bri_sim::presentation::Cue>,
    pub cursor: u64,
    pub tick: u64,
    pub world: PublicWorld,
    pub names: BTreeMap<OwnerId, String>,
    pub avatars: BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    pub chat: VecDeque<bri_sim::session::ChatLine>,
    pub poses: BTreeMap<OwnerId, Pose>,
    history: BTreeMap<OwnerId, VecDeque<Pose>>,
    pub vitals: BTreeMap<OwnerId, bri_sim::session::Vitals>,
    pub minigames: Vec<bri_sim::session::MiniGameView>,
    pub vehicles: BTreeMap<u64, bri_sim::session::VehicleInfo>,
    pub vehicle_poses: BTreeMap<u64, bri_sim::session::VehiclePose>,
    pub time_scale: f32,
}
fn validate_time_scale(scale: f32) -> Result<()> {
    ensure!((0.2..=2.0).contains(&scale), "Invalid time scale");
    Ok(())
}
fn validate_vehicles(vehicles: &[bri_sim::session::VehicleInfo]) -> Result<()> {
    ensure!(
        vehicles.len() <= 1024
            && vehicles.iter().all(|v| v.id > 0
                && !v.definition.is_empty()
                && v.definition.len() <= 128
                && v.occupants.len() <= 16),
        "Invalid vehicle listing"
    );
    Ok(())
}
fn validate_vehicle_pose(pose: &bri_sim::session::VehiclePose) -> Result<()> {
    ensure!(
        pose.id > 0
            && pose
                .position
                .iter()
                .chain(&pose.rotation)
                .chain(&pose.velocity)
                .chain(&pose.turret_aim)
                .chain(&pose.wheel_suspension)
                .chain(&pose.wheel_rotation)
                .all(|v| v.is_finite())
            && pose.steering.is_finite()
            && pose.wheel_suspension.len() <= 16
            && pose.wheel_rotation.len() <= 16,
        "Invalid vehicle pose"
    );
    Ok(())
}
fn validate_vitals(
    vitals: &BTreeMap<OwnerId, bri_sim::session::Vitals>,
    names: &BTreeMap<OwnerId, String>,
) -> Result<()> {
    ensure!(
        vitals.len() <= 64
            && vitals.keys().all(|id| names.contains_key(id))
            && vitals.values().all(|v| v.health.is_finite()
                && (0.0..=bri_sim::player_types::PlayerType::highest_max_health())
                    .contains(&v.health)),
        "Invalid player vitals"
    );
    Ok(())
}
fn validate_minigames(games: &[bri_sim::session::MiniGameView]) -> Result<()> {
    ensure!(
        games.len() <= 64
            && games.iter().all(|g| g.members.len() <= 64
                && g.color < 10
                && g.settings.title.len() <= 256
                && !g.settings.title.chars().any(char::is_control)),
        "Invalid minigame listing"
    );
    Ok(())
}
impl Replica {
    pub fn new(checkpoint: Checkpoint) -> Result<Self> {
        ensure!(
            checkpoint.world.bricks.len() <= bri_world::MAX_BRICKS
                && !checkpoint.world.palette.is_empty()
                && checkpoint.world.palette.len() <= 256
                && checkpoint.names.len() <= 64
                && checkpoint.chat.len() <= 100,
            "Invalid checkpoint bounds"
        );
        for (id, b) in &checkpoint.world.bricks {
            ensure!(*id > 0, "Invalid brick identity");
            b.validate(checkpoint.world.palette.len())?;
        }
        validate_avatars(&checkpoint.avatars, &checkpoint.names)?;
        validate_tools(&checkpoint.tools, &checkpoint.names)?;
        validate_vitals(&checkpoint.vitals, &checkpoint.names)?;
        validate_minigames(&checkpoint.minigames)?;
        validate_vehicles(&checkpoint.vehicles)?;
        validate_time_scale(checkpoint.time_scale)?;
        for pose in &checkpoint.vehicle_poses {
            validate_vehicle_pose(pose)?;
        }
        checkpoint.weapons.validate(&checkpoint.names)?;
        let mut out = Self {
            weapons: checkpoint.weapons,
            tools: checkpoint.tools,
            cue_cursor: checkpoint.cue_cursor,
            dropped_cues: checkpoint.dropped_cues,
            cues: Vec::new(),
            cursor: checkpoint.cursor,
            tick: checkpoint.tick,
            world: checkpoint.world,
            names: checkpoint.names,
            avatars: checkpoint.avatars,
            chat: checkpoint.chat.into(),
            poses: BTreeMap::new(),
            history: BTreeMap::new(),
            vitals: checkpoint.vitals,
            minigames: checkpoint.minigames,
            vehicles: checkpoint.vehicles.into_iter().map(|v| (v.id, v)).collect(),
            vehicle_poses: checkpoint
                .vehicle_poses
                .into_iter()
                .map(|p| (p.id, p))
                .collect(),
            time_scale: checkpoint.time_scale,
        };
        for pose in checkpoint.poses {
            out.pose(pose)?;
        }
        Ok(out)
    }
    pub fn update(&mut self, delta: Delta) -> Result<()> {
        ensure!(
            delta.base == self.cursor
                && delta.cursor == self.cursor.checked_add(1).unwrap_or(0)
                && delta.tick >= self.tick,
            "Replication gap or reversed tick"
        );
        ensure!(
            delta.bricks.len() <= bri_world::MAX_BRICKS
                && delta.cues.len() <= bri_sim::presentation::MAX_CUES
                && delta.dropped_cues >= self.dropped_cues
                && delta.names.as_ref().is_none_or(|n| n.len() <= 64)
                && delta.chat.len() <= 100,
            "Invalid delta bounds"
        );
        let mut previous = 0;
        let mut expected = self.cue_cursor;
        let mut missing = 0u64;
        for cue in &delta.cues {
            cue.validate()?;
            ensure!(
                cue.id > previous && cue.tick <= delta.tick,
                "Reordered/future presentation cues"
            );
            previous = cue.id;
            if cue.id > expected {
                missing = missing.saturating_add(cue.id - expected - 1);
                expected = cue.id;
            }
        }
        ensure!(
            missing <= delta.dropped_cues - self.dropped_cues,
            "Unreported presentation gap"
        );
        let new_cues = delta.cues.iter().filter(|c| c.id > self.cue_cursor).count();
        ensure!(
            self.cues.len() + new_cues <= bri_sim::presentation::MAX_CUES,
            "Client presentation backlog exceeded"
        );
        for (id, b) in &delta.bricks {
            ensure!(*id > 0, "Invalid brick identity");
            if let Some(b) = b {
                b.validate(delta.palette.as_ref().unwrap_or(&self.world.palette).len())?;
            }
        }
        if let Some(avatars) = &delta.avatars {
            validate_avatars(avatars, delta.names.as_ref().unwrap_or(&self.names))?;
        }
        validate_tools(
            delta.tools.as_ref().unwrap_or(&self.tools),
            delta.names.as_ref().unwrap_or(&self.names),
        )?;
        delta
            .weapons
            .as_ref()
            .unwrap_or(&self.weapons)
            .validate(delta.names.as_ref().unwrap_or(&self.names))?;
        if let Some(vitals) = &delta.vitals {
            validate_vitals(vitals, delta.names.as_ref().unwrap_or(&self.names))?;
        }
        if let Some(games) = &delta.minigames {
            validate_minigames(games)?;
        }
        if let Some(vehicles) = &delta.vehicles {
            validate_vehicles(vehicles)?;
        }
        if let Some(scale) = delta.time_scale {
            validate_time_scale(scale)?;
        }
        if let Some(palette) = &delta.palette {
            ensure!(
                palette.len() <= 256
                    && palette.starts_with(&self.world.palette)
                    && palette
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "Invalid append-only palette update"
            );
        }
        let removed = delta
            .bricks
            .iter()
            .filter(|(id, b)| b.is_none() && self.world.bricks.contains_key(id))
            .count();
        let added = delta
            .bricks
            .iter()
            .filter(|(id, b)| b.is_some() && !self.world.bricks.contains_key(id))
            .count();
        ensure!(
            self.world.bricks.len() - removed + added <= bri_world::MAX_BRICKS,
            "Replica brick budget exceeded"
        );
        if let Some(palette) = delta.palette {
            self.world.palette = palette;
        }
        for (id, b) in delta.bricks {
            if let Some(b) = b {
                self.world.bricks.insert(id, b);
            } else {
                self.world.bricks.remove(&id);
            }
        }
        if let Some(names) = delta.names {
            self.names = names;
            self.poses.retain(|id, _| self.names.contains_key(id));
            self.history.retain(|id, _| self.names.contains_key(id));
        }
        if let Some(avatars) = delta.avatars {
            self.avatars = avatars;
        }
        if let Some(tools) = delta.tools {
            self.tools = tools;
        }
        if let Some(weapons) = delta.weapons {
            self.weapons = weapons;
        }
        if let Some(vitals) = delta.vitals {
            self.vitals = vitals;
        }
        if let Some(games) = delta.minigames {
            self.minigames = games;
        }
        if let Some(scale) = delta.time_scale {
            self.time_scale = scale;
        }
        if let Some(vehicles) = delta.vehicles {
            self.vehicles = vehicles.into_iter().map(|v| (v.id, v)).collect();
            self.vehicle_poses
                .retain(|id, _| self.vehicles.contains_key(id));
        }
        self.vitals.retain(|id, _| self.names.contains_key(id));
        self.avatars.retain(|id, _| self.names.contains_key(id));
        for line in delta.chat {
            if self.chat.back().is_none_or(|p| p.id < line.id) {
                self.chat.push_back(line);
            }
        }
        while self.chat.len() > 100 {
            self.chat.pop_front();
        }
        self.cursor = delta.cursor;
        for cue in delta.cues {
            if cue.id > self.cue_cursor {
                self.cue_cursor = cue.id;
                self.cues.push(cue);
            }
        }
        self.dropped_cues = delta.dropped_cues;
        self.tick = delta.tick;
        Ok(())
    }
    /// Newest-tick vehicle motion; older or unknown datagrams are ignored.
    pub fn vehicle_pose(&mut self, pose: bri_sim::session::VehiclePose) -> Result<()> {
        validate_vehicle_pose(&pose)?;
        if self
            .vehicle_poses
            .get(&pose.id)
            .is_none_or(|old| old.tick < pose.tick)
        {
            self.vehicle_poses.insert(pose.id, pose);
        }
        Ok(())
    }
    pub fn take_cues(&mut self) -> Vec<bri_sim::presentation::Cue> {
        std::mem::take(&mut self.cues)
    }
    pub fn pose(&mut self, pose: Pose) -> Result<()> {
        let p = &pose.player;
        ensure!(
            p.owner > 0
                && p.feet
                    .iter()
                    .chain(p.velocity.iter())
                    .all(|n| n.is_finite())
                && p.yaw.is_finite()
                && p.pitch.is_finite(),
            "Invalid player pose"
        );
        if !self.names.contains_key(&p.owner)
            || self
                .poses
                .get(&p.owner)
                .is_some_and(|old| old.tick >= pose.tick)
        {
            return Ok(());
        }
        let history = self.history.entry(p.owner).or_default();
        history.push_back(pose.clone());
        while history.len() > 32 {
            history.pop_front();
        }
        self.poses.insert(p.owner, pose);
        Ok(())
    }
    /// Render remote players behind the latest server tick; never extrapolate
    /// unbounded motion during a network stall. Local prediction is separate.
    pub fn interpolated(&self, owner: OwnerId, tick: f64) -> Option<bri_sim::player::PlayerState> {
        let history = self.history.get(&owner)?;
        let last = history.back()?;
        let first = history.front()?;
        if tick <= first.tick as f64 {
            return Some(first.player.clone());
        }
        for (a, b) in history.iter().zip(history.iter().skip(1)) {
            if tick <= b.tick as f64 {
                let blend =
                    ((tick - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;
                let mut out = b.player.clone();
                for i in 0..3 {
                    out.feet[i] = a.player.feet[i] + (b.player.feet[i] - a.player.feet[i]) * blend;
                }
                let yaw = (b.player.yaw - a.player.yaw + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                out.yaw = a.player.yaw + yaw * blend;
                out.pitch = a.player.pitch + (b.player.pitch - a.player.pitch) * blend;
                return Some(out);
            }
        }
        Some(last.player.clone())
    }
}

fn validate_avatars(
    avatars: &BTreeMap<OwnerId, bri_content::avatar::Appearance>,
    names: &BTreeMap<OwnerId, String>,
) -> Result<()> {
    ensure!(
        avatars.len() <= 64 && avatars.keys().all(|id| names.contains_key(id)),
        "Invalid replicated avatar owners"
    );
    for appearance in avatars.values() {
        appearance.validate_bounds()?;
    }
    Ok(())
}

fn validate_tools(
    tools: &BTreeMap<OwnerId, bri_sim::session::ToolInventory>,
    names: &BTreeMap<OwnerId, String>,
) -> Result<()> {
    ensure!(
        tools.len() <= 64 && tools.keys().eq(names.keys()),
        "Invalid replicated inventory owners"
    );
    for inventory in tools.values() {
        inventory.validate()?;
    }
    Ok(())
}
