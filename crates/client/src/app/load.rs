//! Building the App: content, audio and every system's start state.
use super::*;

impl App {
    pub fn load(content_root: &Path, state_dir: &Path, size: (u32, u32)) -> Result<Self> {
        Self::load_with_audio(content_root, state_dir, size, bri_audio::OutputKind::Null)
    }
    pub fn load_with_audio(
        content_root: &Path,
        state_dir: &Path,
        size: (u32, u32),
        output: bri_audio::OutputKind,
    ) -> Result<Self> {
        // Resolve once so state (including identity and host administration)
        // cannot silently switch when the process working directory changes.
        let absolute_state_dir = std::path::absolute(state_dir)?;
        let state_dir = absolute_state_dir.as_path();
        let requested = bri_package::packages::PackageSet::load_root(content_root)?;
        let (loaded, mut content_problems) = crate::add_on_health::collecting(|| {
            ClientContent::load_leaving_out_broken(content_root, &requested)
        });
        let (mut content, left_out) = loaded?;
        let old_saves = crate::old_saves::OldSaves::new(
            state_dir.join("saves"),
            state_dir.join("converted-saves"),
        );
        let mut rule_problems = Vec::new();
        let package_catalog = {
            let (catalog, problems) = crate::packages::load(&content.paths.root);
            rule_problems.extend(problems);
            catalog
        };
        let client_code = crate::client_code::ClientCode::load(
            &content.paths.root,
            &bri_package::packages::PackageSet::load_root(&content.paths.root)
                .unwrap_or_else(|_| bri_package::packages::PackageSet::base()),
        );
        // Worlds that packages provide are hosted like maps.
        let server_packages = {
            let (catalog, problems) = crate::packages::load_server(&content.paths.root);
            rule_problems.extend(problems);
            catalog
        };
        if let Some(catalog) = &server_packages {
            let worlds = crate::packages::world_maps(catalog, &content.maps);
            content.maps.extend(worlds);
        }
        let foliage = crate::foliage::ClientFoliage::load(&content.paths.foliage)?;
        let effects_pack = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
        let effects = crate::effects::WorldEffects::new(effects_pack.clone(), Default::default())?;
        let mut weapon_shells = crate::weapon_debris::WeaponDebris::new(
            crate::weapon_debris::WeaponDebrisAssets::load(&content.paths.weapon_debris)?,
            Default::default(),
        )?;
        // Without its models the practice still runs and completes on
        // schedule; only the targets go undrawn.
        let tutorial_targets = crate::tutorial_targets::TutorialTargets::load(&content.paths.tutorial)
            .unwrap_or_else(|error| {
                eprintln!("Tutorial targets will not be drawn: {error:#}");
                Default::default()
            });
        let ContentParts {
            weapon_effects,
            actor_effects,
            explosion_shapes,
            explosion_debris,
            tool_ui,
            item_assets,
            item_ui,
            vehicle_assets,
            world_items,
        } = {
            let (parts, more) = crate::add_on_health::collecting(|| {
                ContentParts::build(&content, effects_pack, &state_dir.join(ITEM_ICONS))
            });
            content_problems.extend(more);
            parts?
        };
        content_problems.extend(
            weapon_shells.set_casings(&content.weapons.pack, |m| world_items.has_model(m)),
        );
        let mut avatar_assets = crate::avatar::AvatarAssets::load(&content.paths.avatar)?;
        avatar_assets.load_horse(&content.paths.vehicles)?;
        let avatar_assets = Arc::new(avatar_assets);
        let settings::Recovered {
            settings: mut saved,
            notice: settings_notice,
        } = settings::recover(&state_dir.join("settings.json"));
        let weather = crate::weather::ClientWeather::load(&content.paths.weather, &mut saved)?;
        let graphics = crate::graphics::Graphics::from_settings(&saved);
        let mut audio = crate::audio::ClientAudio::load(&content.paths.audio, &mut saved, output)?;
        audio.set_pack_sounds(&content.weapons.pack, &content.paths.weapons);
        let platform = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Windows
        };
        let mut ui = Ui::new(
            content.ui_pack.clone(),
            UiConfig {
                size,
                scale: None,
                platform,
            },
            saved,
        );
        ui.set_console_commands(crate::console::commands());
        if let Some(text) = settings_notice {
            ui.apply(UiUpdate::MessageBox {
                title: "Settings Problem".into(),
                text,
            });
        }
        ui.apply(UiUpdate::Maps(content.maps.clone()));
        ui.core.music_tracks = content.music.iter().map(|(_, name)| name.clone()).collect();
        ui.apply(UiUpdate::GameModes(crate::packages::modes(
            server_packages.as_ref(),
        )));
        let backgrounds = content
            .ui_pack
            .data
            .images
            .keys()
            .filter(|key| key.to_ascii_lowercase().starts_with("screenshots/"))
            .cloned()
            .map(IconRef::Pack)
            .collect();
        ui.apply(UiUpdate::MainMenuBackgrounds(backgrounds));
        let frame_limit = settings::startup_display(&ui.settings()).max_fps;
        ui.apply(UiUpdate::Version(crate::updates::version()));
        let mut app = Self {
            item_assets,
            item_ui,
            world_items,
            foliage,
            weather,
            weather_renderer: None,
            audio,
            ui,
            saves: crate::saves::Store::new(state_dir, &content, Some(old_saves.clone())),
            old_saves,
            old_saves_started: false,
            save_refresh: None,
            file_jobs: Default::default(),
            color_load: None,
            closing: Vec::new(),
            close_asked: None,
            content,
            controls: Controls::default(),
            state_dir: state_dir.into(),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            host_runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("bri-host")
                .enable_all()
                .build()?,
            attempt: None,
            abilities: Default::default(),
            brick_hand: None,
            ghost_report: None,
            copy_report: None,
            remote_ghosts: BTreeMap::new(),
            cpu_scene: None,
            steering_sent: None,
            crosshair_hidden: false,
            tool_wheel: None,
            tool_takes_paint: false,
            camera_wheel: false,
            aim_wheel: false,
            scope_overlay: None,
            cpu_terrain: Vec::new(),
            renderer: None,
            effects,
            weapon_effects,
            actor_effects,
            explosion_shapes,
            beams: Default::default(),
            tutorial_targets,
            explosion_debris,
            cosmetic_faults: Default::default(),
            weapon_shells,
            shell_gpu: None,
            weapon_cues: VecDeque::new(),
            weapon_cue_drops: 0,
            brick_debris: Default::default(),
            debris_models: Default::default(),
            brick_fades: Default::default(),
            fade_models: Default::default(),
            chunks_left_out: BTreeSet::new(),
            package_catalog,
            client_code,
            item_skins: Default::default(),
            server_packages,
            packages_from_tools: false,
            skip_add_on_reload: false,
            join_notices: Vec::new(),
            package_models: Default::default(),
            brick_kills: Vec::new(),
            hidden_lines: None,
            vignette: None,
            environment_sent: None,
            selection_lines: None,
            selection_uploaded: None,
            world_shapes: None,
            shapes_uploaded: None,
            hidden_uploaded: None,
            hidden_fading: Vec::new(),
            weapon_light_deferred: 0,
            weapon_effect_session: None,
            weapon_animation_cues: VecDeque::new(),
            weapon_animation_drops: 0,
            weapon_animation_cursor: 0,
            shot_kicks: Vec::new(),
            kick_seen: None,
            effects_renderer: None,
            gpu_scene: None,
            light_volume: LightVolumeState::default(),
            shape_indices: BTreeMap::new(),
            gpu_broken: BTreeSet::new(),
            gpu_terrain: Vec::new(),
            depth: None,
            meshes: None,
            mirror_shapes: Default::default(),
            mirror_index: Default::default(),
            reflections: None,
            environment_probe: None,
            palette: None,
            gpu_palette: None,
            chunked: Default::default(),
            cpu_chunks: HashMap::new(),
            cpu_chunk_bricks: HashMap::new(),
            gpu_chunks: HashMap::new(),
            gpu_chunk_bricks: HashMap::new(),
            chunk_hides: BTreeMap::new(),
            liquid_cache: None,
            chunk_uploads: BTreeSet::new(),
            world_source: None,
            world_revision: 0,
            world_log: None,
            world_job: None,
            graphics,
            load_limit: Arc::new(tokio::sync::Semaphore::new(2)),
            host_any_port: false,
            gpu_restart: false,
            scene_map: None,
            materials: None,
            building: None,
            pending_actions: BTreeMap::new(),
            tool_ui,
            dialog_epoch: 0,
            trigger_epoch: None,
            query_source: None,
            query_log: None,
            ghost_gpu: None,
            ghost_look: None,
            ghost_uploaded: u64::MAX,
            avatar_assets,
            avatars: BTreeMap::new(),
            mount_meshes: BTreeMap::new(),
            avatar_actions: BTreeMap::new(),
            avatar_threads: BTreeMap::new(),
            avatar_action_images: BTreeMap::new(),
            animation_time: 0.0,
            avatar_preview: None,
            preview_request: None,
            preview_dirty: false,
            save_pictures: HashMap::new(),
            save_previews: Default::default(),
            splash_checked: false,
            save_picture: None,
            save_shots: Default::default(),
            motion: Default::default(),
            ghosts: Default::default(),
            shot_origins: Default::default(),
            vehicle_assets,
            vehicles: Default::default(),
            mount_heading: None,
            seated_on: None,
            takes_turret: false,
            seat_report: None,
            rider_rotations: BTreeMap::new(),
            rider_eye: None,
            observer_eye: None,
            rendered_camera: None,
            rendered_roll: 0.0,
            drawn_controls: None,
            tumble: None,
            music_world: None,
            net_sampler: Default::default(),
            lag_watch: Default::default(),
            perf_stats_due: std::time::Instant::now(),
            gpu_name: String::new(),
            gpu_passes: Vec::new(),
            time_passes: false,
            frame_stats: Default::default(),
            frame_log: None,
            update_check: None,
            auto_quality: false,
            lan_hosts: BTreeMap::new(),
            reconnects: 0,
            lan_query: None,
            add_on_sync: None,
            left_out_add_ons: None,
            content_problems,
            add_on_health: Default::default(),
            invite: None,
            firewall_fix: None,
            frame_limit,
            macro_recording: None,
            build_macro: Vec::new(),
            macro_playback: VecDeque::new(),
            combat: Default::default(),
        };
        if !left_out.is_empty() {
            // Catalogs, rules and client code follow the list that loaded.
            let loaded = app.content.paths.packages.clone();
            app.left_out_add_ons = Some((requested.clone(), loaded, left_out.clone()));
            if let Err(error) = app.apply_packages(&requested) {
                bri_console::warn(format!("Add-Ons not applied: {error:#}"));
                app.check_add_ons(&rule_problems);
            }
            app.notify_left_out_add_ons(&left_out);
        } else {
            app.check_add_ons(&rule_problems);
        }
        Ok(app)
    }
}
