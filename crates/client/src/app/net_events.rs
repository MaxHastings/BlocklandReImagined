//! Draining the network worker's events each frame.
use super::*;

impl App {
    pub(super) fn poll_network(&mut self) -> Result<()> {
        let Some(mut a) = self.net.attempt.take() else {
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
        let mut failed = self.drain_events(&mut a)?;
        if failed.is_none() && a.worker.events.is_closed() {
            failed = Some("Connection worker stopped".into());
        }
        if let Some(reason) = failed {
            // A joined remote game whose network dropped is rejoined
            // automatically a few times; the host gives the player their
            // owner number, and so their bricks, back.
            let id = a.id;
            let rejoin = a
                .join_target
                .clone()
                .filter(|_| a.entered && reason.contains(bri_net::client::CONNECTION_LOST));
            if let Some(target) = rejoin
                && self.net.reconnects < MAX_RECONNECTS
            {
                self.net.reconnects += 1;
                let resume = a.view.as_ref().map(|view| view.resume.clone());
                if self
                    .join_resuming(id, target, String::new(), resume)
                    .is_ok()
                {
                    return Ok(());
                }
            }
            self.net.reconnects = 0;
            // The server's Add-Ons bring content: load it and join again
            // (the downloads are cached, so this join fetches nothing).
            let add_ons = a.add_ons.lock().ok().and_then(|mut slot| slot.take());
            if let Some(set) = add_ons {
                self.disconnect();
                self.queue_package_reload(
                    set,
                    None,
                    Some(addons::ReloadResume::Downloaded {
                        id,
                        address: a.join_target.clone().unwrap_or_default(),
                    }),
                )?;
                return Ok(());
            }
            if a.identity_changed
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                // The saved server is known by its address, not an invite.
                let address = a
                    .join_target
                    .as_deref()
                    .and_then(|t| bri_net::invite::JoinTarget::parse(t).ok())
                    .map_or_else(|| a.name.clone(), |t| t.address());
                let question = self.identity_question(&address);
                self.ui
                    .apply_session(id, UiUpdate::FailureQuestion(question));
            }
            // A game this player hosted failed: its build waits to be kept.
            if a.local
                && let Some(left) = crate::recovery::left(&self.state_dir, &self.files.saves)
            {
                let question = crate::recovery::question(&left, Some("it hit an internal error"));
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
        self.take_prepared_scene(&mut a)?;
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
        }
        // The server's Add-Ons' wrench events join the wrench's lists.
        if let Some(view) = &a.view
            && let Some(update) = self.build.tool_ui.offer_events(&view.brick_events)
            && a.entered
        {
            self.ui.apply_session(a.id, update);
        }
        if let (Some(building), Some(view)) = (&mut self.build.building, &a.view) {
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
        if let (Some(building), Some(view)) = (&mut self.build.building, &a.view)
            && let Some(inventory) = view.tools.get(&view.owner)
        {
            match building.sync_tools(inventory, view.tick) {
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
            && let Some(building) = &self.build.building
        {
            let hand = bri_sim::session::BrickHand {
                stocked: building.inventory().iter().any(Option::is_some),
                equipped: matches!(building.equipment(), crate::building::Equipment::Brick(_)),
                ghost: building.ghost().is_some(),
            };
            // A full request queue leaves the report pending for the next frame.
            if self.net.brick_hand != Some(hand)
                && a.worker
                    .request(REPORT_REQUEST, Command::BrickHand(hand))
                    .is_ok()
            {
                self.net.brick_hand = Some(hand);
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
            let due = self.build.ghost_report.as_ref().is_none_or(|(sent, at)| {
                *sent != ghost && (ghost.is_none() || at.elapsed() >= GHOST_REPORT_INTERVAL)
            });
            if due
                && a.worker
                    .request(REPORT_REQUEST, Command::GhostBrick(ghost.clone()))
                    .is_ok()
            {
                self.build.ghost_report = Some((ghost, std::time::Instant::now()));
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
        if let (Some(building), Some(view)) = (&mut self.build.building, &a.view) {
            building.set_broken_shapes(&view.broken_shapes)?;
        }
        if let (Some(building), Some(view)) = (&mut self.build.building, &a.view)
            && self
                .scene
                .query_source
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
        {
            let known = self
                .scene
                .query_log
                .as_ref()
                .filter(|(log, _)| {
                    self.scene.query_source.is_some() && Arc::ptr_eq(log, &view.world_log)
                })
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
                    .scene
                    .query_source
                    .as_ref()
                    .is_none_or(|old| old.palette != view.world.palette)
            {
                let colors = self.colorset(&view.world.palette);
                self.ui.apply_session(a.id, UiUpdate::Colorset(colors));
            }
            self.scene.query_source = Some(view.world.clone());
            self.scene.query_log = Some((view.world_log.clone(), view.world_revision));
            self.gpu.ghost_uploaded = u64::MAX;
            self.fx.brick_debris.sync_world(&view.world);
        }
        if let Some(view) = &a.view {
            self.scene.mirror_index.follow(
                &view.world,
                &view.world_log,
                view.world_revision,
                &self.scene.mirror_shapes,
            );
            // One set of openings: the windows show where bodies go.
            if let Some(collision) = self.motion.collision() {
                self.scene
                    .mirror_index
                    .link(collision.links(), &self.scene.mirror_shapes);
            }
        }
        if let Some(job) = &mut self.scene.world_job
            && let Ok((source, revision, log, result)) = job.receiver.try_recv()
        {
            let left_out = std::mem::take(&mut job.left_out);
            self.scene.world_job = None;
            match result {
                // Always applied: chunk state is consistent with `source`, and
                // a newer replica is reached by the next incremental update.
                Ok((chunked, changes)) => {
                    self.scene.chunked = chunked;
                    self.scene.chunks_rebuilt += changes.len() as u64;
                    for (key, built) in changes {
                        if let Some(built) = built {
                            self.scene.cpu_chunks.insert(key, built.scene);
                            self.scene
                                .cpu_chunk_bricks
                                .insert(key, Arc::new(built.bricks));
                            self.gpu.chunk_uploads.insert(key);
                        } else {
                            self.scene.cpu_chunks.remove(&key);
                            self.scene.cpu_chunk_bricks.remove(&key);
                            self.gpu.gpu_chunks.remove(&key);
                            self.gpu.gpu_chunk_bricks.remove(&key);
                            self.gpu.chunk_uploads.remove(&key);
                        }
                    }
                    self.scene.world_source = Some(source);
                    self.scene.world_revision = revision;
                    self.scene.world_log = Some(log);
                    self.fx.brick_fades.chunks_applied(&left_out);
                    self.scene.chunks_left_out = left_out;
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
        if self.scene.world_job.is_none()
            && let (Some(meshes), Some(materials), Some(palette), Some(view)) = (
                &self.scene.meshes,
                &self.scene.materials,
                &self.scene.palette,
                &a.view,
            )
            && (self
                .scene
                .world_source
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &view.world))
                || self
                    .fx
                    .brick_fades
                    .needs_rebuild(&self.scene.chunks_left_out))
        {
            let meshes = meshes.clone();
            let materials = materials.clone();
            let palette = palette.clone();
            let world = view.world.clone();
            let (revision, log) = (view.world_revision, view.world_log.clone());
            // Compare only the bricks the replica reports changed since the
            // applied revision; without that history, compare whole worlds.
            let known = self
                .scene
                .world_log
                .as_ref()
                .filter(|applied| Arc::ptr_eq(applied, &log))
                .and_then(|log| log.between(self.scene.world_revision, revision));
            // v20 eases repainted bricks to their new colour (`brick_fade`).
            match (&self.scene.world_source, &known) {
                (Some(drawn), Some(known)) if !known.palette => {
                    self.fx
                        .brick_fades
                        .observe(drawn, &world, known.bricks.iter().copied());
                    // A knocked-out brick does not fade out in place: its
                    // debris replaces it at once. Easing it would draw it
                    // twice and cost a model per brick plus a second
                    // chunk rebuild once the fades settle.
                    let killing = self.pending_kills();
                    for id in &known.bricks {
                        if self.fx.brick_debris.is_dead(*id) || killing.contains(id) {
                            self.fx.brick_fades.settle(*id);
                        }
                    }
                }
                _ => self.fx.brick_fades.settle_all(),
            }
            settle_blasted(
                &mut self.fx.brick_fades,
                &world.bricks,
                &self.fx.brick_kills,
            );
            let left_out = self.fx.brick_fades.left_out();
            let job_left_out = left_out.clone();
            let mut chunked = std::mem::take(&mut self.scene.chunked);
            let (send, receive) = mpsc::sync_channel(1);
            self.scene.chunk_jobs += 1;
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
            self.scene.world_job = Some(WorldJob {
                receiver: receive,
                abort: task.abort_handle(),
                left_out: job_left_out,
            });
        }
        if a.reloading
            && self.scene_pipelines_ready()
            && let Some(view) = &a.view
            && self.scene.scene_map.as_deref() == Some(view.world.map_id.as_str())
            && self
                .scene
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
        self.enter_when_ready(&mut a)?;
        self.present_session(&mut a)?;
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
        self.net.attempt = Some(a);
        Ok(())
    }

    /// Handles the worker's queued events in order; the failure that ends
    /// the connection, if one came.
    fn drain_events(&mut self, a: &mut Attempt) -> Result<Option<String>> {
        let mut failed = None;
        while let Ok(event) = a.worker.events.try_recv() {
            match event {
                network::Event::Presentation { cues, dropped } => {
                    self.audio.server_dropped = dropped;
                    // The attempt is out of `self` here, so ask it directly
                    // where the local player hears from.
                    let view = a.view.as_ref().filter(|_| a.entered);
                    let heard_at = listener(&self.motion, view);
                    let mut cues = cues;
                    crate::avatar::follow_drawn_bodies(&mut cues, |owner| {
                        self.avatar.avatars.get(&owner)
                    });
                    for cue in cues {
                        self.queue_cue_heard_at(cue, heard_at);
                    }
                }
                network::Event::Ready => a.ready = true,
                network::Event::MapChanged(map) => {
                    a.saved_revision = None;
                    // Movement limits belong to the old map's Tutorial; the
                    // new map sends its own if it has any.
                    self.net.abilities = Default::default();
                    a.settling = Some(std::time::Instant::now() + SETTLE);
                    // Load the new map's scene and prediction world; the old
                    // scene stays until it is ready.
                    let paths = self.content.paths.clone();
                    let lighting = self.graphics.lighting;
                    let light_cache = self.state_dir.join("light-volumes");
                    let selected = self.content.selectable.clone();
                    let catalog = self.build.tool_ui.server_catalog();
                    let load_limit = self.load_limit.clone();
                    let (scene_tx, scene) = mpsc::sync_channel(1);
                    a.scene = scene;
                    self.world_items.reset();
                    self.fx.brick_debris.clear();
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
                                prepare_map(
                                    &paths,
                                    &map,
                                    selected,
                                    &catalog,
                                    &light_cache,
                                    lighting,
                                )
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
                        || self.net.trigger_epoch != Some(self.net.dialog_epoch)
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
                    match self.build.tool_ui.accept_inspection(
                        &reply,
                        mode,
                        None,
                        &view.world,
                        &view.names,
                        view.owner,
                    ) {
                        Ok(updates) => {
                            for mut update in updates {
                                region_defaults(&mut update, &reply, self.scene.meshes.as_deref());
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
                            self.net.abilities = abilities;
                            continue;
                        }
                        bri_sim::session::Notice::MusicTracks(music) => {
                            let Some(update) = self.build.tool_ui.offer_music(&music) else {
                                continue;
                            };
                            update
                        }
                        bri_sim::session::Notice::TempBrickColor(color) => {
                            if let Some(building) = self.build.building.as_mut() {
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
                            if let Some(building) = self.build.building.as_mut()
                                && let Err(error) = building.set_blueprint(blueprint.map(|b| *b))
                            {
                                bri_console::echo(format!("Copied build ignored: {error:#}"));
                            }
                            continue;
                        }
                        bri_sim::session::Notice::PutAway => {
                            if let Some(building) = self.build.building.as_mut() {
                                for update in building.put_away() {
                                    self.ui.apply_session(a.id, update);
                                }
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MirrorCopy { across_z } => {
                            if let Some(building) = self.build.building.as_mut() {
                                building.mirror_copy(across_z);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MirrorGhost {
                            definition,
                            quarter_turns,
                        } => {
                            if let Some(building) = self.build.building.as_mut() {
                                building.mirror_ghost(&definition, quarter_turns);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MoveCopy { point, normal } => {
                            if let Some(building) = self.build.building.as_mut() {
                                building.move_copy(point, normal);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::FlipCopy => {
                            if let Some(building) = self.build.building.as_mut() {
                                building.flip_copy();
                            }
                            continue;
                        }
                        bri_sim::session::Notice::PivotCopy { whole } => {
                            if let Some(building) = self.build.building.as_mut() {
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
                            self.ui
                                .core
                                .request(UiAction::Game(GameAction::RotateBrick {
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
                            if let Some(building) = self.build.building.as_mut() {
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
                            if let Some(building) = self.build.building.as_mut()
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
                network::Event::Reply {
                    request,
                    result,
                    revision,
                    tick,
                } => {
                    self.accept_reply(a, request, result, revision, tick);
                }
                network::Event::Failed(reason) => {
                    failed = Some(reason);
                    break;
                }
            }
        }
        Ok(failed)
    }

    /// Takes the map scene the loader prepared and sets up the world for it.
    fn take_prepared_scene(&mut self, a: &mut Attempt) -> Result<()> {
        if let Ok(prepared) = a.scene.try_recv() {
            a.progress.begin(
                bri_progress::Stage::LoadingGraphics,
                bri_progress::Unit::Steps,
                None,
            );
            // A map change: renderers keep per-map sky and terrain state,
            // so rebuild them for the new map like a fresh join, once it is
            // set up below. Finishing the change waits for their pipelines
            // (`scene_pipelines_ready`), so they are rebuilt here, not on
            // the next frame: a window drawing no frames (minimized, or
            // covered) would otherwise never finish it.
            let mut rebuild = None;
            if self.scene.cpu_scene.is_some() {
                rebuild = self.gpu.device.take();
                self.gpu_stopped();
                self.gpu.gpu_restart = rebuild.is_none();
            }
            self.scene.scene_map = Some(prepared.map_id.clone());
            self.foliage.set_map(prepared.foliage);
            self.weather.set_map(&prepared.map_id, prepared.waters)?;
            self.lighting.light_volume = prepared.light_volume;
            self.scene.cpu_scene = Some(prepared.scene);
            self.scene.shape_indices = prepared.shape_indices;
            self.scene.cpu_terrain = prepared.terrain;
            self.scene.meshes = Some(prepared.meshes);
            self.scene.mirror_shapes = prepared.mirror_shapes;
            self.scene.mirror_index.clear();
            self.scene.materials = Some(prepared.materials);
            self.scene.palette = Some(prepared.palette);
            self.gpu.gpu_palette = None;
            // Easing bricks bind the old palette's materials.
            self.fx.fade_models.clear();
            let old = self.build.building.replace(prepared.building);
            // The new controller has not seen any world or palette yet.
            // The replica can be unchanged while the background map load finishes.
            self.scene.query_source = None;
            self.scene.query_log = None;
            self.motion.install(prepared.mirror);
            let building = self.build.building.as_mut().unwrap();
            building.set_tool_catalog(self.item_ui.catalog())?;
            if let Some(old) = &old {
                building.carry_over(old);
            }
            self.gpu.gpu_scene = None;
            self.gpu.gpu_terrain.clear();
            if let Some((device, queue, format)) = rebuild {
                self.gpu_ready(&device, &queue, format)?;
            }
            // A map change replaced what first entry set the HUD up from.
            if a.entered
                && let (Some(scene), Some(view)) = (&self.scene.cpu_scene, &a.view)
            {
                for update in self.map_setup_updates(scene, &view.world.palette)? {
                    self.ui.apply_session(a.id, update);
                }
            }
        }
        Ok(())
    }

    /// Enters the game once the connection is ready and the world is built.
    fn enter_when_ready(&mut self, a: &mut Attempt) -> Result<()> {
        if a.ready
            && !a.entered
            && self.scene.world_source.is_some()
            && self.scene_pipelines_ready()
            && let Some(scene) = &self.scene.cpu_scene
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
            self.net.reconnects = 0;
            for text in std::mem::take(&mut self.net.join_notices) {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
            // Admins hear once per game what this computer's Add-Ons lack.
            if a.view.as_ref().is_some_and(|v| v.administrator)
                && let Some(text) = self.addons.add_on_health.summary()
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
            if self.addons.client_code.loaded_from() != Some(&set) {
                self.addons.client_code =
                    crate::client_code::ClientCode::load(&self.content.paths.root, &set);
            }
            self.addons.client_code.start(
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
                && let Some(prompt) = self.addons.client_code.trust_prompt(
                    &server,
                    &plain_chat(&a.name),
                    &self.state_dir,
                )
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
        Ok(())
    }

    /// Hands the session's view to the interface: environment, players,
    /// minigames and the other windows that follow the game.
    fn present_session(&mut self, a: &mut Attempt) -> Result<()> {
        if let Some(view) = &a.view {
            // The Environment window's view: on every change, and each
            // second while a day/night cycle turns.
            if let Some(scene) = &self.scene.cpu_scene {
                let next = bri_ui::models::environment::EnvironmentView {
                    authored: authored_environment(scene),
                    settings: view.environment.clone(),
                    tick: view.tick,
                };
                let due = self
                    .net
                    .environment_sent
                    .as_ref()
                    .is_none_or(|(session, sent)| {
                        *session != a.id
                            || sent.authored != next.authored
                            || sent.settings != next.settings
                            || next.settings.day_cycle.is_some()
                                && next.tick.abs_diff(sent.tick)
                                    >= bri_content::atmosphere::TICKS_PER_SECOND
                    });
                if due {
                    self.net.environment_sent = Some((a.id, next.clone()));
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
        Ok(())
    }
}
