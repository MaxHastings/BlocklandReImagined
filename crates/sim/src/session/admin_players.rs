//! Administrator player and vehicle commands (`serverCmdDropPlayerAtCamera`,
//! `Fetch`, `Find`, `Warp`, `ResetVehicles`, `ClearVehicles`, `TimeScale`).
use super::*;
use bri_minigames as mg;
use rapier3d::prelude::*;

/// `serverCmdWarp` scans 1000 units along the eye vector.
const WARP_RANGE: f32 = 1000.0;
/// After an admin teleport, minigame weapons stay quiet for 3 s
/// (`WeaponImage::onFire`, `ProjectileData::onCollision`/`onExplode`).
pub(super) const TELEPORT_WEAPON_LOCK_MS: u64 = 3000;
/// and, with weapon damage on, pickups and activation wait 5 s
/// (`ItemData::onPickup`, `Weapon::onPickup`, `Player::ActivateStuff`).
pub(super) const TELEPORT_PICKUP_LOCK_MS: u64 = 5000;

impl Session {
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }
    fn admin_name(&self, owner: OwnerId) -> String {
        self.peers
            .get(&owner)
            .map_or_else(String::new, |p| p.name.clone())
    }
    /// `/fetch` and `/find`: a rider's root mount moves instead and plays
    /// the effect; a body on foot moves and stops. Returns whether it rode.
    fn admin_teleport(&mut self, owner: OwnerId, feet: Vec3, yaw: f32) -> Result<bool> {
        let peer = self.peers.get(&owner).context("Unknown player")?;
        ensure!(peer.combat.alive, "That player is dead");
        if let Some((at, scale)) =
            self.teleport_mount(owner, feet, glam::Quat::from_rotation_y(-yaw))?
        {
            self.teleport_burst(owner, at, scale);
            return Ok(true);
        }
        let peer = self.peers.get_mut(&owner).context("Unknown player")?;
        peer.player
            .teleport(&mut self.simulation.physics, feet, yaw)?;
        peer.inputs.clear();
        self.simulation.stream_terrain();
        Ok(false)
    }
    fn body(&self, owner: OwnerId) -> Result<(Vec3, f32)> {
        let peer = self.peers.get(&owner).context("Unknown player")?;
        ensure!(peer.combat.alive, "That player has no body");
        let state = peer.player.state();
        Ok((Vec3::from(state.feet), state.yaw))
    }
    /// `getSimTime() - %client.lastF8Time < ms` inside a minigame; with
    /// `weapon_damage`, only when that minigame has `weaponDamage` on.
    pub(super) fn teleport_lockout(&self, owner: OwnerId, ms: u64, weapon_damage: bool) -> bool {
        self.damage_policy()
            .teleport_lockout(owner, ms, weapon_damage)
    }
    /// `%client.lastF8Time = getSimTime()`.
    fn note_admin_teleport(&mut self, owner: OwnerId) {
        let tick = self.simulation.state().tick;
        if let Some(peer) = self.peers.get_mut(&owner) {
            peer.last_drop_tick = Some(tick);
        }
    }
    /// `Vehicle::teleportEffect`: a playerTeleportProjectile, which
    /// explodes at once, at the vehicle.
    fn teleport_burst(&mut self, owner: OwnerId, at: Vec3, scale: f32) {
        self.teleport_cue(owner, at, scale, false);
    }
    fn teleport_cue(&mut self, owner: OwnerId, at: Vec3, scale: f32, player: bool) {
        self.emote_cue(
            self.simulation.state().tick,
            crate::presentation::CueKind::Teleport {
                actor: owner,
                scale: scale.clamp(0.01, 100.0),
                player,
            },
            at.to_array(),
        );
    }
    /// `Player::teleportEffect`: the burst at the hack position, scaled with
    /// the player, and the PlayerTeleportImage sparkle on the back for 3 s.
    pub(super) fn player_teleport_effect(&mut self, owner: OwnerId) {
        let Some(peer) = self.peers.get(&owner) else {
            return;
        };
        let state = peer.player.state();
        let center = Vec3::from(state.feet) + Vec3::Y * peer.player.tuning().stand_height * 0.5;
        let scale = state.scale;
        self.teleport_cue(owner, center, scale, true);
    }
    /// Nearest terrain, interior or brick along a ray.
    fn world_ray(&self, start: Vec3, dir: Vec3, range: f32) -> Option<f32> {
        // Players (1) and vehicles (2) are not world geometry.
        let predicate = |_: ColliderHandle, c: &Collider| !matches!(c.user_data >> 64, 1 | 2);
        let ray = Ray::new(
            Vector::from_array(start.to_array()),
            Vector::from_array(dir.to_array()),
        );
        let bricks = self
            .simulation
            .physics
            .query_pipeline_with_filter(
                QueryFilter::default()
                    .exclude_sensors()
                    .predicate(&predicate),
            )
            .cast_ray(&ray, range, true)
            .map(|(_, distance)| distance);
        let terrain = self
            .simulation
            .terrain_ray(start, dir, range)
            .map(|(distance, _)| distance);
        bricks.into_iter().chain(terrain).min_by(f32::total_cmp)
    }
    /// `serverCmdDropPlayerAtCamera` (F7). `view` is the client's camera at
    /// the key press; otherwise the camera is wherever it was last left, so
    /// F7 without F8 goes back there. A dead administrator respawns.
    pub(super) fn drop_player_at_camera(
        &mut self,
        owner: OwnerId,
        view: Option<CameraView>,
    ) -> Result<()> {
        if let Some(view) = view {
            view.validate()?;
        }
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        ensure!(peer.actor.administrator, "Only administrators can do that");
        if view.is_some() {
            peer.camera = view;
        }
        if !peer.combat.alive {
            let effects = self
                .minigames
                .execute(mg::Command::ForceRespawn {
                    target: peer.combat.player,
                })
                .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
            return self.apply_minigame_effects(effects);
        }
        // Inside minigames it costs a point.
        let player = peer.combat.player;
        if self
            .minigames
            .player(player)
            .is_ok_and(|p| p.game.is_some())
            && let Ok(effects) = self.minigames.event_score(player, -1, true)
        {
            self.apply_minigame_effects(effects)?;
        }
        self.note_admin_teleport(owner);
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let state = peer.player.state();
        let eye = peer.player.eye();
        let offset = eye - Vec3::from(state.feet);
        let camera = peer.camera.unwrap_or(CameraView {
            eye: eye.to_array(),
            yaw: state.yaw,
            pitch: state.pitch,
        });
        // A rider's vehicle takes the camera's whole transform, pitch too.
        if let Some((at, scale)) = self.teleport_mount(owner, camera.eye(), camera.rotation())? {
            self.teleport_burst(owner, at, scale);
        } else {
            // The eye lands on the camera, unless the ground is closer below
            // it than the eye's height, which the feet then stand on.
            let eye = camera.eye();
            let feet = offset
                .try_normalize()
                .and_then(|up| {
                    self.world_ray(eye, -up, offset.length())
                        .map(|distance| eye - up * distance)
                })
                .unwrap_or(eye - offset);
            let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
            peer.player
                .teleport(&mut self.simulation.physics, feet, camera.yaw)?;
            peer.inputs.clear();
            self.simulation.stream_terrain();
            self.player_teleport_effect(owner);
        }
        self.peers
            .get_mut(&owner)
            .context("Unknown connection")?
            .control = ControlObject::Player;
        Ok(())
    }
    /// `/fetch`: bring the victim to the administrator. The effect plays on
    /// the administrator, or on the victim's vehicle when it rides one.
    pub(super) fn admin_fetch(&mut self, admin: OwnerId, victim: OwnerId) -> Result<()> {
        let (feet, yaw) = self.body(admin)?;
        self.body(victim)?;
        self.note_admin_teleport(admin);
        self.player_teleport_effect(admin);
        self.admin_teleport(victim, feet, yaw)?;
        Ok(())
    }
    /// `/find`: go to the victim.
    pub(super) fn admin_find(&mut self, admin: OwnerId, victim: OwnerId) -> Result<()> {
        let (feet, yaw) = self.body(victim)?;
        self.body(admin)?;
        self.note_admin_teleport(admin);
        if !self.admin_teleport(admin, feet, yaw)? {
            self.player_teleport_effect(admin);
        }
        Ok(())
    }
    /// `/warp`: jump to the terrain, interior or brick under the crosshair.
    pub(super) fn admin_warp(&mut self, admin: OwnerId) -> Result<()> {
        let peer = self.peers.get(&admin).context("Unknown player")?;
        ensure!(peer.combat.alive, "You are dead");
        let state = peer.player.state();
        let start = peer.player.eye();
        let dir = state.forward();
        let yaw = state.yaw;
        let Some(distance) = self.world_ray(start, dir, WARP_RANGE) else {
            return Ok(());
        };
        if self.seated(admin) {
            self.eject(admin);
        }
        let peer = self.peers.get_mut(&admin).context("Unknown player")?;
        peer.player
            .teleport(&mut self.simulation.physics, start + dir * distance, yaw)?;
        peer.inputs.clear();
        self.simulation.stream_terrain();
        self.player_teleport_effect(admin);
        Ok(())
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
