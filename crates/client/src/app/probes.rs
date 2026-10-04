//! Read-only views of the game for tests, tools and the console.
use super::*;

impl App {
    pub fn work_counters(&self) -> crate::perf::WorkCounters {
        crate::perf::WorkCounters {
            chunk_jobs: self.scene.chunk_jobs,
            chunks_rebuilt: self.scene.chunks_rebuilt,
            uploads: self
                .gpu
                .renderer
                .as_ref()
                .and_then(|r| r.finished())
                .map(|r| r.upload_counts()),
            item_model_builds: self.world_items.diagnostics.model_builds,
            item_model_uploads: self.world_items.diagnostics.model_uploads,
            debris_looks_built: self.fx.debris_models.diagnostics.looks_built,
            music_full_scans: self.audio.music_bricks.full_scans,
            music_visited: self.audio.music_bricks.visited,
            hidden_outlines: self.gpu.hidden_outlines.diagnostics,
        }
    }
    pub fn item_assets(&self) -> &Arc<crate::items::ItemAssets> {
        &self.item_assets
    }
    /// Names of the Add-Ons whose client code runs in the game entered.
    pub fn add_on_code_running(&self) -> Vec<&str> {
        self.addons.client_code.running()
    }
    /// Bodies of knocked-out bricks still flying or fading.
    pub fn brick_debris_count(&self) -> usize {
        self.fx.brick_debris.len()
    }
    /// Gun casings currently tumbling or resting.
    pub fn weapon_shell_count(&self) -> usize {
        self.fx.weapon_shells.active_count()
    }
    pub fn world_item_stats(&self) -> &crate::world_items::WorldItemDiagnostics {
        &self.world_items.diagnostics
    }
    /// The drawn world items: identity, transform and instance tint.
    pub fn world_item_instances(
        &self,
    ) -> impl Iterator<
        Item = (
            crate::world_items::ItemIdentity,
            &bri_render::scene::SceneTransform,
        ),
    > {
        self.world_items.instances()
    }
    pub fn foliage_stats(&self) -> &bri_foliage::RenderStats {
        &self.foliage.stats
    }
    pub fn foliage_placement(&self) -> (&[bri_foliage::PlacementStats], f64) {
        (
            &self.foliage.prepared.placement,
            self.foliage.prepared.elapsed_ms,
        )
    }
    pub fn weather_counts(&self) -> (usize, usize) {
        (
            self.weather.world.drop_count(),
            self.weather.world.splash_count(),
        )
    }
    pub fn weather_diagnostics(&self) -> bri_weather::WeatherDiagnostics {
        self.weather.world.diagnostics()
    }
    /// Draws and binds the last rendered frame recorded.
    pub fn render_stats(&self) -> Option<bri_render::scene::RenderStats> {
        self.gpu
            .renderer
            .as_ref()
            .and_then(|r| r.finished())
            .map(|r| r.stats())
    }
    /// Time each world pass on the GPU every frame (as the expanded
    /// performance overlay does), for benchmarks.
    pub fn time_gpu_passes(&mut self, on: bool) {
        self.gpu.time_passes = on;
    }
    /// GPU ms per world pass in the latest timed frame, in frame order.
    pub fn gpu_pass_times(&self) -> &[(&'static str, f32)] {
        &self.gpu.gpu_passes
    }
    pub fn frame_stats(&self) -> &crate::console::FrameStats {
        &self.perf.frame_stats
    }
    /// First run: choose Low, Medium or High from the GPU and screen, and
    /// save it as the player's graphics options. Their own later choices win.
    pub(super) fn pick_quality(&mut self, adapter: &wgpu::AdapterInfo) {
        if !crate::quality::first_run(&self.ui.settings().prefs) {
            return;
        }
        let screen = self.ui.core.display_modes.as_ref().map(|m| m.native);
        let quality = crate::quality::pick(adapter.device_type, screen);
        bri_console::echo(format!(
            "First run: {} graphics quality for {} ({:?}) on a {} screen. Options > Graphics changes it.",
            quality.name(),
            adapter.name,
            adapter.device_type,
            screen.map_or("unknown".into(), |(w, h)| format!("{w}x{h}")),
        ));
        self.ui.apply(UiUpdate::SetPrefs(quality.prefs()));
        // The renderer about to be built uses it; saving follows in `pump`.
        self.graphics = crate::graphics::Graphics::from_settings(&self.ui.settings());
    }
    /// The game a player started (not a test or tool): check for a newer
    /// release, pick a graphics quality on the first run, and log frame
    /// times to the session log.
    pub fn player_session(&mut self) {
        let settings = self.ui.settings();
        self.lobby.update_check = crate::updates::start(&settings);
        self.perf.auto_quality = true;
        self.perf.frame_log = Some(Default::default());
    }
    pub fn audio_stats(&self) -> bri_audio::AudioStats {
        self.audio.stats()
    }
    pub fn audio_requests(&self) -> &BTreeMap<String, u64> {
        &self.audio.requested
    }
    pub fn audio_warnings(&self) -> &std::collections::BTreeSet<String> {
        &self.audio.warnings
    }
    pub fn effect_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.fx.effects.attachment_count(),
            self.fx.effects.world.source_count(),
            self.fx.effects.world.particle_count(),
            self.fx.effects.deferred,
        )
    }
    /// Live weapon effect sources and particles (trails, muzzle and image
    /// state emitters, explosions).
    pub fn weapon_effect_counts(&self) -> (usize, usize) {
        (
            self.fx.weapon_effects.world().source_count(),
            self.fx.weapon_effects.world().particle_count(),
        )
    }
    pub fn weapon_effect_diagnostics(&self) -> &crate::weapon_effects::Diagnostics {
        &self.fx.weapon_effects.diagnostics
    }
    pub fn weapon_effect_backlog(&self) -> (usize, u64, usize) {
        (
            self.fx.weapon_cues.len(),
            self.fx.weapon_cue_drops,
            self.fx.weapon_light_deferred,
        )
    }
    pub fn avatar_scene(&self, owner: bri_world::OwnerId) -> Option<&SceneData> {
        self.avatar.avatars.get(&owner).map(|avatar| &avatar.data)
    }
    /// A body's object transform (feet and facing), as drawn this frame.
    pub fn avatar_body(&self, owner: bri_world::OwnerId) -> Option<glam::Mat4> {
        Some(self.avatar.avatars.get(&owner)?.body_transform())
    }
    /// A body's action sequence and whether it is still blending in.
    pub fn avatar_action(&self, owner: bri_world::OwnerId) -> Option<(&'static str, bool)> {
        Some(self.avatar.avatars.get(&owner)?.action())
    }
    /// A body's posed node in the world, as drawn this frame.
    pub fn avatar_node(&self, owner: bri_world::OwnerId, name: &str) -> Option<glam::Mat4> {
        self.avatar
            .avatars
            .get(&owner)?
            .world_node(&self.avatar.avatar_assets, name)
    }
    /// The camera the last rendered frame was drawn from: eye, yaw, pitch.
    pub fn rendered_camera(&self) -> Option<(Vec3, f32, f32)> {
        self.view.rendered_camera
    }
    /// Where vehicle `id` was last drawn: position, rotation and turret aim,
    /// interpolated between the host's poses (the camera of a rider rides
    /// this, not the newest pose).
    pub fn drawn_vehicle(&self, id: u64) -> Option<(Vec3, glam::Quat, [f32; 2])> {
        let frame = self.vehicles.frame(id)?;
        Some((frame.position, frame.rotation, frame.turret_aim))
    }
    /// The last drawn view's roll, radians (see [`crate::controls::roll`]).
    pub fn rendered_roll(&self) -> f32 {
        self.view.rendered_roll
    }
    /// The local player's image in `hand`, placed as drawn this frame.
    pub fn held_image_transform(&self, hand: u8) -> Option<glam::Mat4> {
        let owner = self.network_view()?.owner;
        self.world_items.mounted_transform(owner, hand)
    }
    pub fn building(&self) -> Option<&crate::building::Building> {
        self.build.building.as_ref()
    }
    pub fn pending_requests(&self) -> usize {
        self.net.pending_actions.len() + self.files.file_jobs.len()
    }
    /// Whether the actual region GPU line buffer is populated this frame.
    pub fn region_outlines_visible(&self) -> bool {
        self.gpu
            .region_lines
            .as_ref()
            .is_some_and(|lines| !lines.is_empty())
    }
    /// True when the CPU render snapshot has caught up with the latest replica.
    pub fn world_render_ready(&self) -> bool {
        self.network_view().is_some_and(|view| {
            self.scene
                .world_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &view.world))
        })
    }
    /// The map's lighting has finished loading and is on the GPU. A
    /// lighting source or bake landing later uploads the whole map scene
    /// again with all its textures, whenever its worker finishes: waits
    /// that measure steady play wait for this, not for the clock.
    pub fn map_lighting_settled(&self) -> bool {
        self.world_render_ready()
            && self.lighting.light_volume.settled()
            && !self.switchable_sheets_due()
            && self.gpu.gpu_scene.is_some()
    }
    /// Presented (predicted and interpolated) local state and camera eye.
    pub fn local_motion(&self) -> Option<(bri_sim::player::PlayerState, Option<Vec3>)> {
        let view = self.network_view()?;
        let state = self.motion.presented().get(&view.owner)?.clone();
        Some((state, self.local_eye()))
    }
    /// How far the current host or join attempt has got: a number that
    /// grows with every step of its loading, and None with no attempt. Waits
    /// in tests watch it to tell a slow load from a stopped one.
    /// Host the next games on a port the system picks, free when it is
    /// bound, instead of `$Pref::Server::Port`; [`App::hosted_port`] says
    /// which. For tests and tools that host side by side: a port picked
    /// first and bound later can be taken in between.
    pub fn host_on_any_port(&mut self) {
        self.host_any_port = true;
    }
    /// The port this game's own server listens on, once it is connected.
    pub fn hosted_port(&self) -> Option<u16> {
        self.net.attempt.as_ref()?.worker.probes.get()?.host_port
    }
    pub fn loading_revision(&self) -> Option<u64> {
        self.net
            .attempt
            .as_ref()
            .map(|a| a.progress.snapshot().revision)
    }
    pub fn network_view(&self) -> Option<&network::View> {
        self.net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref())
    }
    /// How many cosmetic entities this client simulates and draws, for the
    /// headless performance probes.
    pub fn entity_counts(&self) -> serde_json::Value {
        let world = |w: &bri_fx_runtime::EffectsWorld| serde_json::json!({ "sources": w.source_count(), "particles": w.particle_count() });
        let drawn = self.gpu.effects_renderer.as_ref().map(|r| r.stats());
        serde_json::json!({
            "brick_effects": world(&self.fx.effects.world),
            "brick_effects_deferred": self.fx.effects.deferred,
            "weapon_effects": world(self.fx.weapon_effects.world()),
            "actor_effects": world(self.fx.actor_effects.world()),
            "particles_drawn": drawn.map_or(0, |s| s.instances),
            "particles_cut": self.fx.effect_sprites_cut,
            "particle_draw_calls": drawn.map_or(0, |s| s.draw_calls),
            "particle_upload_bytes": drawn.map_or(0, |s| s.uploaded_bytes),
            "avatars": self.avatar.avatars.len(),
            "vehicles": self.network_view().map_or(0, |v| v.vehicles.len()),
            "projectiles": self.network_view().map_or(0, |v| v.weapons.projectiles.len()),
            "explosion_debris": self.fx.explosion_debris.models().count(),
            "shells": self.fx.weapon_shells.active_count(),
            "brick_debris": self.fx.brick_debris.len(),
        })
    }
    /// Map whose scene is installed and drawn.
    pub fn scene_map(&self) -> Option<&str> {
        self.scene.scene_map.as_deref()
    }
    /// The local player as presented this frame (prediction included).
    pub fn presented_local(&self) -> Option<&bri_sim::player::PlayerState> {
        let owner = self.network_view()?.owner;
        self.motion.presented().get(&owner)
    }
}
