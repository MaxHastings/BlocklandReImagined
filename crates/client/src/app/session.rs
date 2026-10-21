//! Hosting, joining, commands and their replies.
use super::*;

/// The connection to a game: the attempt in flight, its pending requests and what was last sent.
pub(super) struct SessionState {
    /// Movement the server's map rules currently allow (the Tutorial's lessons).
    pub(super) abilities: bri_sim::session::Abilities,
    /// The held gun's ammunition, as the server last said
    /// (`Notice::Ammo`), for HUD regions.
    pub(super) held_ammo: Option<bri_sim::session::HeldAmmo>,
    pub(super) inventory_ammo: bri_sim::session::InventoryAmmoSlots,
    /// The next bottom print is the ammo display (`HeldAmmo::printed`).
    pub(super) next_print_is_ammo: bool,
    /// Last brick inventory state reported to the server.
    pub(super) brick_hand: Option<bri_sim::session::BrickHand>,
    pub(super) attempt: Option<Attempt>,
    /// Steering prefs last sent to this session (`SteeringPrefsEvent`).
    pub(super) steering_sent: Option<(RequestId, (bool, bool))>,
    /// Told to the player in chat once the next game is entered.
    pub(super) join_notices: Vec<String>,
    /// The environment the UI was last told of, for which session.
    pub(super) environment_sent: Option<(RequestId, bri_ui::models::environment::EnvironmentView)>,
    pub(super) pending_actions: BTreeMap<RequestId, PendingAction>,
    pub(super) dialog_epoch: u64,
    /// `dialog_epoch` when the latest trigger click was sent. A wrench or
    /// printer hit notice opens its dialog only if no tool switch, cancel or
    /// close has happened since, so a cancelled click never reopens late.
    pub(super) trigger_epoch: Option<u64>,
    /// Automatic rejoins tried since the connection last dropped.
    pub(super) reconnects: u8,
    /// The invite for the game this player hosts (`/invite` copies it).
    pub(super) invite: Option<String>,
    /// The connection the host announced this player on while they load,
    /// for chat until they enter the game. It outlives the join attempt
    /// across an Add-On reload, so the player is not announced twice.
    pub(super) arrival: Option<HeldArrival>,
}
/// A join's loading connection ([`bri_net::client::Arrival`]).
pub(super) struct HeldArrival {
    /// The join it was opened for.
    pub(super) id: RequestId,
    /// What the player joined, as they gave it.
    pub(super) address: String,
    pub(super) arrival: bri_net::client::Arrival,
    /// The newest chat line shown.
    pub(super) last_chat: u64,
}

