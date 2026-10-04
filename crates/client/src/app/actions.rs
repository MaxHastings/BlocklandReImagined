//! Dispatching UI actions after each input event.
use super::*;

impl App {
    pub(super) fn dispatch(&mut self) -> Result<Vec<PlatformCommand>> {
        for sound in self.ui.drain_sounds() {
            self.audio
                .profile(sound.profile, bri_audio::Placement::Listener);
        }
        let mut platform = Vec::new();
        for (id, action) in self.ui.drain_actions() {
            if self.intercept_action(id, &action) {
                continue;
            }
            self.dispatch_action(id, action, &mut platform)?;
        }
        Ok(platform)
    }

    /// Actions that never reach `dispatch_action`: a spectator's buttons,
    /// a dead player's clicks, recorded macros and building actions.
    /// True when `action` was handled here.
    fn intercept_action(&mut self, id: RequestId, action: &UiAction) -> bool {
        note_trigger(&mut self.controls, action);
        // A spectator's keys go to the rules that hold them
        // (`Observer::onTrigger`, `serverCmdLight`).
        if self.spectating() {
            let button = match action {
                UiAction::Game(GameAction::Held {
                    control,
                    down: true,
                }) => match control {
                    HeldControl::Fire => Some(bri_sim::session::ObserverButton::Fire),
                    HeldControl::Jump => Some(bri_sim::session::ObserverButton::Jump),
                    HeldControl::Jet => Some(bri_sim::session::ObserverButton::Jet),
                    _ => None,
                },
                UiAction::Game(GameAction::UseLight) => {
                    Some(bri_sim::session::ObserverButton::Light)
                }
                _ => None,
            };
            let swallowed = button.is_some()
                || matches!(
                    action,
                    UiAction::Game(GameAction::Held {
                        control: HeldControl::Fire | HeldControl::Jump | HeldControl::Jet,
                        down: false,
                    })
                );
            if let Some(button) = button {
                let command = Command::ObserverButton(button);
                if let Err(error) = self.command(id, command, action.clone()) {
                    self.answer(id, Err(error));
                }
                return true;
            }
            if swallowed {
                self.answer(id, Ok(()));
                return true;
            }
        }
        // Dead players click to respawn; other fire/tool input is ignored.
        if !self.local_alive()
            && matches!(
                action,
                UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    ..
                })
            )
        {
            if matches!(action, UiAction::Game(GameAction::Held { down: true, .. })) {
                let ready = self.network_view().is_some_and(|view| {
                    view.vitals
                        .get(&view.owner)
                        .is_some_and(|v| !v.respawn_held && view.tick >= v.respawn_tick)
                });
                if ready {
                    if let Err(error) = self.command(id, Command::Respawn, action.clone()) {
                        self.answer(id, Err(error));
                    }
                    return true;
                }
            }
            self.answer(id, Ok(()));
            return true;
        }
        // Clicking out of the spy orbit returns to the body
        // (`Observer::onTrigger` in `Corpse` mode); the free camera
        // uses it only to fly faster. The dead click to respawn above.
        // In an Add-On's orbit whose body acts the click is the
        // player's empty-hand trigger, for the Add-On
        // (`Observer::onTrigger` in its mode); a frozen one's keys went
        // to the rules above.
        if let Some(observer) = self.controls.observer()
            && let UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down,
            }) = action
        {
            let addon_orbit = self.network_view().is_some_and(|v| {
                v.vitals.get(&v.owner).is_some_and(|v| {
                    matches!(
                        v.control,
                        bri_sim::session::ControlObject::Orbit {
                            body: bri_sim::session::OrbitBody::Acts,
                            ..
                        }
                    )
                })
            });
            if addon_orbit {
                let command = if *down {
                    Command::Activate
                } else {
                    Command::ActivateRelease
                };
                if let Err(error) = self.command(id, command, action.clone()) {
                    self.answer(id, Err(error));
                }
            } else if *down && matches!(observer.mode, crate::controls::ObserverMode::Orbit(_)) {
                if let Err(error) = self.command(id, Command::ControlPlayer, action.clone()) {
                    self.answer(id, Err(error));
                }
            } else {
                self.answer(id, Ok(()));
            }
            return true;
        }
        if self.local_weapon_seat()
            && let UiAction::Game(GameAction::Held {
                control: HeldControl::Fire,
                down,
            }) = action
        {
            // A gunner's fire drives the vehicle weapon (tank, cannon);
            // every other rider uses their tools as on foot.
            if let Err(error) =
                self.command(id, Command::WeaponTrigger { down: *down }, action.clone())
            {
                self.answer(id, Err(error));
            }
            return true;
        }
        if crate::minigame_ui::is_minigame_action(action) {
            let own = self
                .network_view()
                .and_then(|v| v.vitals.get(&v.owner).and_then(|v| v.minigame));
            let owner = self.network_view().is_some_and(|v| {
                v.minigames
                    .iter()
                    .any(|g| Some(g.id) == own && g.owner == v.owner)
            });
            let result = crate::minigame_ui::command(action, own, owner).and_then(|command| {
                match command {
                    Some(command) => self.command(id, command, action.clone()).map(|()| true),
                    None => {
                        self.combat.minigame_state = None; // force a refresh
                        Ok(false)
                    }
                }
            });
            match result {
                Ok(true) => {}
                Ok(false) => self.answer(id, Ok(())),
                Err(error) => self.answer(id, Err(error)),
            }
            return true;
        }
        if matches!(action, UiAction::OpenAdmin | UiAction::Admin(_)) {
            let admin_action = match action {
                UiAction::Admin(action) => action.clone(),
                _ => bri_ui::models::admin::AdminAction::Refresh,
            };
            if let Err(error) = self.handle_admin(id, admin_action) {
                self.answer(id, Err(error));
            }
            return true;
        }
        if let Some(recording) = &mut self.build.macro_recording
            && macro_action(action)
            && recording.len() < 4096
        {
            recording.push(action.clone());
        }
        if building_action(action) {
            match self.handle_building(id, action) {
                Ok(true) => {}
                Ok(false) => self.answer(id, Err(anyhow::anyhow!("Not connected"))),
                Err(error) => self.answer(id, Err(error)),
            }
            return true;
        }
        false
    }

    /// Runs one UI action and answers it, or queues the platform command
    /// it asks for.
    pub(super) fn dispatch_action(
        &mut self,
        id: RequestId,
        action: UiAction,
        platform: &mut Vec<PlatformCommand>,
    ) -> Result<()> {
        let session = matches!(
            action,
            UiAction::HostGame { .. }
                | UiAction::JoinServer { .. }
                | UiAction::TrustNewServerIdentity { .. }
                | UiAction::StartTutorial
        );
        let host_needs_content =
            matches!(action, UiAction::HostGame { .. } | UiAction::StartTutorial)
                && !self.addons.packages_from_tools;
        if session && self.addons.add_on_sync.is_some() {
            // The Add-Ons folder is still converting, which can replace or
            // remove Add-Ons that are on: a game started now would have
            // them change under it. It starts once that is done, as it
            // waits for loading.
            if self.ui.session_request() != Some(id) {
                return Ok(());
            }
            self.disconnect();
            self.ui.apply_session(
                id,
                UiUpdate::Connection(ConnectionState::Connecting {
                    text: "Converting Add-Ons…".into(),
                }),
            );
            self.addons.after_sync = Some(addons::ReloadResume::Action {
                id,
                action: Box::new(action),
            });
            return Ok(());
        }
        if session && (host_needs_content || self.addons.reload.is_some()) {
            if self.ui.session_request() != Some(id) {
                return Ok(());
            }
            self.disconnect();
            let result = bri_package::packages::PackageSet::load_root(&self.content.paths.root)
                .and_then(|set| {
                    self.queue_package_reload(
                        set,
                        None,
                        Some(addons::ReloadResume::Action {
                            id,
                            action: Box::new(action),
                        }),
                    )
                });
            if let Err(error) = result {
                self.answer(id, Err(error));
            }
            return Ok(());
        }
        let result = match action {
            UiAction::LoadBricksColors(choice) => {
                self.choose_color_load(choice);
                Ok(())
            }
            UiAction::RequestSaveList { .. } | UiAction::LoadBricks { .. } => {
                // Saves dropped in while the game runs convert too.
                if matches!(action, UiAction::RequestSaveList { .. })
                    && self.files.old_saves_started
                {
                    self.files.old_saves.start();
                }
                let result = (|| {
                    if matches!(action, UiAction::LoadBricks { .. }) {
                        ensure!(
                            self.network_view().is_some_and(|v| v.administrator),
                            "Loading requires host or administrator permission"
                        );
                    }
                    self.files.file_jobs.enqueue(crate::saves::Request {
                        id,
                        session: self.net.attempt.as_ref().map(|a| a.id),
                        action,
                        build: None,
                    })
                })();
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::SaveBricks {
                ref name,
                ref description,
                events,
                ownership,
                ..
            } => {
                let result = (|| {
                    ensure!(
                        crate::saves::valid_name(name),
                        "Invalid native save filename"
                    );
                    ensure!(
                        description.len() <= 64 * 1024,
                        "Save description is too long"
                    );
                    self.command(id, Command::SaveBuild { events, ownership }, action.clone())
                })();
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Quit => {
                self.disconnect();
                platform.push(PlatformCommand::Quit);
                return Ok(());
            }
            UiAction::ApplyDisplay {
                resolution,
                fullscreen,
                vsync,
            } => {
                platform.push(PlatformCommand::ApplyDisplay {
                    request: id,
                    resolution,
                    fullscreen,
                    vsync,
                });
                return Ok(());
            }
            UiAction::SaveSettings(value) => {
                let max_fps = settings::startup_display(&value).max_fps;
                if max_fps != self.perf.frame_limit {
                    self.perf.frame_limit = max_fps;
                    platform.push(PlatformCommand::FrameLimit(max_fps));
                }
                settings::save(&self.state_dir.join("settings.json"), &value).and_then(|()| {
                    self.audio.apply_settings(&value);
                    self.graphics = crate::graphics::Graphics::from_settings(&value);
                    self.weather.apply_settings(&value)
                })
            }
            UiAction::SetVolume { channel, value } => self.audio.set_volume(&channel, value),
            UiAction::OpenSavesFolder => show_drop_folder(self.files.old_saves.saves_folder()),
            UiAction::RefreshHostColorsets => {
                self.ui
                    .apply(UiUpdate::HostColorsets(crate::colorsets::catalog(
                        &self.content.paths.root,
                        &self.state_dir,
                        &self.content.paint,
                    )));
                Ok(())
            }
            UiAction::ColorsetsFolder => {
                let result = show_drop_folder(&self.state_dir.join("colorsets"));
                self.ui
                    .apply(UiUpdate::HostColorsets(crate::colorsets::catalog(
                        &self.content.paths.root,
                        &self.state_dir,
                        &self.content.paint,
                    )));
                result
            }
            UiAction::OpenUrl(url) => {
                // Only web pages, after the player confirmed them.
                if bri_ui::ui::web_url(&url).as_deref() == Some(url.as_str())
                    && !bri_crash::open(&url)
                {
                    bri_console::warn(format!("Could not open {url}"));
                }
                Ok(())
            }
            UiAction::HostGame {
                map,
                mode,
                game_mode,
                max_players,
                server_name,
                password,
                admin_password,
                super_admin_password,
            } => {
                if self.ui.session_request() != Some(id) {
                    return Ok(());
                }
                let result = self.host(
                    id,
                    map,
                    mode,
                    game_mode,
                    max_players,
                    server_name,
                    password,
                    admin_password,
                    super_admin_password,
                );
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::JoinServer { address, password } => {
                if self.ui.session_request() != Some(id) {
                    return Ok(());
                }
                let result = self.join(id, address, password);
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::TrustNewServerIdentity { address } => {
                if self.ui.session_request() != Some(id) {
                    return Ok(());
                }
                let result = self
                    .forget_server_identity(&address)
                    .and_then(|()| self.join(id, address, String::new()));
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::TrustAddOnCode => self.addons.client_code.accept_trust(&self.state_dir),
            UiAction::ForgetAddOnTrust => {
                crate::client_code::ClientCode::forget_trust(&self.state_dir)
            }
            UiAction::CancelConnect | UiAction::Disconnect => {
                if self.net.attempt.as_ref().is_none_or(|a| a.id <= id) {
                    self.disconnect();
                }
                // Core may already contain a newer attempt queued in this
                // same UI drain. An older cancel must not clear its token.
                if self.ui.session_request().is_none_or(|token| token <= id) {
                    self.ui.apply(UiUpdate::Connection(ConnectionState::Idle));
                }
                Ok(())
            }
            UiAction::Game(GameAction::ToggleFullscreen) => {
                platform.push(PlatformCommand::ToggleFullscreen);
                return Ok(());
            }
            UiAction::Game(GameAction::SavePerfCapture) => {
                let dir = self.state_dir.join("captures");
                let version = self.ui.core.version.clone();
                let text = match crate::perf::save_capture(&dir, &self.ui.core, &version) {
                    Ok(path) => {
                        bri_console::echo(format!("Performance capture saved: {}", path.display()));
                        format!(
                            "Performance capture saved: {}",
                            path.file_name()
                                .map_or_else(String::new, |n| n.to_string_lossy().into())
                        )
                    }
                    Err(error) => format!("Performance capture failed: {error:#}"),
                };
                self.ui.apply(UiUpdate::BottomPrint {
                    text,
                    seconds: 3.0,
                    hide_bar: false,
                });
                Ok(())
            }
            UiAction::Game(GameAction::ToggleBuildMacroRecording) => {
                let text = match self.build.macro_recording.take() {
                    Some(recorded) => {
                        let count = recorded.len();
                        self.build.build_macro = recorded;
                        format!("Build macro saved ({count} actions)")
                    }
                    None => {
                        self.build.macro_recording = Some(Vec::new());
                        "Recording build macro...".into()
                    }
                };
                self.ui.apply(UiUpdate::BottomPrint {
                    text,
                    seconds: 3.0,
                    hide_bar: false,
                });
                Ok(())
            }
            UiAction::Game(GameAction::PlayBackBuildMacro) => {
                if self.build.macro_recording.is_none() {
                    self.build
                        .macro_playback
                        .extend(self.build.build_macro.iter().cloned());
                }
                Ok(())
            }
            UiAction::Game(GameAction::Screenshot { kind }) => {
                let format = bri_ui::screens::options::screenshot_format(&self.ui.core.prefs);
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                platform.push(PlatformCommand::Screenshot {
                    path: self
                        .state_dir
                        .join("screenshots")
                        .join(format!("Blockland_{stamp}.{}", format.extension())),
                    hud: kind == ScreenshotKind::Normal,
                });
                Ok(())
            }
            UiAction::Game(GameAction::DropCameraAtPlayer) => {
                // `serverCmdDropCameraAtPlayer`: the server hands control
                // to the camera; pressing again re-drops it at the eye.
                match self.network_view() {
                    Some(view) if view.administrator => {
                        if let Some(eye) = self.local_eye() {
                            self.controls.redrop_camera(eye);
                        }
                        let result = self.command(
                            id,
                            Command::Admin(bri_admin::Request::new(
                                bri_admin::Action::DropCameraAtPlayer,
                            )),
                            action.clone(),
                        );
                        if result.is_ok() {
                            return Ok(());
                        }
                        result
                    }
                    Some(_) => Err(anyhow::anyhow!(
                        "Only administrators can use the free camera"
                    )),
                    None => Err(anyhow::anyhow!("Not connected")),
                }
            }
            UiAction::Game(GameAction::DropPlayerAtCamera) => {
                // `serverCmdDropPlayerAtCamera`: the server moves the
                // body, or the vehicle it rides, to the camera (where it
                // was last left when none is flying), or respawns.
                match self.network_view() {
                    Some(view) if view.administrator => {
                        let camera = self.camera_view();
                        // The body arrives facing the camera's heading.
                        if let Some(camera) = camera {
                            self.controls.yaw = camera.yaw;
                        }
                        let result =
                            self.command(id, Command::DropPlayerAtCamera(camera), action.clone());
                        if result.is_ok() {
                            return Ok(());
                        }
                        result
                    }
                    _ => Ok(()),
                }
            }
            UiAction::Game(GameAction::NextSeat | GameAction::PrevSeat) => {
                let step = if matches!(action, UiAction::Game(GameAction::NextSeat)) {
                    1
                } else {
                    -1
                };
                let result = self.command(id, Command::SwitchSeat(step), action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(GameAction::Suicide) => {
                let result = self.command(id, Command::Suicide, action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(GameAction::UseLight) => {
                let result = self.command(id, Command::ToggleLight, action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(GameAction::Package {
                ref package,
                ref command,
                pressed,
            }) => {
                let request = Command::Package(bri_sim::session::PackageCommand {
                    package: package.clone(),
                    command: command.clone(),
                    args: pressed
                        .map(bri_sim::session::PackageArg::Bool)
                        .into_iter()
                        .collect(),
                });
                let result = self.command(id, request, action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(GameAction::CameraZoom { notches }) => {
                self.controls.zoom_orbit(notches);
                return Ok(());
            }
            UiAction::Game(GameAction::ToolWheel { notches }) => {
                if self.controls.aim_wheel(notches) {
                    return Ok(());
                }
                // The image's `wheel` command names "package:command".
                let Some((package, command)) = self
                    .view
                    .tool_wheel
                    .as_deref()
                    .and_then(|c| c.split_once(':'))
                else {
                    return Ok(());
                };
                let request = Command::Package(bri_sim::session::PackageCommand {
                    package: package.to_string(),
                    command: command.to_string(),
                    args: vec![bri_sim::session::PackageArg::Int(notches.into())],
                });
                let result = self.command(id, request, action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(GameAction::Emote { ref name }) => {
                let name = name.to_ascii_lowercase();
                let result = self.command(id, Command::Emote(name), action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Game(action) => {
                if self.controls.action(&action) {
                    return Ok(());
                } else {
                    Err(anyhow::anyhow!(
                        "This gameplay action is not connected to the client yet"
                    ))
                }
            }
            UiAction::Chat {
                channel: ChatChannel::Say,
                text,
            } => {
                let result = self.command(
                    id,
                    Command::Chat(text.clone()),
                    UiAction::Chat {
                        channel: ChatChannel::Say,
                        text,
                    },
                );
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::Chat {
                channel: ChatChannel::Team,
                text,
            } => {
                let result = self.command(
                    id,
                    Command::TeamChat(text.clone()),
                    UiAction::Chat {
                        channel: ChatChannel::Team,
                        text,
                    },
                );
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            // `serverCmdClearInventory`: the brick cart empties. Ours is
            // kept here, so it empties as Buy Bricks with ten empty slots.
            UiAction::ChatCommand { ref name, .. }
                if name.eq_ignore_ascii_case("clearinventory") =>
            {
                let clear = UiAction::BuyBricks {
                    slots: vec![None; 10],
                };
                match self.handle_building(id, &clear) {
                    Ok(true) => return Ok(()),
                    Ok(false) => Err(anyhow::anyhow!("Not connected")),
                    Err(error) => Err(error),
                }
            }
            UiAction::ChatCommand { ref name, .. } if name.eq_ignore_ascii_case("invite") => {
                match self.net.invite.clone() {
                    Some(invite) => {
                        let copied = copy_to_clipboard(&invite);
                        let text = match &copied {
                            Ok(()) => format!("Invite copied to the clipboard: {invite}"),
                            Err(_) => format!("Your invite: {invite}"),
                        };
                        if let Some(a) = &self.net.attempt {
                            self.ui.apply_session(a.id, UiUpdate::Chat { text });
                        }
                        Ok(())
                    }
                    None => Err(anyhow::anyhow!(
                        "Only the host has an invite, and it appears once hosting has checked your connection."
                    )),
                }
            }
            UiAction::ChatCommand { ref name, ref args } => {
                let snapshot = self
                    .net
                    .attempt
                    .as_ref()
                    .and_then(|a| a.view.as_ref())
                    .and_then(|v| v.admin_snapshot.as_ref());
                let admin = match snapshot {
                    Some(snapshot) => crate::admin_ui::chat_command(name, args, snapshot),
                    None => Ok(None),
                };
                // Vanilla slash commands that map to existing requests.
                let command = match admin {
                    Err(error) => {
                        self.answer(id, Err(error));
                        return Ok(());
                    }
                    Ok(Some(command)) => Some(command),
                    Ok(None) => match name.to_ascii_lowercase().as_str() {
                        "suicide" | "kill" => Some(Command::Suicide),
                        "light" => Some(Command::ToggleLight),
                        "clearcheckpoint" => Some(Command::ClearCheckpoint),
                        "treasurestatus" => Some(Command::TreasureStatus),
                        "wand" => Some(Command::Wand),
                        // `serverCmdRet`: back from `/spy` to one's own body.
                        "ret" => Some(Command::ControlPlayer),
                        // `serverCmdWtf` and `serverCmdZombie` repeat
                        // `/confusion` and `/hug`.
                        "wtf" => Some(Command::Emote("confusion".into())),
                        "zombie" => Some(Command::Emote("hug".into())),
                        "sit" | "love" | "hate" | "alarm" | "confusion" | "bsd" | "hug" => {
                            Some(Command::Emote(name.to_ascii_lowercase()))
                        }
                        // Every other slash command goes to the host, which
                        // runs the Add-On command of that name, or answers
                        // that there is none (v20's `/x` calls `serverCmdX`).
                        _ => Some(Command::Package(bri_sim::session::PackageCommand {
                            package: String::new(),
                            command: name.clone(),
                            args: args
                                .iter()
                                .cloned()
                                .map(bri_sim::session::PackageArg::String)
                                .collect(),
                        })),
                    },
                };
                match command {
                    Some(command) => {
                        let result = self.command(id, command, action.clone());
                        if result.is_ok() {
                            return Ok(());
                        }
                        result
                    }
                    None => Err(anyhow::anyhow!("Unknown command: /{name}")),
                }
            }
            UiAction::StartTutorial => {
                if self.ui.session_request() != Some(id) {
                    return Ok(());
                }
                let result = self.host(
                    id,
                    bri_sim::tutorial::MAP_ID.into(),
                    ServerMode::SinglePlayer,
                    None,
                    1,
                    "Tutorial".into(),
                    String::new(),
                    String::new(),
                    String::new(),
                );
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::SteeringPrefs {
                strafe,
                auto_return,
            } => {
                if self.network_view().is_none() {
                    return Ok(());
                }
                let result = self.command(
                    id,
                    Command::SteeringPrefs {
                        strafe,
                        auto_return,
                    },
                    action.clone(),
                );
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::StartTyping | UiAction::StopTyping => {
                if self.network_view().is_none() {
                    return Ok(());
                }
                let talking = matches!(action, UiAction::StartTyping);
                let result = self.command(id, Command::Talking(talking), action.clone());
                if result.is_ok() {
                    return Ok(());
                }
                result
            }
            UiAction::ClosePrintSelector | UiAction::CancelWrench { .. } => Ok(()),
            UiAction::TrustInvite { target, level } => {
                self.command(id, Command::TrustInvite { target, level }, action.clone())
            }
            UiAction::TrustDemote { target, level } => {
                self.command(id, Command::DemoteTrust { target, level }, action.clone())
            }
            UiAction::UnIgnore { target } => {
                self.command(id, Command::UnIgnore { target }, action.clone())
            }
            UiAction::AnswerTrustInvite { from, answer } => {
                let command = match answer {
                    TrustAnswer::Accept => Command::AcceptTrust { from },
                    TrustAnswer::Reject => Command::RejectTrust { from },
                    TrustAnswer::Ignore => Command::IgnoreTrust { from },
                };
                self.command(id, command, action.clone())
            }
            UiAction::SetAvatar(ref prefs) => {
                let connected = self.network_view().is_some();
                let result = self
                    .avatar
                    .avatar_assets
                    .from_prefs(prefs)
                    .and_then(|appearance| {
                        if connected {
                            self.command(id, Command::Avatar(appearance), action.clone())
                        } else {
                            Ok(())
                        }
                    });
                if connected && result.is_ok() {
                    self.send_name(prefs);
                    return Ok(());
                }
                result
            }
            UiAction::PreviewSave { map, name } => {
                let key = (map, name);
                match self.files.save_pictures.get(&key).cloned() {
                    Some(path) => self.files.save_previews.start(key, path, &self.runtime),
                    None => {
                        self.files.save_previews.cancel();
                        self.ui.apply(UiUpdate::SavePreview {
                            map: key.0,
                            name: key.1,
                            preview: IconRef::None,
                        });
                    }
                }
                Ok(())
            }
            UiAction::PreviewAvatar {
                avatar,
                camera_rotation,
                orbit_distance,
            } => self
                .avatar
                .avatar_assets
                .from_prefs(&avatar)
                .and_then(|appearance| {
                    ensure!(
                        camera_rotation.iter().all(|v| v.is_finite())
                            && orbit_distance.is_finite()
                            && (1.0..=20.0).contains(&orbit_distance),
                        "Invalid avatar preview camera"
                    );
                    self.avatar.preview_request =
                        Some((appearance, camera_rotation, orbit_distance));
                    self.avatar.preview_dirty = true;
                    Ok(())
                }),
            UiAction::QueryLan => {
                let (send, receive) = mpsc::sync_channel(1);
                let saved =
                    crate::servers::SavedServers::load(&self.state_dir.join("servers.json"));
                let pins: BTreeMap<String, Vec<u8>> =
                    read_small_json(&self.state_dir.join("trusted-hosts.json")).unwrap_or_default();
                self.runtime.spawn(async move {
                    let broadcast = [bri_net::discovery::broadcast()];
                    let lan = bri_net::discovery::query(&broadcast, Duration::from_millis(1200));
                    // Every saved server is asked at once over its game
                    // port; a probe never pins anything.
                    let probes = saved.servers.into_iter().map(|server| {
                        let pin = pins.get(&server.address).cloned();
                        async move {
                            let probe = async {
                                let target = bri_net::invite::JoinTarget::parse(server.target())?;
                                let route = target.resolve().await?;
                                let pin = match (route.key, pin) {
                                    (Some(key), _) => HostPin::Key(key),
                                    (None, Some(certificate)) => HostPin::Certificate(certificate),
                                    (None, None) => HostPin::FirstUse,
                                };
                                bri_net::client::probe(route.address, &pin, Duration::from_secs(2))
                                    .await
                            }
                            .await
                            .map_err(|error| probe_failure(&error));
                            (server, probe)
                        }
                    });
                    let (lan, saved) = tokio::join!(lan, futures_join_all(probes));
                    let _ = send.send(JoinList {
                        lan: lan.unwrap_or_default(),
                        saved,
                    });
                });
                self.lobby.lan_query = Some(receive);
                self.ui.apply(UiUpdate::LanServers {
                    servers: vec![],
                    querying: true,
                });
                Ok(())
            }
            UiAction::RequestAddOns => {
                let mut view = crate::add_ons::view(&self.content.paths.root);
                if let Err(error) = self.sync_add_ons() {
                    view.notice = format!("{error:#}");
                }
                self.show_add_ons(view);
                Ok(())
            }
            UiAction::SetAddOnEnabled { ref id, enabled } => crate::add_on_choices::set_enabled(
                &self.content.paths.root,
                &self.state_dir,
                id,
                enabled,
            )
            .map(|view| self.add_ons_listed(view)),
            UiAction::DefaultAddOns => {
                crate::add_on_choices::defaults(&self.content.paths.root, &self.state_dir)
                    .map(|view| self.add_ons_listed(view))
            }
            UiAction::ApplyAddOns
                if self.addons.packages_from_tools || self.net.attempt.is_some() =>
            {
                Ok(())
            }
            UiAction::ApplyAddOns => {
                let result = bri_package::packages::PackageSet::load_root(&self.content.paths.root)
                    .and_then(|set| self.queue_package_reload(set, Some(id), None))
                    .context("Your Add-On changes could not be loaded");
                if let Err(error) = result {
                    self.answer(id, Err(error));
                }
                return Ok(());
            }
            UiAction::ImportAddOn { id: ref row } => {
                let root = self.content.paths.root.clone();
                crate::add_ons::retry(&root, row).and_then(|()| {
                    self.sync_add_ons()?;
                    let mut view = crate::add_ons::view(&root);
                    view.notice = "Converting... the game keeps running meanwhile.".into();
                    self.show_add_ons(view);
                    Ok(())
                })
            }
            UiAction::OpenAddOnsFolder => {
                let folder = bri_package::classic::folder(&self.content.paths.root);
                std::fs::create_dir_all(&folder)
                    .with_context(|| format!("Could not create {}", folder.display()))
                    .and_then(|()| {
                        if !bri_crash::open(&folder.to_string_lossy()) {
                            anyhow::bail!(
                                "Could not open the Add-Ons folder. You can open it manually at {}",
                                folder.display()
                            );
                        }
                        Ok(())
                    })
            }
            UiAction::ToggleFavorite { ref address } => {
                let path = self.state_dir.join("servers.json");
                let mut saved = crate::servers::SavedServers::load(&path);
                // LAN rows are keyed by address; saved rows may be invites.
                let target = bri_net::invite::JoinTarget::parse(address);
                let key = target.as_ref().map_or(address.clone(), |t| t.address());
                let invite = target
                    .ok()
                    .filter(|t| t.key().is_some())
                    .map(|t| t.to_string())
                    .or_else(|| {
                        // The same order as joining: a saved pin before
                        // an (unsigned) LAN listing, so starring a
                        // spoofed LAN row cannot pin its sender.
                        let pins: BTreeMap<String, Vec<u8>> =
                            read_small_json(&self.state_dir.join("trusted-hosts.json"))
                                .unwrap_or_default();
                        let (pin, _) = crate::servers::join_pin(
                            None,
                            pins.get(&key).cloned(),
                            self.lobby.lan_hosts.get(address),
                        );
                        let HostPin::Certificate(certificate) = pin else {
                            return None;
                        };
                        Some(
                            bri_net::invite::JoinTarget::Direct {
                                host: target_host(address),
                                port: address
                                    .rsplit_once(':')
                                    .and_then(|(_, p)| p.parse().ok())
                                    .unwrap_or(bri_net::invite::DEFAULT_PORT),
                                key: Some(bri_net::invite::host_key(&certificate)),
                            }
                            .to_string(),
                        )
                    });
                let name = self
                    .ui
                    .core
                    .servers
                    .iter()
                    .find(|s| s.address == *address)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                saved.toggle_favorite(&key, invite, &name);
                let result = saved.save(&path);
                self.ui.core.request(UiAction::QueryLan);
                result
            }
            UiAction::AllowFirewall { port } => {
                let (send, receive) = mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let _ = send.send(crate::firewall::allow(port).map_err(|e| format!("{e:#}")));
                });
                self.lobby.firewall_fix = Some(receive);
                Ok(())
            }
            UiAction::Console { ref line } => {
                let mut out = bri_console::Output::default();
                let unknown = crate::console::registry().exec(self, line, &mut out);
                out.flush();
                match unknown.first() {
                    Some(line) => Err(anyhow::anyhow!("Unknown console command: {line}")),
                    None => Ok(()),
                }
            }
            _ => Err(anyhow::anyhow!("This isn't available yet.")),
        };
        self.answer(id, result);
        Ok(())
    }
}
