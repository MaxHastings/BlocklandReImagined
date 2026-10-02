//! Draining the network worker's events each frame.
use super::*;

impl App {
    pub(super) fn poll_network(&mut self) -> Result<()> {
        let Some(mut a) = self.attempt.take() else {
            return Ok(());
        };
        if self.ui.session_request() != Some(a.id) {
            self.disconnect();
            return Ok(());
        }
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
            // A join knows only the typed address until the host names
            // itself; hosting keeps the name and size it was started with.
            if !a.local
                && let Some(view) = &a.view
            {
                (a.name, a.max_players) = joined_server(&view.listing, &a.name, a.max_players);
            }
        }
        self.show_progress(&mut a);
        let mut failed = None;
        while let Ok(event) = a.worker.events.try_recv() {
            match event {
                network::Event::Presentation { cues, dropped } => {
                    self.audio.server_dropped = dropped;
                    // The attempt is out of `self` here, so ask it directly
                    // where the local player hears from.
                    let view = a.view.as_ref().filter(|_| a.entered);
                    let heard_at = listener(&self.motion, view);
                    for cue in cues {
                        self.queue_cue_heard_at(cue, heard_at);
                    }
                }
                network::Event::Ready => a.ready = true,
                network::Event::MapChanged(map) => {
                    a.saved_revision = None;
                    // Movement limits belong to the old map's Tutorial; the
                    // new map sends its own if it has any.
                    self.abilities = Default::default();
                    a.settling = Some(std::time::Instant::now() + SETTLE);
                    // Load the new map's scene and prediction world; the old
                    // scene stays until it is ready.
                    let paths = self.content.paths.clone();
                    let light_cache = self.state_dir.join("light-volumes");
                    let selected = self.content.selectable.clone();
                    let catalog = self.tool_ui.server_catalog();
                    let load_limit = self.load_limit.clone();
                    let (scene_tx, scene) = mpsc::sync_channel(1);
                    a.scene = scene;
                    self.world_items.reset();
                    self.brick_debris.clear();
                    let (failed_tx, failed_rx) = mpsc::sync_channel(1);
                    a.map_failure = Some(failed_rx);
                    // v20 shows the loading GUI while the new mission loads.
                    a.reloading = true;
                    a.progress.begin(
                        bri_progress::Stage::LoadingMap,
                        bri_progress::Unit::Steps,
                        None,
                    );
                    self.ui.apply_session(
                        a.id,
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
                    self.runtime.spawn(async move {
                        let prepared = async {
                            let permit = load_limit.acquire_owned().await?;
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                prepare_map(&paths, &map, selected, &catalog, &light_cache)
                            })
                            .await?
                        }
                        .await;
                        match prepared {
                            Ok(prepared) => {
                                let _ = scene_tx.send(prepared);
                            }
                            Err(error) => {
                                let _ = failed_tx.send(format!("{error:#}"));
                            }
                        }
                    });
                }
                network::Event::Notice(bri_sim::session::Notice::Inspected {
                    brick_id,
                    brick,
                    mode,
                }) => {
                    // A wrench/printer hit opens its dialog only over plain
                    // play, never over a newer modal or while typing, and
                    // only for a click nothing has cancelled since.
                    if self.ui.stack() != [ScreenId::Play]
                        || self.trigger_epoch != Some(self.dialog_epoch)
                    {
                        continue;
                    }
                    let Some(view) = a.view.as_ref() else {
                        continue;
                    };
                    self.invalidate_tool_dialogs();
                    let reply = Reply::Inspected {
                        brick_id,
                        brick,
                        mode,
                    };
                    match self.tool_ui.accept_inspection(
                        &reply,
                        mode,
                        None,
                        &view.world,
                        &view.names,
                        view.owner,
                    ) {
                        Ok(updates) => {
                            for update in updates {
                                self.ui.apply_session(a.id, update);
                            }
                        }
                        Err(error) => {
                            self.ui.apply_session(
                                a.id,
                                UiUpdate::CenterPrint {
                                    text: format!("{error:#}"),
                                    seconds: 2.0,
                                },
                            );
                        }
                    }
                }
                network::Event::Notice(notice) => {
                    let update = match notice {
                        bri_sim::session::Notice::Chat(text) => UiUpdate::Chat {
                            text: bri_ui::ml::sanitize(&text),
                        },
                        bri_sim::session::Notice::Center { text, seconds } => {
                            UiUpdate::CenterPrint {
                                text: print_markup(&self.ui.core.binds, &text),
                                seconds,
                            }
                        }
                        bri_sim::session::Notice::Bottom {
                            text,
                            seconds,
                            hide_bar,
                        } => UiUpdate::BottomPrint {
                            text: print_markup(&self.ui.core.binds, &text),
                            seconds,
                            hide_bar,
                        },
                        bri_sim::session::Notice::Abilities(abilities) => {
                            self.abilities = abilities;
                            continue;
                        }
                        bri_sim::session::Notice::MusicTracks(music) => {
                            self.tool_ui.offer_music(&music);
                            continue;
                        }
                        bri_sim::session::Notice::TempBrickColor(color) => {
                            if let Some(building) = self.building.as_mut() {
                                building.set_random_color(color);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::Sound(profile) => {
                            self.audio.profile(&profile, bri_audio::Placement::Listener);
                            continue;
                        }
                        bri_sim::session::Notice::Fov(fov) => {
                            self.controls.set_server_fov(fov);
                            continue;
                        }
                        bri_sim::session::Notice::PlantError(failure) => {
                            UiUpdate::PlantError(plant_error(failure))
                        }
                        bri_sim::session::Notice::Invite {
                            game,
                            owner_name,
                            title,
                        } => UiUpdate::MiniGameInvite(MiniGameInvitation {
                            game: MiniGameId(game),
                            title: plain_chat(&title),
                            owner: MiniGamePlayerId(0),
                            owner_name: plain_chat(&owner_name),
                            owner_display_id: String::new(),
                        }),
                        bri_sim::session::Notice::MessageBox { title, text } => {
                            UiUpdate::MessageBox {
                                title: plain_chat(&title),
                                text: plain_chat(&text),
                            }
                        }
                        bri_sim::session::Notice::Question {
                            title,
                            text,
                            package,
                            command,
                        } => UiUpdate::Confirm {
                            title: plain_chat(&title),
                            text: plain_chat(&text),
                            action: Box::new(UiAction::Game(GameAction::Package {
                                package,
                                command,
                                pressed: None,
                            })),
                        },
                        bri_sim::session::Notice::TrustInvite {
                            from,
                            name,
                            principal,
                            level,
                        } => {
                            // `clientCmdTrustInvite` refuses ignored players.
                            if a.trust.get(&from).is_some_and(|t| t.ignoring) {
                                continue;
                            }
                            UiUpdate::TrustInvite(TrustInvitation {
                                from,
                                name: plain_chat(&name),
                                bl_id: crate::trust_list::display_id(&principal).to_string(),
                                level,
                            })
                        }
                        bri_sim::session::Notice::TrustSaved {
                            principal,
                            level,
                            name,
                        } => {
                            let path = self.state_dir.join("trust-list.json");
                            let saved = crate::trust_list::TrustList::load(&path).update(
                                &principal,
                                level,
                                &plain_chat(&name),
                            );
                            if let Err(error) = saved {
                                UiUpdate::Chat {
                                    text: plain_chat(&format!("{error:#}")),
                                }
                            } else {
                                continue;
                            }
                        }
                        bri_sim::session::Notice::PlayerTrust(rows) => {
                            a.trust = rows;
                            continue;
                        }
                        bri_sim::session::Notice::Blueprint(blueprint) => {
                            if let Some(building) = self.building.as_mut()
                                && let Err(error) = building.set_blueprint(blueprint.map(|b| *b))
                            {
                                bri_console::echo(format!("Copied build ignored: {error:#}"));
                            }
                            continue;
                        }
                        bri_sim::session::Notice::PutAway => {
                            if let Some(building) = self.building.as_mut() {
                                for update in building.put_away() {
                                    self.ui.apply_session(a.id, update);
                                }
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MirrorCopy { across_z } => {
                            if let Some(building) = self.building.as_mut() {
                                building.mirror_copy(across_z);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MirrorGhost {
                            definition,
                            quarter_turns,
                        } => {
                            if let Some(building) = self.building.as_mut() {
                                building.mirror_ghost(&definition, quarter_turns);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MoveCopy { point, normal } => {
                            if let Some(building) = self.building.as_mut() {
                                building.move_copy(point, normal);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::FlipCopy => {
                            if let Some(building) = self.building.as_mut() {
                                building.flip_copy();
                            }
                            continue;
                        }
                        bri_sim::session::Notice::PivotCopy { whole } => {
                            if let Some(building) = self.building.as_mut() {
                                building.pivot_copy(whole);
                            }
                            continue;
                        }
                        // The host's Add-On pressed a brick key for the
                        // player: it goes through as theirs would, moving
                        // the copy they now hold.
                        bri_sim::session::Notice::ShiftCopy {
                            offset: [x, y, z],
                            super_shift,
                        } => {
                            self.ui.core.request(UiAction::Game(if super_shift {
                                GameAction::SuperShiftBrick { x, y, z }
                            } else {
                                GameAction::ShiftBrick { x, y, z }
                            }));
                            continue;
                        }
                        bri_sim::session::Notice::RotateCopy { direction } => {
                            self.ui.core.request(UiAction::Game(GameAction::RotateBrick {
                                dir: i32::from(direction),
                            }));
                            continue;
                        }
                        bri_sim::session::Notice::PlantCopy => {
                            self.ui.core.request(UiAction::Game(GameAction::PlantBrick));
                            continue;
                        }
                        bri_sim::session::Notice::WrenchCopy { bricks } => {
                            UiUpdate::OpenFillWrench { bricks }
                        }
                        bri_sim::session::Notice::TakePaint(take) => {
                            if let Some(building) = self.building.as_mut() {
                                building.set_paint_taken(take);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::ScrollMode(mode) => {
                            use bri_package_runtime::ops::ScrollMode as Host;
                            use bri_ui::models::hud::ScrollMode as Hud;
                            UiUpdate::ScrollMode(match mode {
                                Host::None => Hud::None,
                                Host::Bricks => Hud::Bricks,
                                Host::Paint => Hud::Paint,
                                Host::Tools => Hud::Tools,
                            })
                        }
                        bri_sim::session::Notice::SelectionBox(outline) => {
                            if let Some(building) = self.building.as_mut()
                                && let Err(error) = building.set_outline(outline.map(|o| *o))
                            {
                                bri_console::echo(format!("Selection box ignored: {error:#}"));
                            }
                            continue;
                        }
                        bri_sim::session::Notice::Report(report) => {
                            let palette = a.view.as_ref().map_or(&[][..], |v| &v.world.palette[..]);
                            UiUpdate::Report(report.map(|r| report_view(&r, palette)))
                        }
                        bri_sim::session::Notice::Inspected { .. } => unreachable!(),
                    };
                    self.ui.apply_session(a.id, update);
                }
                network::Event::Reply { request, result } => {
                    self.accept_reply(&a, request, result);
                }
                network::Event::Failed(reason) => {
                    failed = Some(reason);
                    break;
                }
            }
        }
        if failed.is_none() && a.worker.events.is_closed() {
            failed = Some("Connection worker stopped".into());
        }
        if let Some(mut reason) = failed {
            // A joined remote game whose network dropped is rejoined
            // automatically a few times; the host gives the player their
            // owner number, and so their bricks, back.
            let id = a.id;
            let rejoin =
                (!a.local && a.entered && reason.contains(bri_net::client::CONNECTION_LOST))
                    .then(|| a.name.clone());
            if let Some(address) = rejoin
                && self.reconnects < MAX_RECONNECTS
            {
                self.reconnects += 1;
                if self.join(id, address, String::new()).is_ok() {
                    return Ok(());
                }
            }
            self.reconnects = 0;
            // The server's Add-Ons bring content: load it and join again
            // (the downloads are cached, so this join fetches nothing).
            let add_ons = a.add_ons.lock().ok().and_then(|mut slot| slot.take());
            if let Some(set) = add_ons {
                self.disconnect();
                let applied = self.apply_packages(&set);
                // The next game this player hosts runs their own list again.
                self.packages_from_tools = false;
                // Add-Ons that do not load here are joined without: the
                // player is told which, in chat, once in the game.
                if let Err(error) = applied {
                    let text = format!(
                        "Some of this server's Add-Ons could not be loaded on this computer, so you joined without them: {error:#}"
                    );
                    bri_console::warn(&text);
                    self.join_notices.push(text);
                    self.skip_add_on_reload = true;
                }
                match self.join(id, a.name.clone(), String::new()) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        self.join_notices.clear();
                        reason =
                            format!("Could not join again with the server's Add-Ons: {error:#}");
                        bri_console::warn(&reason);
                    }
                }
            }
            if a.identity_changed
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                let question = self.identity_question(&a.name);
                self.ui
                    .apply_session(id, UiUpdate::FailureQuestion(question));
            }
            if let Some(mismatch) = crate::add_ons::mismatch(&self.content.paths.root, &reason) {
                self.ui.apply_session(id, UiUpdate::AddOnMismatch(mismatch));
            }
            self.ui
                .apply_session(id, UiUpdate::Connection(ConnectionState::Failed { reason }));
            self.disconnect();
            return Ok(());
        }
        if let Some(reason) = a.map_failure.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::Failed {
                    reason: format!("Could not load the new map: {reason}"),
                }),
            );
            self.disconnect();
            return Ok(());
        }
        if let Ok(prepared) = a.scene.try_recv() {
            a.progress.begin(
                bri_progress::Stage::LoadingGraphics,
                bri_progress::Unit::Steps,
                None,
            );
            if self.cpu_scene.is_some() {
                // A map change: renderers keep per-map sky and terrain state,
                // so rebuild them for the new map like a fresh join.
                self.gpu_stopped();
                self.gpu_restart = true;
            }
            self.scene_map = Some(prepared.map_id.clone());
            self.foliage.set_map(prepared.foliage);
            self.weather.set_map(&prepared.map_id, prepared.waters)?;
            self.light_volume = prepared.light_volume;
            self.cpu_scene = Some(prepared.scene);
            self.shape_indices = prepared.shape_indices;
            self.cpu_terrain = prepared.terrain;
            self.meshes = Some(prepared.meshes);
            self.mirror_shapes = prepared.mirror_shapes;
            self.mirror_index.clear();
            self.materials = Some(prepared.materials);
            self.palette = Some(prepared.palette);
            self.gpu_palette = None;
            let old = self.building.replace(prepared.building);
            self.motion.install(prepared.mirror);
            let building = self.building.as_mut().unwrap();
            building.set_tool_catalog(self.item_ui.catalog())?;
            if let Some(old) = &old {
                building.carry_over(old);
            }
            self.gpu_scene = None;
            self.gpu_terrain.clear();
            // A map change replaced what first entry set the HUD up from.
            if a.entered
                && let (Some(scene), Some(view)) = (&self.cpu_scene, &a.view)
            {
                for update in self.map_setup_updates(scene, &view.world.palette)? {
                    self.ui.apply_session(a.id, update);
                }
            }
        }
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
        }
        // The server's Add-Ons' wrench events join the wrench's lists.
        if let Some(view) = &a.view
            && let Some(update) = self
                .tool_ui
                .offer_events(&view.brick_events)
            && a.entered
        {
            self.ui.apply_session(a.id, update);
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view) {
            building.set_held_brick(view.weapons.images.get(&view.owner).is_some_and(|images| {
                images.iter().any(|image| {
                    image.hand == 0
                        && bri_sim::session::BRICK_HAND_IMAGES.contains(&image.image.as_str())
                })
            }));
            building.set_held_image(view.weapons.images.get(&view.owner).is_some_and(|images| {
                images.iter().any(|image| {
                    image.hand == 0
                        && !bri_sim::session::BRICK_HAND_IMAGES.contains(&image.image.as_str())
                })
            }));
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view)
            && let Some(inventory) = view.tools.get(&view.owner)
        {
            match building.sync_tools(inventory) {
                Ok(updates) => {
                    for update in updates {
                        self.ui.apply_session(a.id, update);
                    }
                }
                Err(error) => {
                    self.ui.apply_session(
                        a.id,
                        UiUpdate::Connection(ConnectionState::Failed {
                            reason: format!("Native tool inventory: {error:#}"),
                        }),
                    );
                    self.disconnect();
                    return Ok(());
                }
            }
        }
        if a.entered
            && let Some(building) = &self.building
        {
            let hand = bri_sim::session::BrickHand {
                stocked: building.inventory().iter().any(Option::is_some),
                equipped: matches!(building.equipment(), crate::building::Equipment::Brick(_)),
                ghost: building.ghost().is_some(),
            };
            // A full request queue leaves the report pending for the next frame.
            if self.brick_hand != Some(hand)
                && a.worker
                    .request(REPORT_REQUEST, Command::BrickHand(hand))
                    .is_ok()
            {
                self.brick_hand = Some(hand);
            }
            // Others see the ghost too (v20 ghosted `tempBrick`). Moves are
            // sent at most ten times a second; putting it away goes at once.
            let ghost = building.ghost().and_then(|ghost| {
                let id = |r: &bri_world::ContentRef| match r {
                    bri_world::ContentRef::Resolved(id) => Some(id.clone()),
                    _ => None,
                };
                Some(bri_sim::session::GhostBrick {
                    definition: id(&ghost.definition)?,
                    position: ghost.position,
                    quarter_turns: ghost.quarter_turns,
                    color: ghost.color,
                    print: ghost.print.as_ref().and_then(id),
                })
            });
            let due = self.ghost_report.as_ref().is_none_or(|(sent, at)| {
                *sent != ghost && (ghost.is_none() || at.elapsed() >= GHOST_REPORT_INTERVAL)
            });
            if due
                && a.worker
                    .request(REPORT_REQUEST, Command::GhostBrick(ghost.clone()))
                    .is_ok()
            {
                self.ghost_report = Some((ghost, std::time::Instant::now()));
            }
            // Where the copy in hand stands, for its Add-On to show the
            // others; at the same pace.
            let copy = building.copy_report();
            let due = self.copy_report.as_ref().is_none_or(|(sent, at)| {
                *sent != copy && (copy.is_none() || at.elapsed() >= GHOST_REPORT_INTERVAL)
            }) && (copy.is_some() || self.copy_report.is_some());
            if due
                && a.worker
                    .request(
                        REPORT_REQUEST,
                        Command::CopyPose(copy.map(|(_, pose)| pose)),
                    )
                    .is_ok()
            {
                self.copy_report = Some((copy, std::time::Instant::now()));
            }
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view) {
            building.set_broken_shapes(&view.broken_shapes)?;
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view)
            && self
                .query_source
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
        {
            let known = self
                .query_log
                .as_ref()
                .filter(|(log, _)| self.query_source.is_some() && Arc::ptr_eq(log, &view.world_log))
                .and_then(|(log, revision)| log.between(*revision, view.world_revision));
            if let Err(error) = building.sync_world_changes(&view.world, known.as_ref()) {
                self.ui.apply_session(
                    a.id,
                    UiUpdate::Connection(ConnectionState::Failed {
                        reason: format!("Native query mirror: {error:#}"),
                    }),
                );
                self.disconnect();
                return Ok(());
            }
            if a.entered
                && self
                    .query_source
                    .as_ref()
                    .is_none_or(|old| old.palette != view.world.palette)
            {
                let colors = self.colorset(&view.world.palette);
                self.ui.apply_session(a.id, UiUpdate::Colorset(colors));
            }
            self.query_source = Some(view.world.clone());
            self.query_log = Some((view.world_log.clone(), view.world_revision));
            self.ghost_uploaded = u64::MAX;
            self.brick_debris.sync_world(&view.world);
            self.hidden_uploaded = None;
        }
        if let Some(view) = &a.view {
            self.mirror_index.follow(
                &view.world,
                &view.world_log,
                view.world_revision,
                &self.mirror_shapes,
            );
            // One set of openings: the windows show where bodies go.
            if let Some(collision) = self.motion.collision() {
                self.mirror_index.link(collision.links(), &self.mirror_shapes);
            }
        }
        if let Some(job) = &mut self.world_job
            && let Ok((source, revision, log, result)) = job.receiver.try_recv()
        {
            let left_out = std::mem::take(&mut job.left_out);
            self.world_job = None;
            match result {
                // Always applied: chunk state is consistent with `source`, and
                // a newer replica is reached by the next incremental update.
                Ok((chunked, changes)) => {
                    self.chunked = chunked;
                    for (key, built) in changes {
                        if let Some(built) = built {
                            self.cpu_chunks.insert(key, built.scene);
                            self.cpu_chunk_bricks.insert(key, Arc::new(built.bricks));
                            self.chunk_uploads.insert(key);
                        } else {
                            self.cpu_chunks.remove(&key);
                            self.cpu_chunk_bricks.remove(&key);
                            self.gpu_chunks.remove(&key);
                            self.gpu_chunk_bricks.remove(&key);
                            self.chunk_uploads.remove(&key);
                        }
                    }
                    self.world_source = Some(source);
                    self.world_revision = revision;
                    self.world_log = Some(log);
                    self.brick_fades.chunks_applied(&left_out);
                    self.chunks_left_out = left_out;
                }
                Err(reason) => {
                    self.ui.apply_session(
                        a.id,
                        UiUpdate::Connection(ConnectionState::Failed { reason }),
                    );
                    self.disconnect();
                    return Ok(());
                }
            }
        }
        if self.world_job.is_none()
            && let (Some(meshes), Some(materials), Some(palette), Some(view)) =
                (&self.meshes, &self.materials, &self.palette, &a.view)
            && (self
                .world_source
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &view.world))
                || self.brick_fades.needs_rebuild(&self.chunks_left_out))
        {
            let meshes = meshes.clone();
            let materials = materials.clone();
            let palette = palette.clone();
            let world = view.world.clone();
            let (revision, log) = (view.world_revision, view.world_log.clone());
            // Compare only the bricks the replica reports changed since the
            // applied revision; without that history, compare whole worlds.
            let known = self
                .world_log
                .as_ref()
                .filter(|applied| Arc::ptr_eq(applied, &log))
                .and_then(|log| log.between(self.world_revision, revision));
            // v20 eases repainted bricks to their new colour (`brick_fade`).
            match (&self.world_source, &known) {
                (Some(drawn), Some(known)) if !known.palette => {
                    self.brick_fades
                        .observe(drawn, &world, known.bricks.iter().copied());
                    // A knocked-out brick does not fade out in place: its
                    // debris replaces it at once. Easing it would draw it
                    // twice and cost a model per brick plus a second
                    // chunk rebuild once the fades settle.
                    let killing = self.pending_kills();
                    for id in &known.bricks {
                        if self.brick_debris.is_dead(*id) || killing.contains(id) {
                            self.brick_fades.settle(*id);
                        }
                    }
                }
                _ => self.brick_fades.settle_all(),
            }
            let left_out = self.brick_fades.left_out();
            let job_left_out = left_out.clone();
            let mut chunked = std::mem::take(&mut self.chunked);
            let (send, receive) = mpsc::sync_channel(1);
            let load_limit = self.load_limit.clone();
            let task = self.runtime.spawn(async move {
                let Ok(permit) = load_limit.acquire_owned().await else {
                    return;
                };
                let source = world.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    chunked
                        .update_leaving_out(
                            world,
                            known.as_ref(),
                            &left_out,
                            &meshes,
                            &palette,
                            Some(&materials),
                            WORLD_TRIANGLE_BUDGET,
                        )
                        .map(|changes| (chunked, changes))
                        .map_err(|e| format!("{e:#}"))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
                let _ = send.send((source, revision, log, result));
            });
            self.world_job = Some(WorldJob {
                receiver: receive,
                abort: task.abort_handle(),
                left_out: job_left_out,
            });
        }
        if a.reloading
            && let Some(view) = &a.view
            && self.scene_map.as_deref() == Some(view.world.map_id.as_str())
            && self
                .world_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &view.world))
        {
            a.reloading = false;
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::InGame {
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                    local: a.local,
                    single_player: a.single,
                    admin: view.administrator,
                }),
            );
        }
        if a.ready
            && !a.entered
            && self.world_source.is_some()
            && let Some(scene) = &self.cpu_scene
        {
            self.ui.apply_session(
                a.id,
                UiUpdate::ActionResult {
                    id: a.id,
                    result: Ok(()),
                },
            );
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::InGame {
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                    local: a.local,
                    single_player: a.single,
                    admin: a.view.as_ref().is_some_and(|v| v.administrator),
                }),
            );
            let palette = &a
                .view
                .as_ref()
                .context("Ready connection has no world")?
                .world
                .palette;
            for update in self.map_setup_updates(scene, palette)? {
                self.ui.apply_session(a.id, update);
            }
            a.entered = true;
            self.reconnects = 0;
            for text in std::mem::take(&mut self.join_notices) {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
            // Admins hear once per game what this computer's Add-Ons lack.
            if a.view.as_ref().is_some_and(|v| v.administrator)
                && let Some(text) = self.add_on_health.summary()
            {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
            // A game this player hosts runs their own Add-Ons' code; someone
            // else's server runs only code the player trusted for its host
            // key (never its address, which another host can take over).
            let server = a
                .view
                .as_ref()
                .map_or_else(String::new, |v| v.host_key.clone());
            // The code of the Add-Ons this game runs: the server's list when
            // joining changed it, else this client's own.
            let set = (!a.local)
                .then(|| a.joined.lock().ok().and_then(|mut slot| slot.take()))
                .flatten()
                .unwrap_or_else(|| self.content.paths.packages.clone());
            if self.client_code.loaded_from() != Some(&set) {
                self.client_code =
                    crate::client_code::ClientCode::load(&self.content.paths.root, &set);
            }
            self.client_code.start(
                if a.local {
                    crate::client_code::Host::Local
                } else {
                    crate::client_code::Host::Remote(&server)
                },
                &self.state_dir,
            );
            // Code the player has not trusted on this server yet: ask before
            // any of it runs. Leave ends the game.
            if !a.local
                && let Some(prompt) =
                    self.client_code
                        .trust_prompt(&server, &plain_chat(&a.name), &self.state_dir)
            {
                self.ui
                    .apply_session(a.id, UiUpdate::Question(trust_question(&prompt)));
            }
            if let Some(view) = &a.view {
                self.reset_weapon_effect_session(a.id, view.checkpoint_cue_cursor);
            }
            // `handleYourSpawn`: no favorites auto-buy in a local Tutorial,
            // whose lessons hand out the bricks.
            let tutorial = a.local
                && a.view.as_ref().is_some_and(|v| {
                    v.world
                        .map_id
                        .eq_ignore_ascii_case(bri_sim::tutorial::MAP_ID)
                });
            if !tutorial {
                self.ui.apply_session(a.id, UiUpdate::FirstSpawn);
            }
            self.ui
                .core
                .request(UiAction::SetAvatar(self.ui.settings().avatar));
            // `clientCmdTrustListUpload_Start`; the reply needs no handling.
            let list = crate::trust_list::TrustList::load(&self.state_dir.join("trust-list.json"));
            a.worker
                .request(REPORT_REQUEST, Command::TrustList(list.entries()))?;
        }
        if let Some(view) = &a.view {
            // The Environment window's view: on every change, and each
            // second while a day/night cycle turns.
            if let Some(scene) = &self.cpu_scene {
                let next = bri_ui::models::environment::EnvironmentView {
                    authored: authored_environment(scene),
                    settings: view.environment.clone(),
                    tick: view.tick,
                };
                let due = self.environment_sent.as_ref().is_none_or(|(session, sent)| {
                    *session != a.id
                        || sent.authored != next.authored
                        || sent.settings != next.settings
                        || next.settings.day_cycle.is_some()
                            && next.tick.abs_diff(sent.tick) >= bri_content::atmosphere::TICKS_PER_SECOND
                });
                if due {
                    self.environment_sent = Some((a.id, next.clone()));
                    self.ui.apply_session(a.id, UiUpdate::Environment(next));
                }
            }
            if let Some(snapshot) = &view.admin_snapshot
                && (self.ui.core.admin.snapshot.is_none()
                    || snapshot.revision > self.ui.core.admin.revision)
            {
                self.ui.apply_session(
                    a.id,
                    UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(
                        crate::admin_ui::state(snapshot),
                    )),
                );
            }
            for line in &view.chat {
                if line.id > a.last_chat {
                    // Owner 0 lines are server-authored (death messages) and
                    // may carry ML markup, colour codes and death icons.
                    // Player lines use v20's chat format
                    // `\c7<clan prefix>\c3<name>\c7<clan suffix>\c6: <text>`.
                    let text = if line.owner == 0 {
                        bri_ui::ml::sanitize(&line.text)
                    } else {
                        player_chat(&line.clan, &line.name, &line.text)
                    };
                    self.ui.apply_session(a.id, UiUpdate::Chat { text });
                    a.last_chat = line.id;
                    // addMessageCallback: the message type's GUI sound.
                    if let Some(tag) = line.tag {
                        use bri_sim::session::MessageTag;
                        let key = match tag {
                            MessageTag::UploadStart => "ui.upload_start",
                            MessageTag::UploadEnd => "ui.upload_end",
                            MessageTag::ProcessComplete => "ui.process_complete",
                            MessageTag::ClearBricks => "ui.brick_clear",
                        };
                        self.audio.trigger(key, bri_audio::Placement::Listener);
                    }
                }
            }
            // MsgStartTalking / MsgStopTalking keep WhoTalkSO's order.
            let talking = &mut a.talking;
            talking.retain(|o| view.vitals.get(o).is_some_and(|v| v.talking));
            for (owner, vitals) in &view.vitals {
                if vitals.talking && !talking.contains(owner) {
                    talking.push(*owner);
                }
            }
            let names = talking
                .iter()
                .filter_map(|o| view.names.get(o))
                .map(|n| plain_chat(n))
                .collect();
            self.ui.apply_session(a.id, UiUpdate::Talking(names));
            if a.entered
                && let Some(router) = &a.router
            {
                while let Ok(notice) = router.try_recv() {
                    for (text, confirm) in self.host_notice(notice) {
                        self.ui.apply_session(
                            a.id,
                            UiUpdate::Chat {
                                text: format!("\u{E006}{}", plain_chat(&text)),
                            },
                        );
                        if let Some(update) = confirm {
                            self.ui.apply_session(a.id, update);
                        }
                    }
                }
            }
            // Everyone's rank, from the host's administration list. Names
            // are unique on a server, so they pair the two lists.
            let rank = |name: &str| {
                view.admin_snapshot
                    .as_ref()
                    .and_then(|s| s.players.iter().find(|p| p.name == name))
                    .map(|p| p.role)
            };
            self.ui.apply_session(
                a.id,
                UiUpdate::Players {
                    rows: view
                        .names
                        .iter()
                        .map(|(&owner, name)| PlayerRow {
                            id: owner,
                            name: plain_chat(name),
                            score: view.vitals.get(&owner).map_or(0, |v| {
                                v.score.clamp(i32::MIN as i64, i32::MAX as i64) as i32
                            }),
                            admin: rank(name).map_or(
                                owner == view.owner && view.administrator,
                                bri_admin::Role::is_admin,
                            ),
                            super_admin: rank(name) == Some(bri_admin::Role::SuperAdmin),
                            bl_id: a
                                .trust
                                .get(&owner)
                                .and_then(|t| t.principal.as_ref())
                                .map(crate::trust_list::display_id),
                            trust: crate::trust_list::label(a.trust.get(&owner).map_or(
                                if owner == view.owner {
                                    bri_sim::session::TrustLevel::You
                                } else {
                                    bri_sim::session::TrustLevel::None
                                },
                                |t| t.level,
                            ))
                            .into(),
                            ignoring: a.trust.get(&owner).is_some_and(|t| t.ignoring),
                        })
                        .collect(),
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                },
            );
        }
        if let Some(view) = &a.view
            && let Err(error) = self.motion.observe(view)
        {
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::Failed {
                    reason: format!("Movement prediction: {error:#}"),
                }),
            );
            self.disconnect();
            return Ok(());
        }
        self.track_unsaved(&mut a);
        self.attempt = Some(a);
        Ok(())
    }
}