impl App {
    pub(super) fn answer(&mut self, id: RequestId, result: Result<()>) {
        self.ui.apply(UiUpdate::ActionResult {
            id,
            result: result.map_err(|e| format!("{e:#}")),
        });
    }
    pub(super) fn handle_admin(
        &mut self,
        id: RequestId,
        action: bri_ui::models::admin::AdminAction,
    ) -> Result<()> {
        let attempt = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .context("Not connected")?;
        let snapshot = attempt
            .view
            .as_ref()
            .and_then(|v| v.admin_snapshot.as_ref())
            .context("Waiting for host administration state")?;
        if let Some(command) = crate::admin_ui::command(&action, snapshot)? {
            // Administrative operations do not carry a gameplay aim or a client-selected actor.
            attempt.worker.request(id, command)?;
            self.net.pending_actions.insert(
                id,
                PendingAction {
                    action: UiAction::Admin(action),
                    command: None,
                    dialog_epoch: self.net.dialog_epoch,
                    inspection: None,
                    dialog_request: false,
                },
            );
        } else {
            self.ui.apply_session(
                attempt.id,
                UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(
                    crate::admin_ui::refresh_state(snapshot, self.ui.core.admin.snapshot.as_ref()),
                )),
            );
            self.answer(id, Ok(()));
        }
        Ok(())
    }
    pub(super) fn disconnect(&mut self) {
        self.net.invite = None;
        self.ui.core.name_tags.clear();
        self.scene.scene_map = None;
        self.net.abilities = Default::default();
        self.net.held_ammo = None;
        self.net.inventory_ammo = Default::default();
        self.net.next_print_is_ammo = false;
        self.net.brick_hand = None;
        self.build.ghost_report = None;
        self.copy_report = None;
        self.build.remote_ghosts.clear();
        self.foliage.clear();
        self.weather.clear();
        self.audio.clear();
        self.fx.effects.clear();
        self.fx.weapon_frame_observation = None;
        self.fx.weapon_effects.reset(0);
        self.fx.actor_effects.reset(0);
        self.fx.explosion_shapes.reset(0);
        self.fx.beams.clear();
        self.fx.decals.clear();
        self.fx.body_flashes.clear();
        self.fx.body_shells.clear();
        self.fx.tutorial_targets.update(&[], 0.0);
        self.fx.map_movers.set_map(Vec::new(), Vec::new());
        self.fx.map_scatter.set_map(Vec::new());
        self.fx.explosion_debris.reset(0);
        self.fx.weapon_shells.clear();
        self.fx.weapon_cues.clear();
        self.fx.weapon_animation_cues.clear();
        self.fx.weapon_animation_drops = 0;
        self.fx.weapon_animation_cursor = 0;
        self.fx.weapon_cue_drops = 0;
        self.fx.brick_debris.clear();
        self.fx.debris_models.clear();
        self.fx.fade_models.clear();
        self.addons.package_models.clear();
        self.fx.brick_kills.clear();
        if let Some(lines) = &mut self.gpu.hidden_lines {
            lines.clear();
        }
        self.gpu.hidden_uploaded = None;
        self.gpu.hidden_outlines.clear();
        if let Some(lines) = &mut self.gpu.region_lines {
            lines.clear();
        }
        self.gpu.region_outlines.clear();
        if let Some(lines) = &mut self.gpu.selection_lines {
            lines.clear();
        }
        self.gpu.selection_uploaded = None;
        if let Some(shapes) = &mut self.world_shapes {
            shapes.clear();
        }
        self.shapes_uploaded = None;
        self.fx.weapon_light_deferred = 0;
        self.fx.effect_sprites_cut = 0;
        self.fx.weapon_effect_session = None;
        self.world_items.reset();
        if let Some(mut attempt) = self.net.attempt.take() {
            self.closing.retain(|task| !task.is_finished());
            self.closing.extend(attempt.worker.finish());
        }
        self.ui.apply(UiUpdate::UnsavedChanges(false));
        self.addons.client_code.stop();
        self.addons.edits.game_ended();
        self.avatar.avatars.clear();
        self.avatar.mount_meshes.clear();
        self.avatar.avatar_actions.clear();
        self.avatar.avatar_threads.clear();
        self.avatar.avatar_action_images.clear();
        self.controls = Controls::default();
        self.scene.cpu_scene = None;
        self.scene.backdrop = None;
        self.scene.set_regions(None);
        self.scene.decals = None;
        self.scene.receiver_index = None;
        self.scene.decal_feed = Default::default();
        self.gpu.decal_layer = None;
        self.scene.fog_blend = Default::default();
        self.gpu.gpu_backdrop = None;
        self.lighting.light_volume = LightVolumeState::default();
        self.scene.cpu_terrain.clear();
        self.scene.cpu_water.clear();
        self.scene.scenery.reset();
        self.gpu.gpu_scene = None;
        self.gpu.gpu_terrain.clear();
        self.gpu.gpu_water.clear();
        self.scene.meshes = None;
        self.scene.mirror_shapes = Default::default();
        self.scene.mirror_index.clear();
        self.scene.palette = None;
        self.gpu.gpu_palette = None;
        self.scene.chunked = Default::default();
        self.scene.cpu_chunks.clear();
        self.scene.cpu_chunk_bricks.clear();
        self.gpu.gpu_chunks.clear();
        self.gpu.gpu_chunk_bricks.clear();
        self.scene.chunk_hides.clear();
        self.gpu.chunk_uploads.clear();
        self.fx.brick_fades.clear();
        self.fx.fade_models.clear();
        self.scene.chunks_left_out.clear();
        self.scene.world_source = None;
        self.scene.world_revision = 0;
        self.scene.world_log = None;
        self.scene.world_job = None;
        self.scene.materials = None;
        self.build.building = None;
        self.ui
            .apply(UiUpdate::Tools(vec![None; bri_sim::session::TOOL_SLOTS]));
        self.ui.apply(UiUpdate::SetActiveTool(None));
        self.net.pending_actions.clear();
        self.ui.core.admin = Default::default();
        self.ui.core.minigames = Default::default();
        self.build.tool_ui.invalidate();
        if let Some(update) = self.build.tool_ui.reset_music_offer() {
            self.ui.apply(update);
        }
        self.scene.query_source = None;
        self.scene.query_log = None;
        self.net.dialog_epoch = self.net.dialog_epoch.wrapping_add(1);
        self.gpu.ghost_gpu = None;
        self.gpu.ghost_look = None;
        self.gpu.ghost_uploaded = u64::MAX;
        self.build.remote_ghosts.clear();
        self.motion.reset();
        // Nothing is ridden any more, and the camera forgets the game's
        // eyes. The crosshair, wheel and overlay flags mirror what the UI
        // was told; the next frame's `update_held_weapon` settles them.
        self.mounts.mount_heading = None;
        self.mounts.seated_on = None;
        self.mounts.takes_turret = false;
        self.mounts.seat_report = None;
        self.mounts.rider_rotations.clear();
        self.mounts.rider_fallback_hips.clear();
        self.mounts.rider_hands.clear();
        self.mounts.rider_aims.clear();
        self.mounts.rider_overlays.clear();
        self.mounts.posed_eye = None;
        self.mounts.tumble = None;
        self.view.observer_eye = None;
        self.view.rendered_camera = None;
        self.view.addon_camera = false;
        self.view.rendered_roll = 0.0;
        self.view.pixels_per_unit = None;
        self.view.drawn_controls = None;
        self.scene.liquid_cache = None;
        self.ghosts.clear();
        self.vehicles.clear();
        self.music_world = None;
        self.controls.clear_observer();
        self.build.macro_recording = None;
        self.build.macro_playback.clear();
        self.combat = Default::default();
        // A save waiting on the colour question belongs to the session
        // that just ended.
        if let Some((request, _)) = self.files.color_load.take() {
            self.ui.core.pop(ScreenId::LoadBricksColor);
            self.answer(request.id, Err(anyhow::anyhow!(bri_ui::api::LOAD_CANCELED)));
        }
    }
    /// The name and clan tags a join sends (`onConnectRequest`'s name,
    /// `$Pref::Player::ClanPrefix` and `ClanSuffix`).
    pub(super) fn join_name(&self) -> bri_net::protocol::JoinName {
        let avatar = &self.ui.settings().avatar;
        bri_net::protocol::JoinName {
            name: player_name(avatar),
            clan: clan(avatar),
        }
    }
    /// Game start: ask for a name once while it is still the stock "Blockhead".
    /// A first run asks after its controls and welcome questions instead
    /// (`Core::first_run_welcome`), so a fresh install asks once.
    pub fn prompt_for_name(&mut self) {
        // The stored settings, not `Ui::settings()`, which always fills in
        // the live binds: a fresh install has none saved until its controls
        // question is answered.
        if self.ui.core.settings.binds.is_some() {
            self.ui.core.name_prompt();
            self.ui.update(0);
        }
    }
    /// Avatar Done while connected also renames the player on the server.
    pub(super) fn send_name(&mut self, prefs: &AvatarPrefs) {
        let name = player_name(prefs);
        let current = self
            .network_view()
            .and_then(|v| v.names.get(&v.owner).cloned());
        if current.as_deref() != Some(name.as_str())
            && let Some(a) = self.net.attempt.as_mut().filter(|a| a.entered)
        {
            let _ = a.worker.request(REPORT_REQUEST, Command::SetName(name));
        }
        // The host ignores tags it already has, so Done sends them each time.
        if let Some(a) = self.net.attempt.as_mut().filter(|a| a.entered) {
            let _ = a
                .worker
                .request(REPORT_REQUEST, Command::SetClan(clan(prefs)));
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn host(
        &mut self,
        id: RequestId,
        map: String,
        mode: ServerMode,
        game_mode: Option<String>,
        max_players: u32,
        name: String,
        password: String,
        admin: String,
        super_admin: String,
    ) -> Result<()> {
        ensure!(
            password.is_empty(),
            "The server join password is not connected yet. It cannot be silently ignored."
        );
        // v20's Tutorial is a single-player walkthrough (the main menu's
        // Tutorial button): one spawn and a script per player, no guests.
        let mode = if map.contains("map_tutorial") {
            ServerMode::SinglePlayer
        } else {
            mode
        };
        let admin = bri_admin::Secret::new(admin)?;
        let super_admin = bri_admin::Secret::new(super_admin)?;
        ensure!((1..=64).contains(&max_players), "Invalid player limit");
        let host_palette = if map.contains("map_tutorial") {
            None
        } else {
            crate::colorsets::selected(
                &self.content.paths.root,
                &self.state_dir,
                self.ui.core.prefs.str_or("$Pref::Server::ColorSet", ""),
            )?
        };
        // A host runs its own Add-On list as it is now; a game joined before
        // may have loaded another server's.
        self.disconnect();
        if !self.addons.packages_from_tools {
            let set = bri_package::packages::PackageSet::load_root(&self.content.paths.root)?;
            self.apply_packages(&set)?;
        }
        // What runs: the chosen game mode's Add-Ons, or (Custom) every
        // enabled Add-On that fits the map. A package world stands on its
        // environment map; the packages then generate the ground.
        let hosted = crate::packages::hosted(
            self.addons.server_packages.as_ref(),
            &map,
            game_mode.as_deref(),
        )?;
        let map = hosted.map.clone();
        ensure!(
            self.content.maps.iter().any(|m| m.id == map),
            "This map has no usable native bundle yet"
        );
        let paths = self.content.paths.clone();
        let lighting = self.graphics.lighting;
        let light_cache = self.state_dir.join("light-volumes");
        let paths_for_maps = paths.clone();
        let base_map = hosted.base_map.clone();
        let add_ons =
            self.addons
                .server_packages
                .clone()
                .map(|server| bri_net::host_setup::HostedAddOns {
                    server,
                    mode: game_mode.clone(),
                    saves: Some(self.state_dir.join("packages")),
                });
        // Admin Change Map choices (the Tutorial has its own entry point).
        let map_list: Vec<_> = self
            .content
            .maps
            .iter()
            .filter(|m| !m.id.contains("map_tutorial"))
            .map(|m| bri_sim::session::MapListing {
                id: m.id.clone(),
                name: m.name.clone(),
            })
            .collect();
        let weapon_snapshot = self.content.weapons.clone();
        let physics_snapshot = self.content.item_physics.clone();
        let selected = self.content.selectable.clone();
        let avatar_catalog = self.avatar.avatar_assets.package.clone();
        let body_mounts =
            bri_sim::session::shape_mount_points(&self.avatar.avatar_assets.rig.shape);
        // Shots leave the muzzle each image draws at in the hand.
        let body_grips = bri_sim::session::shape_grips(&self.avatar.avatar_assets.rig, &[]);
        let package_body_grips = bri_net::content_identity::package_body_grips(
            &self.content.paths.root,
            &self.content.paths.packages,
        );
        let image_barrels = self.world_items.assets().image_barrels();
        let mut catalog = self.build.tool_ui.server_catalog();
        // Start Game's Music Files: the loops this game's music bricks offer.
        let prefs = &self.ui.core.prefs;
        let off: std::collections::BTreeSet<&str> = self
            .content
            .music
            .iter()
            .filter(|(_, name)| !bri_ui::screens::music::music_enabled(prefs, name))
            .map(|(id, _)| id.as_str())
            .collect();
        catalog.sounds.retain(|id| !off.contains(id.as_str()));
        let player = self.join_name();
        let local_name = if name.trim().is_empty() {
            "Blockland ReImagined".into()
        } else {
            name
        };
        let single = mode == ServerMode::SinglePlayer;
        let internet = mode == ServerMode::Internet;
        let max_players = if single { 1 } else { max_players };
        // Start Game's Advanced Config: v20's saved `$Pref::Server::*`.
        let server_settings = crate::admin_ui::host_settings(
            &bri_ui::models::admin::options_from_prefs(&self.ui.core.prefs),
            &local_name,
            u16::try_from(max_players).unwrap_or(1),
        );
        let listing_name = local_name.clone();
        let listing_map = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map)
            .map_or_else(|| map.clone(), |m| m.name.clone());
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let (router_tx, router) = mpsc::channel();
        let state_dir = self.state_dir.clone();
        // This game's recovery snapshot takes the slot: a build an earlier
        // game left there that the player has not answered for is kept as
        // a save first, never overwritten.
        match crate::recovery::make_room(&state_dir, &self.files.saves) {
            Ok(crate::recovery::Room::Free) => {}
            Ok(crate::recovery::Room::Kept(name)) => bri_console::echo(format!(
                "The build an earlier game left unsaved is in Load Bricks as {name}."
            )),
            Ok(crate::recovery::Room::SetAside(aside)) => bri_console::warn(format!(
                "The build an earlier game left unsaved is damaged and cannot be loaded; \
                 it was moved to {}.",
                aside.display()
            )),
            Err(error) => bri_console::warn(format!(
                "Could not keep the build an earlier game left unsaved: {error:#}"
            )),
        }
        let copies = Arc::new(crate::copies::CopyFiles::new(self.files.old_saves.clone()));
        // What Add-Ons' rules keep between games (money, ranks, saved
        // configs), beside the host's other state.
        let addon_data: Arc<dyn bri_sim::session::AddOnData> = Arc::new(
            bri_net::addon_data::AddOnDataFiles::new(self.state_dir.join("addon-data")),
        );
        let load_limit = self.load_limit.clone();
        // v20's `$Pref::Server::Port`, 28000 unless the player changed it.
        let port = u16::try_from(self.ui.core.prefs.i64_or("$Pref::Server::Port", 28000))
            .ok()
            .filter(|p| *p != 0)
            .unwrap_or(bri_net::invite::DEFAULT_PORT);
        let port = if self.host_any_port { 0 } else { port };
        // A STUN server the player names finds the public address when the
        // router reports none; empty (the default) asks no one.
        let stun = Some(
            self.ui
                .core
                .prefs
                .str_or("$Pref::Server::StunServer", "")
                .trim()
                .to_string(),
        )
        .filter(|s| !s.is_empty());
        self.disconnect();
        self.ui.apply_session(
            id,
            UiUpdate::Connection(ConnectionState::Loading {
                map: map.clone(),
                preview: self
                    .content
                    .maps
                    .iter()
                    .find(|m| m.id == map)
                    .map_or(IconRef::None, |m| m.preview.clone()),
                status: "LOADING".into(),
                progress: 0.0,
            }),
        );
        let event_catalog = self.content.events.clone();
        let event_sounds: Vec<String> = self
            .content
            .event_sounds
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let progress = bri_progress::Progress::new();
        let palette_for_maps = host_palette.clone();
        let record = self
            .ui
            .core
            .prefs
            .bool_or(bri_ui::api::RECORD_MATCHES_PREF, false);
        progress.set_subject(&map);
        let reporting = progress.clone();
        let host_runtime = self.host_runtime.handle().clone();
        let worker = Worker::start(self.runtime.handle(), progress.clone(), async move {
            let identity_file = state_dir.join("client.identity");
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            reporting.begin(
                bri_progress::Stage::LoadingMap,
                bri_progress::Unit::Steps,
                None,
            );
            let permit = load_limit.acquire_owned().await?;
            let animation_avatar = avatar_catalog.clone();
            let (
                loaded,
                visual,
                identity,
                catalog,
                weapon_pack,
                animation_assets,
                body_pose_bindings,
                item_bounds,
                (vehicle_pack, bot_kinds),
                recipe_start,
            ) = tokio::task::spawn_blocking(move || -> Result<_> {
                let _permit = permit;
                let weapons = paths.weapon_content()?;
                weapon_snapshot.ensure_same(&weapons)?;
                let (animation_assets, body_pose_bindings) =
                    bri_net::content_identity::body_pose_assets(
                        &paths.root,
                        &paths.packages,
                        &paths.avatar,
                        &animation_avatar,
                        &weapons.pack,
                        &paths.item_presentation,
                        &paths.weapon_extras,
                    )?;
                let item_physics = paths.item_physics(&weapons)?;
                physics_snapshot.ensure_same(&item_physics)?;
                // Package worlds grow their ground when the session is
                // built, so a recording keeps the world as it starts, and
                // the palette Change Map paints with.
                let start_world = paths.start_world(&base_map, None, host_palette.as_deref())?;
                let recipe_start = if record {
                    Some((
                        start_world.clone(),
                        paths.map_palette(host_palette.as_deref())?,
                    ))
                } else {
                    None
                };
                let loaded = paths.map_content()?.load(start_world)?;
                let visual = load_visual_map(paths.maps.root(&base_map)?, &base_map, lighting)?;
                let mut light_volume = LightVolumeState::start(
                    &visual.scene,
                    &light_cache,
                    visual.modern_lights.as_deref(),
                );
                light_volume.animations = visual.light_animations.clone();
                light_volume.spots = visual.light_spots.clone();
                light_volume.overflow = visual.overflow_lights.clone();
                light_volume.use_prepared_volume(visual.light_volume.clone());
                light_volume.set_light_shapes(&loaded.breakables);
                // Every package this host loaded, hashed: what joiners must match.
                let identity = paths.environment()?;
                let vehicle_pack = paths.vehicle_pack()?;
                let bot_kinds = paths.bot_kinds()?;
                let meshes = Arc::new(
                    loaded
                        .simulation
                        .definitions
                        .entries
                        .iter()
                        .map(|(id, def)| (id.clone(), def.mesh.clone()))
                        .collect(),
                );
                let mirror_shapes =
                    Arc::new(crate::mirrors::shapes(&loaded.simulation.definitions));
                let materials = paths.materials()?;
                let palette = Arc::new(crate::world_chunks::BrickPalette::new(&materials)?);
                let mut mirror = bri_sim::prediction::CollisionMirror::new(
                    loaded.simulation.definitions.clone(),
                    loaded.query_colliders.clone(),
                    loaded.simulation.waters.clone(),
                );
                mirror.attach_terrain(loaded.terrain.clone())?;
                mirror.set_breakables(&loaded.breakables);
                mirror.set_movers(&loaded.movers);
                let mut building = crate::building::Building::new(
                    loaded.simulation.definitions.clone(),
                    loaded.query_colliders.clone(),
                )?;
                building.set_breakables(&loaded.breakables);
                building.attach_terrain(loaded.terrain.clone());
                building.set_catalog(selected)?;
                if let Some(print) = &catalog.default_print {
                    building.set_default_prints(
                        catalog
                            .brick_print_aspects
                            .keys()
                            .map(|id| (id.clone(), print.clone()))
                            .collect(),
                    )?;
                }
                let waters = loaded.simulation.waters.clone();
                let plate = loaded.plate;
                let movers = loaded.movers.clone();
                let foliage = crate::foliage::PreparedFoliage::load_for_map(
                    &paths.foliage,
                    paths.maps.root(&base_map)?,
                    &base_map,
                    &building,
                    &waters,
                )?;
                Ok((
                    loaded,
                    Prepared {
                        foliage,
                        map_effects: crate::effects::load_map_effects(paths.maps.root(&base_map)?)?,
                        map_id: base_map.clone(),
                        waters,
                        plate,
                        scene: visual.scene,
                        terrain: visual.terrain.into_iter().map(Arc::new).collect(),
                        repeated_water: visual.repeated_water.into_iter().map(Arc::new).collect(),
                        meshes,
                        mirror_shapes,
                        materials,
                        palette,
                        building,
                        mirror,
                        shape_indices: visual.shape_indices,
                        map_movers: movers,
                        mover_scenes: visual.movers,
                        scatter: visual.scatter,
                        backdrop: visual.backdrop,
                        regions: visual.regions,
                        decals: visual.decals,
                        receiver_index: visual.receiver_index,
                        light_volume,
                    },
                    identity,
                    catalog,
                    weapons.pack,
                    animation_assets,
                    body_pose_bindings,
                    item_physics.bounds,
                    (vehicle_pack, bot_kinds),
                    recipe_start,
                ))
            })
            .await??;
            scene_tx.send(visual).context("Loading cancelled")?;
            reporting.begin(
                bri_progress::Stage::StartingServer,
                bri_progress::Unit::Steps,
                None,
            );
            // Tests set BRI_TEST_HOST_PORT so a hosted test game never takes
            // the port of a real game running on this machine.
            let port = std::env::var("BRI_TEST_HOST_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(port);
            let bind: SocketAddr = if single {
                "127.0.0.1:0".parse()?
            } else {
                SocketAddr::from(([0, 0, 0, 0], port))
            };
            let setup = Arc::new(bri_net::host_setup::HostSetup {
                // v20 `$Server::LAN`: single-player and LAN hosts keep the looser
                // brick-damage rule; internet hosts use miniGameCanDamage.
                lan: !internet,
                content: bri_net::host_setup::SessionContent {
                    animation_assets,
                    body_pose_bindings,
                    tool_catalog: catalog,
                    weapon_pack,
                    item_bounds,
                    avatar_catalog,
                    body_mounts,
                    body_grips,
                    package_body_grips,
                    image_barrels,
                    vehicle_pack,
                    bot_kinds,
                    event_catalog,
                    event_sounds,
                },
                maps: map_list,
                copies: Some(copies),
                addon_data: Some(addon_data),
                game_version: Some(crate::updates::version()),
                // `/botreload` reads the Add-Ons' bots.json again; `/botsave`
                // keeps dial overrides beside settings.json, never in the
                // install folder, and every hosted game starts with them.
                bot_tuning: Some(bri_sim::session::BotTuning {
                    reload: Some({
                        let paths = paths_for_maps.clone();
                        std::sync::Arc::new(move || paths.bot_kinds())
                    }),
                    overrides: Some(state_dir.join(bri_sim::bot_kind::tuning::OVERRIDES_FILE)),
                }),
                bot_overrides: None,
                // Change Map keeps the host's Server Settings.
                settings: Some(server_settings),
                passwords: Some((admin, super_admin)),
                add_ons,
                load_map: Some({
                    let paths = paths_for_maps.clone();
                    Arc::new(move |map: &str| {
                        Ok(paths
                            .load_map_with_palette(map, None, palette_for_maps.as_deref())?
                            .into_session())
                    })
                }),
            });
            let package_save = setup.package_save(&hosted)?;
            let (session, spawn_points) =
                setup.session_with(&hosted, loaded.into_session(), package_save.as_deref())?;
            // A recording that cannot start leaves the game unrecorded.
            let recording = if let Some((start_world, map_palette)) = recipe_start {
                let dir = state_dir.join(bri_net::replay::RECORDINGS_DIR);
                match bri_net::replay::next_recording(&dir) {
                    Ok(path) => Some(bri_net::replay::Recording {
                        path,
                        host: setup.recipe(&hosted, start_world, package_save, map_palette),
                    }),
                    Err(error) => {
                        eprintln!("Not recording this match ({}): {error:#}", dir.display());
                        None
                    }
                }
            } else {
                None
            };
            let map_loader: server::MapLoader = setup;
            // The server's tasks spawn onto the host runtime it is started in.
            let entered = host_runtime.enter();
            let mut host = server::start_with_admin_store_limit_and_recording(
                session,
                ServerOptions {
                    bind,
                    environment: identity.clone(),
                    spawn_points,
                    // LAN hosts keep one identity so joiners' saved trust stays valid.
                    certificate: if single {
                        None
                    } else {
                        Some(server::HostCertificate::load_or_create(&state_dir)?)
                    },
                    map_loader: Some(map_loader),
                    // Joiners download the Add-Ons this host runs. Listing
                    // them reads and hashes every Add-On file, so a single
                    // player game, which nobody joins, lists none.
                    packages: if single {
                        None
                    } else {
                        Some(Arc::new(bri_net::packages::PackageShelf::new(
                            &paths_for_maps.root,
                            &paths_for_maps.packages,
                            &identity,
                        )?))
                    },
                },
                max_players as usize,
                state_dir.join("administration.json"),
                recording,
            )?;
            drop(entered);
            // Unless keeping it failed above, the slot is free.
            if !crate::recovery::path(&state_dir).exists() {
                host.keep_recovery(server::Recovery::new(crate::recovery::path(&state_dir)))?;
            }
            let address = SocketAddr::from(([127, 0, 0, 1], host.address.port()));
            if !single {
                // LAN players find this host (and its certificate) by broadcast;
                // Connect to IP asks the same responder directly, so internet
                // hosts answer it too.
                // Another game on this computer may hold the LAN port; this
                // one is then joined by address only.
                if let Err(error) = host
                    .advertise(listing_name, listing_map, max_players, identity.digest())
                    .await
                {
                    bri_console::warn(format!("Not listed on the LAN: {error:#}"));
                }
            }
            if !single {
                // Windows Firewall can block friends whatever the router does.
                let firewall = router_tx.clone();
                let port = host.address.port();
                std::thread::spawn(move || {
                    let status = crate::firewall::status(port);
                    let _ = firewall.send(HostNotice::Firewall { status, port });
                });
            }
            if internet {
                let reach = router_tx.clone();
                host.open_to_internet_with(stun, move |report| {
                    let _ = reach.send(HostNotice::Reach(report));
                });
            } else if !single && let Some(ip) = bri_net::reach::local_ip() {
                let invite = bri_net::invite::invite(
                    SocketAddr::new(ip, host.address.port()),
                    &host.certificate,
                );
                let _ = router_tx.send(HostNotice::Lan { invite });
            }
            let client = Client::connect_reporting(
                address,
                &host.certificate,
                player,
                identity.client_packages(),
                None,
                Some(host.host_token.clone()),
                &native_identity,
                reporting,
            )
            .await?;
            Ok(Connected {
                client,
                host: Some(host),
                mods: Default::default(),
            })
        });
        // This game runs the Add-Ons as they are now; edits made while it
        // runs are named in chat (`add_on_edits`).
        self.addons.edits.game_started(&self.content.paths.root);
        self.net.attempt = Some(Attempt {
            id,
            worker,
            scene,
            name: local_name,
            join_target: None,
            max_players,
            local: true,
            single,
            ready: false,
            entered: false,
            view: None,
            last_chat: 0,
            talking: Vec::new(),
            router: (!single).then_some(router),
            trust: BTreeMap::new(),
            map_failure: None,
            reloading: false,
            progress,
            progress_seen: 0,
            logged_stage: None,
            stages_ended: 0,
            saved_revision: None,
            settling: None,
            identity_changed: Default::default(),
            add_ons: Default::default(),
            joined: Default::default(),
            arrival: Default::default(),
        });
        Ok(())
    }
    /// Asked when a saved server answers with a different identity.
    pub(super) fn identity_question(&self, address: &str) -> bri_ui::api::Question {
        let saved = crate::servers::SavedServers::load(&self.state_dir.join("servers.json"));
        let name = saved
            .find(address)
            .map(|s| plain_chat(&s.name))
            .filter(|name| !name.trim().is_empty())
            .map_or_else(|| address.to_string(), |name| format!("{name} ({address})"));
        bri_ui::api::Question {
            title: "Server Identity Changed".into(),
            text: format!(
                "{name} has a different identity than when you last joined.\n\nThis happens \
                 when its host reinstalls the game, but it can also mean someone else is \
                 answering at that address. Only continue if the host told you they \
                 reinstalled."
            ),
            yes: "Continue".into(),
            no: "Cancel".into(),
            on_yes: Box::new(UiAction::TrustNewServerIdentity {
                address: address.to_string(),
            }),
            on_no: None,
        }
    }
    /// Continue after an identity change: drop the saved identity and the
    /// saved invite's key, so the next join trusts what answers and saves it.
    pub(super) fn forget_server_identity(&self, address: &str) -> Result<()> {
        let key = bri_net::invite::JoinTarget::parse(address)?.address();
        update_small_json(
            &self.state_dir.join("trusted-hosts.json"),
            |pins: &mut BTreeMap<String, Vec<u8>>| {
                pins.remove(&key);
            },
        )?;
        let path = self.state_dir.join("servers.json");
        let mut saved = crate::servers::SavedServers::load(&path);
        for server in &mut saved.servers {
            if server.address.eq_ignore_ascii_case(&key) {
                server.invite = None;
            }
        }
        saved.save(&path)
    }
    pub(super) fn join(&mut self, id: RequestId, address: String, password: String) -> Result<()> {
        self.join_resuming(id, address, password, None, None, None)
    }
    /// [`App::join`] presenting `resume`, the lost connection's ticket, so
    /// the host gives the player back their number (and so their bricks)
    /// even before it has timed the old connection out, and `kept` the
    /// bricks that connection held, so the host sends only what changed
    /// since the drop. `shown` is the map
    /// the loading screen already names when the join resumes from it (the
    /// host's Add-Ons were downloaded and loaded): it stays on that screen
    /// instead of going back to the Connecting dialog.
    pub(super) fn join_resuming(
        &mut self,
        id: RequestId,
        address: String,
        password: String,
        resume: Option<bri_net::protocol::ResumeToken>,
        kept: Option<bri_net::client::KeptBricks>,
        shown: Option<String>,
    ) -> Result<()> {
        ensure!(
            self.addons.reload.is_none(),
            "Add-On loading is still in progress"
        );
        ensure!(
            password.is_empty(),
            "Password authentication is not connected yet"
        );
        let target = bri_net::invite::JoinTarget::parse(&address)?;
        let typed = target.address();
        let reload_add_ons = !std::mem::take(&mut self.addons.skip_add_on_reload);
        // An invite's key, a LAN listing or a saved pin identifies the host;
        // a first join trusts the certificate the host presents and pins it.
        let pins_file = self.state_dir.join("trusted-hosts.json");
        let servers_file = self.state_dir.join("servers.json");
        let lan_hosts = self.lobby.lan_hosts.clone();
        let paths = self.content.paths.clone();
        let lighting = self.graphics.lighting;
        let light_cache = self.state_dir.join("light-volumes");
        let player = self.join_name();
        let weapon_snapshot = self.content.weapons.clone();
        let physics_snapshot = self.content.item_physics.clone();
        let selected = self.content.selectable.clone();
        let catalog = self.build.tool_ui.server_catalog();
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let load_limit = self.load_limit.clone();
        let identity_file = self.state_dir.join("client.identity");
        // Downloaded Add-Ons live in the game folder (a dot folder the
        // Add-Ons list skips), so their content loads like a local Add-On's.
        let package_cache = self.content.paths.root.join(".downloads");
        let add_ons = Arc::new(std::sync::Mutex::new(None));
        let needs_add_ons = add_ons.clone();
        let joined_list = Arc::new(std::sync::Mutex::new(None));
        let joined_add_ons = joined_list.clone();
        self.disconnect();
        let progress = bri_progress::Progress::new();
        let screen = match &shown {
            Some(map) => {
                progress.set_subject(map);
                self.loading_screen(map, &bri_progress::Snapshot::default())
            }
            None => ConnectionState::Connecting {
                text: connecting_text(bri_progress::Stage::Starting, &typed, self.net.reconnects),
            },
        };
        self.ui.apply_session(id, UiUpdate::Connection(screen));
        let pin_key = typed.clone();
        let reporting = progress.clone();
        let reporting_early = progress.clone();
        // A saved server's invite carries the key it had when joined.
        let saved_invite = crate::servers::SavedServers::load(&servers_file)
            .servers
            .iter()
            .any(|s| s.invite.as_deref() == Some(address.trim()));
        let identity_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = identity_changed.clone();
        // A join that reloads for the server's Add-Ons keeps the connection
        // the host announced the player on; a rejoin after a dropped
        // connection is not announced again.
        if self
            .net
            .arrival
            .as_ref()
            .is_some_and(|held| held.id != id || held.address != address.trim() || resume.is_some())
        {
            self.net.arrival = None;
        }
        let announced = self.net.arrival.as_ref().map(|held| {
            (
                held.arrival.ticket.clone(),
                held.arrival.certificate.clone(),
                held.arrival.listing.map.clone(),
            )
        });
        let arrival_slot = Arc::new(std::sync::Mutex::new(None));
        let arrived = arrival_slot.clone();
        let opens_arrival = announced.is_none() && resume.is_none();
        let early = (
            paths.clone(),
            selected.clone(),
            catalog.clone(),
            light_cache.clone(),
            load_limit.clone(),
        );
        let early_scene = Arc::new(std::sync::Mutex::new(None::<(String, Prepared)>));
        let early_out = early_scene.clone();
        let worker = Worker::start(self.runtime.handle(), progress.clone(), async move {
            let route = target.resolve().await?;
            let address = route.address;
            let pins = pins_file.clone();
            let saved_key = pin_key.clone();
            let saved = match route.key {
                Some(_) => None,
                None => {
                    tokio::task::spawn_blocking(move || {
                        read_small_json::<BTreeMap<String, Vec<u8>>>(&pins)
                            .and_then(|pins| pins.get(&saved_key).cloned())
                    })
                    .await?
                }
            };
            let (pin, had_pin) =
                crate::servers::join_pin(route.key, saved, lan_hosts.get(&address.to_string()));
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            let identity_paths = paths.clone();
            let (package_root, package_set) = (paths.root.clone(), paths.packages.clone());
            // Hashing this player's content is local work: the Connecting
            // dialog says so rather than blaming the host.
            reporting.begin(
                bri_progress::Stage::CheckingContent,
                bri_progress::Unit::Steps,
                None,
            );
            let permit = load_limit.clone().acquire_owned().await?;
            let identity = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let weapons = identity_paths.weapon_content()?;
                weapon_snapshot.ensure_same(&weapons)?;
                let item_physics = identity_paths.item_physics(&weapons)?;
                physics_snapshot.ensure_same(&item_physics)?;
                identity_paths.environment()
            })
            .await??;
            // A server running Add-Ons this client lacks, or has in another
            // version, refuses the join naming them; download them into the
            // package cache, load the server's set and join again. Nothing
            // asks the player (only sandboxed Add-On code does, after).
            let cache = bri_package::sync::Cache::open(&package_cache)?;
            let local = identity.client_packages();
            let mut mods = None;
            // v20 accepts the connection before the loading screen: the
            // host says the player connected, and they chat while the
            // Add-Ons download and the map loads. The game join spawns them.
            let (pin, ticket, map_early) = match announced {
                Some((ticket, certificate, map)) => {
                    (HostPin::Certificate(certificate), Some(ticket), Some(map))
                }
                None if opens_arrival => {
                    let arrival = bri_net::client::Arrival::open(
                        address,
                        &pin,
                        player.clone(),
                        None,
                        &native_identity,
                    )
                    .await;
                    let arrival = match arrival {
                        Ok(arrival) => arrival,
                        Err(error) => {
                            if (had_pin || saved_invite)
                                && matches!(
                                    error.downcast_ref::<bri_net::client::JoinError>(),
                                    Some(bri_net::client::JoinError::IdentityChanged(_))
                                )
                            {
                                changed.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                            return Err(error);
                        }
                    };
                    let found = (
                        HostPin::Certificate(arrival.certificate.clone()),
                        Some(arrival.ticket.clone()),
                        Some(arrival.listing.map.clone()),
                    );
                    if let Ok(mut slot) = arrived.lock() {
                        *slot = Some(arrival);
                    }
                    found
                }
                None => (pin, None, None),
            };
            // The map loads before the player spawns, as v20's mission
            // load comes before `onClientEnterGame`.
            let ready = async move {
                // An Add-On map this player has not downloaded yet loads
                // after the download, when the join starts again.
                let Some(map) = map_early.filter(|m| early.0.maps.contains(m)) else {
                    return Ok(());
                };
                let (paths, selected, catalog, light_cache, load_limit) = early;
                reporting_early.begin(
                    bri_progress::Stage::LoadingMap,
                    bri_progress::Unit::Steps,
                    None,
                );
                let permit = load_limit.acquire_owned().await?;
                let prepared_map = map.clone();
                let visual = tokio::task::spawn_blocking(move || -> Result<Prepared> {
                    let _permit = permit;
                    prepare_map(
                        &paths,
                        &prepared_map,
                        selected,
                        &catalog,
                        &light_cache,
                        lighting,
                    )
                })
                .await??;
                if let Ok(mut slot) = early_out.lock() {
                    *slot = Some((map, visual));
                }
                Ok(())
            };
            let joined = Client::connect_arriving(
                address,
                pin,
                player,
                local.clone(),
                resume,
                kept,
                None,
                &native_identity,
                &cache,
                reporting.clone(),
                |fetched, dropped| {
                    // Logged here, before a join that must reload for the
                    // server's Add-Ons ends this pass; the rejoin after it
                    // already has them loaded and fetches nothing.
                    log_fetched(fetched);
                    reporting.begin(
                        bri_progress::Stage::LoadingAddOns,
                        bri_progress::Unit::Steps,
                        None,
                    );
                    let (catalog, packages) = crate::mods::load_fetched(
                        &package_root,
                        &package_set,
                        &local,
                        fetched,
                        dropped,
                    )?;
                    // Prepare engine content before the transport admits a
                    // player. The existing reload continuation starts the
                    // final cached join after installation or visible fallback.
                    let set =
                        crate::mods::joined_set(&package_root, &package_set, fetched, dropped);
                    let set = if reload_add_ons { Some(set?) } else { set.ok() };
                    let reload = reload_add_ons
                        && set.as_ref().is_some_and(|set| {
                            crate::content::ContentPaths::resolve(&package_root, set).map_or(
                                true,
                                |fresh| {
                                    fresh.brick_extras != paths.brick_extras
                                        || fresh.weapon_extras != paths.weapon_extras
                                        || fresh.vehicle_extras != paths.vehicle_extras
                                        || fresh.bot_extras != paths.bot_extras
                                        // A host's Add-On map loads from the
                                        // reloaded content.
                                        || fresh.maps.iter().ne(paths.maps.iter())
                                },
                            )
                                // A body the host's players may wear (an
                                // imported Mario) loads with the content.
                                || fetched.iter().any(|f| {
                                    !crate::avatar::body_models(&f.dir, &f.package.id).is_empty()
                                })
                        });
                    if reload {
                        *needs_add_ons.lock().map_err(|_| {
                            anyhow::anyhow!("Server Add-On preparation state is unavailable")
                        })? = set;
                        return Err(bri_net::client::JoinPreparationPending.into());
                    }
                    mods = Some(catalog);
                    Ok(packages)
                },
                ticket,
                ready,
            )
            .await;
            let client = match joined {
                Ok((client, fetched, dropped)) if !fetched.is_empty() || !dropped.is_empty() => {
                    let set =
                        crate::mods::joined_set(&package_root, &package_set, &fetched, &dropped);
                    let set = if reload_add_ons { Some(set?) } else { set.ok() };
                    // No new content, but the server's Add-On code runs for
                    // this game: the host decides which (`ClientCode`).
                    if let Ok(mut slot) = joined_add_ons.lock() {
                        *slot = set;
                    }
                    client
                }
                Ok((client, _, _)) => client,
                Err(error) => {
                    // A saved server that answers with a new identity may
                    // have reinstalled, or may not be the same host: the
                    // player decides (`TrustNewServerIdentity`). A pasted
                    // invite's key came from the host just now and stands.
                    if (had_pin || saved_invite)
                        && matches!(
                            error.downcast_ref::<bri_net::client::JoinError>(),
                            Some(bri_net::client::JoinError::IdentityChanged(_))
                        )
                    {
                        changed.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    return Err(error);
                }
            };
            let mods = std::sync::Arc::new(mods.unwrap_or_default());
            // Remember the host's certificate and the server for later joins.
            let pin = client.certificate.clone();
            let name = plain_chat(&client.listing.name);
            let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                let shared = bri_net::invite::JoinTarget::Direct {
                    host: target_host(&pin_key),
                    port: address.port(),
                    key: Some(bri_net::invite::host_key(&pin)),
                };
                update_small_json(&pins_file, |pins: &mut BTreeMap<String, Vec<u8>>| {
                    if pins.len() < 1024 || pins.contains_key(&pin_key) {
                        pins.insert(pin_key.clone(), pin);
                    }
                })?;
                let mut saved = crate::servers::SavedServers::load(&servers_file);
                saved.joined(&pin_key, Some(shared.to_string()), &name, unix_now());
                saved.save(&servers_file)
            })
            .await;
            let map = client.replica.world.map_id.clone();
            ensure!(
                paths.maps.contains(&map),
                "Server map has no supported native render bundle yet"
            );
            let early = early_scene.lock().ok().and_then(|mut slot| slot.take());
            let visual = match early {
                Some((prepared, visual)) if prepared == map => visual,
                // The host changed maps while this player loaded.
                _ => {
                    reporting.begin(
                        bri_progress::Stage::LoadingMap,
                        bri_progress::Unit::Steps,
                        None,
                    );
                    let permit = load_limit.acquire_owned().await?;
                    tokio::task::spawn_blocking(move || -> Result<Prepared> {
                        let _permit = permit;
                        prepare_map(&paths, &map, selected, &catalog, &light_cache, lighting)
                    })
                    .await??
                }
            };
            scene_tx.send(visual).context("Loading cancelled")?;
            Ok(Connected {
                client,
                host: None,
                mods,
            })
        });
        self.net.attempt = Some(Attempt {
            id,
            worker,
            scene,
            name: typed.clone(),
            join_target: Some(address.trim().to_string()),
            max_players: 64,
            local: false,
            single: false,
            ready: false,
            entered: false,
            view: None,
            last_chat: 0,
            talking: Vec::new(),
            router: None,
            trust: BTreeMap::new(),
            map_failure: None,
            reloading: false,
            progress,
            progress_seen: 0,
            logged_stage: None,
            stages_ended: 0,
            saved_revision: None,
            settling: None,
            identity_changed,
            add_ons,
            joined: joined_list,
            arrival: arrival_slot,
        });
        Ok(())
    }
    pub(super) fn command(
        &mut self,
        id: RequestId,
        command: Command,
        action: UiAction,
    ) -> Result<()> {
        // Tool dialogs and inventory reconciliation need the command afterward. In particular,
        // do not clone/retain an entire loaded build while awaiting its reply.
        let retained = matches!(
            &command,
            Command::Tool(_)
                | Command::EquipTool { .. }
                | Command::DropTool { .. }
                | Command::WeaponTrigger { .. }
        )
        .then(|| command.clone());
        let inspection = match &command {
            Command::Tool(ToolAction::Inspect { mode }) => Some(*mode),
            _ => None,
        };
        // Administration requests never carry a gameplay aim.
        let aim = (!matches!(command, Command::Admin(_))).then_some(bri_sim::session::ActionAim {
            yaw: self.controls.yaw,
            pitch: self.controls.pitch,
        });
        let dialog_request = inspection.is_some()
            || matches!(
                action,
                UiAction::SetPrint { .. }
                    | UiAction::SendWrench { .. }
                    | UiAction::SendEvents { .. }
            );
        self.net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .context("Not connected")?
            .worker
            .request_with_aim(id, command, aim)?;
        self.net.pending_actions.insert(
            id,
            PendingAction {
                action,
                command: retained,
                dialog_epoch: self.net.dialog_epoch,
                inspection,
                dialog_request,
            },
        );
        Ok(())
    }
    pub(super) fn accept_reply(
        &mut self,
        attempt: &Attempt,
        request: RequestId,
        result: std::result::Result<Reply, bri_sim::session::Rejection>,
        revision: u64,
        tick: u64,
    ) {
        let Some(pending) = self.net.pending_actions.remove(&request) else {
            return;
        };
        // Placement failures use the original HUD plant-error icon and sound,
        // never a modal dialog.
        let plant_failure = match &result {
            Err(rejection) => rejection.plant,
            Ok(_) => None,
        };
        let result = result.map_err(|rejection| rejection.message);
        if pending.dialog_request && pending.dialog_epoch != self.net.dialog_epoch {
            // The dialog that asked is gone, so its data is not applied, but
            // the request is still answered: a screen left waiting on it
            // (the print selector's pending print) would otherwise refuse
            // every later request.
            if let Err(reason) = &result {
                bri_console::echo(format!("Closed dialog's request refused: {reason}"));
            }
            self.ui.apply_session(
                attempt.id,
                UiUpdate::ActionResult {
                    id: request,
                    result: Ok(()),
                },
            );
            return;
        }
        if let UiAction::Admin(action) = &pending.action {
            let accepted = (|| {
                let reply = result.map_err(anyhow::Error::msg)?;
                let Reply::Admin(reply) = reply else {
                    anyhow::bail!("Server returned an unexpected administration reply");
                };
                if matches!(
                    reply.data,
                    bri_sim::session::AdminData::BrickGroups(_)
                        | bri_sim::session::AdminData::BanList { .. }
                        | bri_sim::session::AdminData::AutoRoles(_)
                        | bri_sim::session::AdminData::Maps(_)
                ) {
                    ensure!(
                        reply.snapshot.revision >= self.ui.core.admin.revision,
                        "Administration state changed; refresh the list"
                    );
                }
                for update in crate::admin_ui::reply_updates(request, action, &reply)? {
                    if let UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(snapshot)) =
                        &update
                        && snapshot.revision < self.ui.core.admin.revision
                    {
                        continue;
                    }
                    self.ui.apply_session(attempt.id, update);
                }
                Ok(())
            })();
            self.answer(request, accepted);
            return;
        }
        if let Some(command) = &pending.command
            && let Some(building) = &mut self.build.building
        {
            for update in
                building.command_finished(request, command, result.is_ok().then_some(tick))
            {
                self.ui.apply_session(attempt.id, update);
            }
        }
        if matches!(&pending.action, UiAction::SaveBricks { .. }) {
            let queued = match result {
                Ok(Reply::Saved(build)) => self.files.file_jobs.enqueue(crate::saves::Request {
                    id: request,
                    session: Some(attempt.id),
                    action: pending.action,
                    build: Some(build),
                    revision: Some(revision),
                }),
                Ok(_) => Err(anyhow::anyhow!("Server returned an unexpected save reply")),
                Err(error) => Err(anyhow::anyhow!(error)),
            };
            if let Err(error) = queued {
                self.answer(request, Err(error));
            }
            return;
        }
        let result = result.and_then(|reply| {
            if let Some(mode) = pending.inspection {
                // Initial inspection may not interrupt a newer modal/typing
                // interaction. Events are opened only for their still-open wrench.
                let expected = if let UiAction::RequestEvents { brick } = pending.action {
                    Some(brick)
                } else {
                    None
                };
                let can_open = if expected.is_some() {
                    self.ui
                        .stack()
                        .iter()
                        .any(|s| matches!(s, ScreenId::Wrench(_) | ScreenId::WrenchEvents))
                } else {
                    self.ui.stack() == [ScreenId::Play]
                };
                if !can_open {
                    return Ok(());
                }
                let view = attempt
                    .view
                    .as_ref()
                    .ok_or_else(|| "Inspection has no active world".to_string())?;
                let updates = self
                    .build
                    .tool_ui
                    .accept_inspection(&reply, mode, expected, &view.world, &view.names, view.owner)
                    .map_err(|e| format!("{e:#}"))?;
                for mut update in updates {
                    region_defaults(&mut update, &reply, self.scene.meshes.as_deref());
                    self.ui.apply_session(attempt.id, update);
                }
            } else if matches!(
                pending.action,
                UiAction::SetPrint { .. }
                    | UiAction::SendWrench { .. }
                    | UiAction::SendEvents { .. }
            ) {
                let last_print = self
                    .build
                    .tool_ui
                    .command_accepted(
                        pending
                            .command
                            .as_ref()
                            .ok_or("Missing accepted tool command")?,
                    )
                    .map_err(|e| {
                        format!("Server accepted the edit, but dialog refresh failed: {e:#}")
                    })?;
                if let Some(last) = last_print
                    && let Some(building) = self.build.building.as_mut()
                {
                    building
                        .remember_print(&last)
                        .map_err(|e| format!("Last print was not remembered: {e:#}"))?;
                }
            }
            Ok(())
        });
        let result = match plant_failure {
            Some(failure) => {
                self.ui
                    .apply_session(attempt.id, UiUpdate::PlantError(plant_error(failure)));
                Ok(())
            }
            None => result,
        };
        // A click the server refuses (nothing in hand yet, just after a
        // respawn or Change Map) shows nothing, as in v20; the reason goes
        // to the log only.
        let result = match (&pending.command, result) {
            (Some(Command::WeaponTrigger { .. }), Err(reason)) => {
                bri_console::echo(format!("Trigger ignored: {reason}"));
                Ok(())
            }
            (_, result) => result,
        };
        self.ui.apply_session(
            attempt.id,
            UiUpdate::ActionResult {
                id: request,
                result,
            },
        );
    }
    /// Put the load's progress on screen: the loading screen for a host
    /// start, a join once the host has answered (or anything but connecting
    /// has begun), and a map change once the new world starts arriving.
    /// Until the host answers, a join is in the Connecting dialog, which
    /// names what it waits on. Nothing changes once in game.
    pub(super) fn show_progress(&mut self, a: &mut Attempt) {
        let snapshot = a.progress.snapshot();
        if snapshot.revision == a.progress_seen {
            return;
        }
        a.progress_seen = snapshot.revision;
        let (passed, ended) = a.progress.ended_since(a.stages_ended);
        a.stages_ended = ended;
        let subject = a.progress.subject();
        let showing = match &self.ui.core.conn {
            ConnectionState::Connecting { .. } | ConnectionState::Loading { .. } => true,
            // A map change: the host's new world has started to arrive.
            ConnectionState::InGame { .. } => {
                subject.is_some()
                    && (a.reloading
                        || (snapshot.stage == bri_progress::Stage::ReceivingWorld
                            && self.scene.scene_map.as_deref() != subject.as_deref()))
            }
            _ => false,
        };
        if !showing
            || (a.entered && !a.reloading && snapshot.stage != bri_progress::Stage::ReceivingWorld)
        {
            return;
        }
        // Each stage the screen shows goes to the session log with how long
        // the one before took, so a player's log says where a slow or stuck
        // load sat. A stage that began and ended between two frames (a
        // small download over a fast link) is named too.
        let what = subject.as_ref().map_or_else(
            || format!("Joining {}", a.name),
            |map| format!("Loading {map}"),
        );
        for (i, ended) in passed.iter().enumerate() {
            let logged = a
                .logged_stage
                .is_some_and(|(stage, _)| stage == ended.stage);
            if i > 0 || !logged {
                bri_console::echo(format!("{what}: {} (between frames)", ended.status()));
            }
        }
        if a.logged_stage
            .is_none_or(|(stage, _)| stage != snapshot.stage)
        {
            let now = std::time::Instant::now();
            let previous = a.logged_stage.map_or(String::new(), |(_, since)| {
                format!(" (after {} ms)", now.duration_since(since).as_millis())
            });
            bri_console::echo(format!("{what}: {}{previous}", snapshot.status()));
            a.logged_stage = Some((snapshot.stage, now));
        }
        let screen = match subject {
            Some(map) => self.loading_screen(&map, &snapshot),
            // Downloading the host's Add-Ons, or anything else after the
            // host answered, is never hidden behind "Connecting": it gets
            // the loading screen's bar even if the host never named its map.
            None if !in_connecting_dialog(snapshot.stage)
                || matches!(self.ui.core.conn, ConnectionState::Loading { .. }) =>
            {
                self.loading_screen(&a.name, &snapshot)
            }
            None => ConnectionState::Connecting {
                text: connecting_text(snapshot.stage, &a.name, self.net.reconnects),
            },
        };
        if self.ui.core.conn != screen {
            self.ui.apply_session(a.id, UiUpdate::Connection(screen));
        }
    }
    /// The loading screen for `map` (a map id, the host's listed map name,
    /// or the server's name when it named no map) at `snapshot`.
    pub(super) fn loading_screen(
        &self,
        map: &str,
        snapshot: &bri_progress::Snapshot,
    ) -> ConnectionState {
        let preview = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map || m.name == map)
            .map_or(IconRef::None, |m| m.preview.clone());
        ConnectionState::Loading {
            map: map.to_string(),
            preview,
            status: snapshot.status(),
            progress: snapshot.fraction(),
        }
    }
    /// Everything the HUD takes from the map and the building controller.
    /// First entry sends it, and every map change sends it again, since the
    /// change replaces both.
    pub(super) fn map_setup_updates(
        &self,
        scene: &SceneData,
        palette: &[[f32; 4]],
    ) -> Result<Vec<UiUpdate>> {
        let mut updates = vec![
            UiUpdate::Bricks(self.content.bricks.clone()),
            UiUpdate::Colorset(self.colorset(palette)),
            UiUpdate::Datablocks(self.content.datablocks.clone()),
            UiUpdate::BuildingAllowed(true),
        ];
        updates.extend(self.build.tool_ui.catalog_updates());
        updates.extend(
            self.build
                .building
                .as_ref()
                .context("Ready connection has no building controller")?
                .initial_updates(),
        );
        // The name the save list files this map's saves under (`Store::map_name`),
        // so the dialogs open on the map being played.
        let map = self.content.maps.iter().find(|m| m.id == scene.id);
        updates.push(UiUpdate::SaveContext {
            map: map.map_or_else(|| scene.name.clone(), |m| m.name.clone()),
            preview: map.map(|m| m.preview.clone()).unwrap_or(IconRef::None),
        });
        Ok(updates)
    }
}
/// Stages a join spends in the Connecting dialog before the host answers:
/// this player's own content check, and reaching the host. Everything
/// after is on the loading screen.
pub(super) fn in_connecting_dialog(stage: bri_progress::Stage) -> bool {
    matches!(
        stage,
        bri_progress::Stage::Starting
            | bri_progress::Stage::CheckingContent
            | bri_progress::Stage::Connecting
    )
}
/// The Connecting dialog's text for a join to `server` in `stage`.
pub(super) fn connecting_text(stage: bri_progress::Stage, server: &str, reconnects: u8) -> String {
    match stage {
        bri_progress::Stage::CheckingContent => "Checking your game content…".into(),
        _ if reconnects > 0 => format!("Connection lost. Reconnecting to {server}…"),
        _ => format!("Connecting to {server}…"),
    }
}
/// The host's Add-Ons a join needed, each either downloaded now or already
/// in the download cache from an earlier visit, for the session log.
fn log_fetched(fetched: &[bri_net::packages::Fetched]) {
    for f in fetched {
        let package = &f.package;
        if f.downloaded > 0 {
            let size = if f.downloaded < 1024 * 1024 {
                format!("{} KB", f.downloaded.div_ceil(1024))
            } else {
                format!("{:.1} MB", f.downloaded as f64 / (1024.0 * 1024.0))
            };
            bri_console::echo(format!(
                "Downloaded Add-On {} {} ({size})",
                package.id, package.version
            ));
        } else {
            bri_console::echo(format!(
                "Add-On {} {} was already downloaded",
                package.id, package.version
            ));
        }
    }
}
