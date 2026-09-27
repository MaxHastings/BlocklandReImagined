//! Administrator player and vehicle commands (`serverCmdFetch`, `Find`,
//! `Warp`, `ResetVehicles`, `ClearVehicles`, `TimeScale`).
use super::*;
use rapier3d::prelude::*;

/// `serverCmdWarp` scans 1000 units along the eye vector.
const WARP_RANGE: f32 = 1000.0;

impl Session {
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }
    fn admin_name(&self, owner: OwnerId) -> String {
        self.peers.get(&owner).map_or_else(String::new, |p| p.name.clone())
    }
    /// Move a living, unmounted body; riders are put on foot first.
    fn admin_teleport(&mut self, owner: OwnerId, feet: Vec3, yaw: f32) -> Result<()> {
        if self.vehicles.is_mounted(owner) {
            self.eject(owner);
        }
        let peer = self.peers.get_mut(&owner).context("Unknown player")?;
        ensure!(peer.combat.alive, "That player is dead");
        peer.player.teleport(&mut self.simulation.physics, feet, yaw)?;
        peer.inputs.clear();
        self.simulation.stream_terrain();
        Ok(())
    }
    fn body(&self, owner: OwnerId) -> Result<(Vec3, f32)> {
        let peer = self.peers.get(&owner).context("Unknown player")?;
        ensure!(peer.combat.alive, "That player has no body");
        let state = peer.player.state();
        Ok((Vec3::from(state.feet), state.yaw))
    }
    /// `/fetch`: bring the victim to the administrator.
    pub(super) fn admin_fetch(&mut self, admin: OwnerId, victim: OwnerId) -> Result<()> {
        let (feet, yaw) = self.body(admin)?;
        self.admin_teleport(victim, feet, yaw)
    }
    /// `/find`: go to the victim.
    pub(super) fn admin_find(&mut self, admin: OwnerId, victim: OwnerId) -> Result<()> {
        let (feet, yaw) = self.body(victim)?;
        self.admin_teleport(admin, feet, yaw)
    }
    /// `/warp`: jump to the terrain, interior or brick under the crosshair.
    pub(super) fn admin_warp(&mut self, admin: OwnerId) -> Result<()> {
        let peer = self.peers.get(&admin).context("Unknown player")?;
        ensure!(peer.combat.alive, "You are dead");
        let state = peer.player.state();
        let start = peer.player.eye();
        let dir = state.forward();
        let yaw = state.yaw;
        // Players (1) and vehicles (2) are not warp targets.
        let predicate = |_: ColliderHandle, c: &Collider| !matches!(c.user_data >> 64, 1 | 2);
        let ray = Ray::new(
            Vector::from_array(start.to_array()),
            Vector::from_array(dir.to_array()),
        );
        let bricks = self
            .simulation
            .physics
            .query_pipeline_with_filter(QueryFilter::default().exclude_sensors().predicate(&predicate))
            .cast_ray(&ray, WARP_RANGE, true)
            .map(|(_, distance)| distance);
        let terrain = self
            .simulation
            .terrain_ray(start, dir, WARP_RANGE)
            .map(|(distance, _)| distance);
        let Some(distance) = bricks.into_iter().chain(terrain).min_by(f32::total_cmp) else {
            return Ok(());
        };
        self.admin_teleport(admin, start + dir * distance, yaw)
    }
    /// `/resetVehicles`: every spawn brick spawns a fresh vehicle or bot.
    pub(super) fn admin_reset_vehicles(&mut self, admin: OwnerId) -> Result<()> {
        let name = self.admin_name(admin);
        self.system_chat(format!("\u{E003}{name}\u{E000} reset all vehicles."));
        for brick in self.vehicle_spawn_bricks() {
            self.clear_brick_vehicle(brick)?;
            self.respawn_vehicle_brick(brick)?;
        }
        for brick in self.bot_bricks() {
            self.respawn_vehicle_brick(brick)?;
        }
        Ok(())
    }
    /// `/clearVehicles`.
    pub(super) fn admin_clear_vehicles(&mut self, admin: OwnerId) -> Result<()> {
        let bricks = self.vehicle_spawn_bricks();
        for brick in &bricks {
            self.clear_brick_vehicle(*brick)?;
        }
        let name = self.admin_name(admin);
        self.system_chat(format!(
            "\u{E003}{name}\u{E000} cleared all vehicles ({}).",
            bricks.len()
        ));
        Ok(())
    }
    /// `/timeScale`: the whole game runs faster or slower.
    pub(super) fn admin_time_scale(&mut self, admin: OwnerId, scale: f32) -> Result<()> {
        ensure!(scale.is_finite(), "Invalid time scale");
        let scale = scale.clamp(0.2, 2.0);
        self.time_scale = scale;
        let name = self.admin_name(admin);
        self.system_chat(format!("{name} changed the timescale to {scale}"));
        Ok(())
    }
}
