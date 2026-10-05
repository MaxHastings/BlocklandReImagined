//! Hosting, joining, commands and their replies.
use super::*;

/// The connection to a game: the attempt in flight, its pending requests and what was last sent.
pub(super) struct SessionState {
    /// Movement the server's map rules currently allow (the Tutorial's lessons).
    pub(super) abilities: bri_sim::session::Abilities,
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
        self.net.brick_hand = None;
        self.build.ghost_report = None;
        self.copy_report = None;
        self.build.remote_ghosts.clear();
        self.foliage.clear();
        self.weather.clear();
        self.audio.clear();
        self.fx.effects.clear();
        self.fx.weapon_effects.reset(0);
        self.fx.actor_effects.reset(0);
        self.fx.explosion_shapes.reset(0);
        self.fx.beams.clear();
        self.fx.tutorial_targets.update(&[], 0.0);
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
        self.avatar.avatars.clear();
        self.avatar.mount_meshes.clear();
        self.avatar.avatar_actions.clear();
        self.avatar.avatar_threads.clear();
        self.avatar.avatar_action_images.clear();
        self.controls = Controls::default();
        self.scene.cpu_scene = None;
        self.lighting.light_volume = LightVolumeState::default();
        self.scene.cpu_terrain.clear();
        self.gpu.gpu_scene = None;
        self.gpu.gpu_terrain.clear();
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
        self.mounts.rider_eye = None;
        self.mounts.tumble = None;
        self.view.observer_eye = None;
        self.view.rendered_camera = None;
        self.view.rendered_roll = 0.0;
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
        if crate::recovery::left(&state_dir, &self.files.saves).is_some() {
            match crate::recovery::keep(&state_dir, &self.files.saves) {
                Ok(name) => bri_console::echo(format!(
                    "The build an earlier game left unsaved is in Load Bricks as {name}."
                )),
                Err(error) => bri_console::warn(format!(
                    "Could not keep the build an earlier game left unsaved: {error:#}"
                )),
            }
        }
        let copies = Arc::new(crate::copies::CopyFiles::new(self.files.old_saves.clone()));
        let load_limit = self.load_limit.clone();
        // v20's `$Pref::Server::Port`, 28000 unless the player changed it.
        let port = u16::try_from(self.ui.core.prefs.i64_or("$Pref::Server::Port", 28000))
            .ok()
            .filter(|p| *p != 0)
            .unwrap_or(bri_net::invite::DEFAULT_PORT);
        let port = if self.host_any_port { 0 } else { port };
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
            let (
                loaded,
                visual,
                identity,
                catalog,
                weapon_pack,
                item_bounds,
                (vehicle_pack, bot_kinds),
            ) = tokio::task::spawn_blocking(move || -> Result<_> {
                let _permit = permit;
                let weapons = paths.weapon_content()?;
                weapon_snapshot.ensure_same(&weapons)?;
                let item_physics = paths.item_physics(&weapons)?;
                physics_snapshot.ensure_same(&item_physics)?;
                let loaded =
                    paths.load_map_with_palette(&base_map, None, host_palette.as_deref())?;
                let visual = load_visual_map(&paths.map_bundle, &base_map, lighting)?;
                let mut light_volume = LightVolumeState::start(
                    &visual.scene,
                    &light_cache,
                    visual.modern_lights.as_deref(),
                );
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
                let materials = Arc::new(crate::materials::BrickMaterials::load(
                    &paths.brick_materials,
                )?);
                let palette = Arc::new(crate::world_chunks::BrickPalette::new(&materials)?);
                let mut mirror = bri_sim::prediction::CollisionMirror::new(
                    loaded.simulation.definitions.clone(),
                    loaded.query_colliders.clone(),
                    loaded.simulation.waters.clone(),
                );
                mirror.attach_terrain(loaded.terrain.clone())?;
                mirror.set_breakables(&loaded.breakables);
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
                let foliage = crate::foliage::PreparedFoliage::load(
                    &paths.foliage,
                    &base_map,
                    &building,
                    &waters,
                )?;
                Ok((
                    loaded,
                    Prepared {
                        foliage,
                        map_id: base_map.clone(),
                        waters,
                        scene: visual.scene,
                        terrain: visual.terrain.into_iter().map(Arc::new).collect(),
                        meshes,
                        mirror_shapes,
                        materials,
                        palette,
                        building,
                        mirror,
                        shape_indices: visual.shape_indices,
                        light_volume,
                    },
                    identity,
                    catalog,
                    weapons.pack,
                    item_physics.bounds,
                    (vehicle_pack, bot_kinds),
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
                    tool_catalog: catalog,
                    weapon_pack,
                    item_bounds,
                    avatar_catalog,
                    body_mounts,
                    vehicle_pack,
                    bot_kinds,
                    event_catalog,
                    event_sounds,
                },
                maps: map_list,
                copies: Some(copies),
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
            let (session, spawn_points) = setup.session(&hosted, loaded.into_session())?;
            let map_loader: server::MapLoader = setup;
            // The server's tasks spawn onto the host runtime it is started in.
            let entered = host_runtime.enter();
            let mut host = server::start_with_admin_store_and_limit(
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
                    // Joiners download the Add-Ons this host runs.
                    packages: Some(Arc::new(bri_net::packages::PackageShelf::new(
                        &paths_for_maps.root,
                        &paths_for_maps.packages,
                        &identity,
                    )?)),
                },
                max_players as usize,
                state_dir.join("administration.json"),
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
                host.open_to_internet(move |report| {
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
            saved_revision: None,
            settling: None,
            identity_changed: Default::default(),
            add_ons: Default::default(),
            joined: Default::default(),
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
        self.join_resuming(id, address, password, None)
    }
    /// [`App::join`] presenting `resume`, the lost connection's ticket, so
    /// the host gives the player back their number (and so their bricks)
    /// even before it has timed the old connection out.
    pub(super) fn join_resuming(
        &mut self,
        id: RequestId,
        address: String,
        password: String,
        resume: Option<bri_net::protocol::ResumeToken>,
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
        self.ui.apply_session(
            id,
            UiUpdate::Connection(ConnectionState::Connecting {
                text: if self.net.reconnects > 0 {
                    format!("Connection lost. Reconnecting to {typed}…")
                } else {
                    format!("Connecting to {typed}…")
                },
            }),
        );
        let pin_key = typed.clone();
        let progress = bri_progress::Progress::new();
        let reporting = progress.clone();
        // A saved server's invite carries the key it had when joined.
        let saved_invite = crate::servers::SavedServers::load(&servers_file)
            .servers
            .iter()
            .any(|s| s.invite.as_deref() == Some(address.trim()));
        let identity_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = identity_changed.clone();
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
            let joined = Client::connect_fetching_resuming(
                address,
                pin,
                player,
                local.clone(),
                resume,
                None,
                &native_identity,
                &cache,
                reporting.clone(),
                |fetched, dropped| {
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
                                },
                            )
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
                LOADABLE_MAPS.contains(&map.as_str()),
                "Server map has no supported native render bundle yet"
            );
            reporting.begin(
                bri_progress::Stage::LoadingMap,
                bri_progress::Unit::Steps,
                None,
            );
            let permit = load_limit.acquire_owned().await?;
            let visual = tokio::task::spawn_blocking(move || -> Result<Prepared> {
                let _permit = permit;
                prepare_map(&paths, &map, selected, &catalog, &light_cache, lighting)
            })
            .await??;
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
            saved_revision: None,
            settling: None,
            identity_changed,
            add_ons,
            joined: joined_list,
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
            for update in building.command_finished(request, command, result.is_ok()) {
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
    /// start, a join once the host has named its map, and a map change once
    /// the new world starts arriving. Nothing changes once in game.
    pub(super) fn show_progress(&mut self, a: &mut Attempt) {
        let snapshot = a.progress.snapshot();
        if snapshot.revision == a.progress_seen {
            return;
        }
        a.progress_seen = snapshot.revision;
        let Some(map) = a.progress.subject() else {
            return;
        };
        let showing = match &self.ui.core.conn {
            ConnectionState::Connecting { .. } | ConnectionState::Loading { .. } => true,
            // A map change: the host's new world has started to arrive.
            ConnectionState::InGame { .. } => {
                a.reloading
                    || (snapshot.stage == bri_progress::Stage::ReceivingWorld
                        && self.scene.scene_map.as_deref() != Some(map.as_str()))
            }
            _ => false,
        };
        if !showing
            || (a.entered && !a.reloading && snapshot.stage != bri_progress::Stage::ReceivingWorld)
        {
            return;
        }
        let preview = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map)
            .map_or(IconRef::None, |m| m.preview.clone());
        self.ui.apply_session(
            a.id,
            UiUpdate::Connection(ConnectionState::Loading {
                map,
                preview,
                status: snapshot.status(),
                progress: snapshot.fraction(),
            }),
        );
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
