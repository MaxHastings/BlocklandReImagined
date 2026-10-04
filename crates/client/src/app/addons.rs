//! Add-On packages: enabling, applying and their HUD.
use super::*;
use std::rc::Rc;

/// Add-On packages: the catalog, client code, server packages and imports.
pub(super) struct AddOns {
    pub(super) reload: Option<ReloadJob>,
    pub(super) reload_pending: Option<ReloadRequest>,
    /// Client-side mod packages (HUD panels, models) from `packages.json`.
    pub(super) package_catalog: Option<Arc<bri_package_runtime::Catalog>>,
    /// Sandboxed code of enabled Add-Ons, run while a game is entered.
    pub(super) client_code: crate::client_code::ClientCode,
    /// Add-On items' skins, drawn over every copy of them.
    pub(super) item_skins: crate::item_skins::ItemSkins,
    /// Every enabled package including server behaviour, for hosting.
    pub(super) server_packages: Option<Arc<bri_package_runtime::Catalog>>,
    /// The loaded Add-Ons are this player's own choice (packages.json, the
    /// Add-Ons screen, `enable_packages` or `apply_packages`), so hosting
    /// runs them as they are. False after a join loaded another server's.
    pub(super) packages_from_tools: bool,
    /// The next join keeps the loaded content even when the server's
    /// Add-Ons bring bricks, weapons or vehicles: loading them failed, so
    /// the player joins without them rather than not at all.
    pub(super) skip_add_on_reload: bool,
    pub(super) package_models: crate::packages::PackageModels,
    /// Add-On import in progress: request, row id and the worker's answer.
    /// Converting the Add-Ons folder (`add_ons::start_sync`).
    pub(super) add_on_sync: Option<mpsc::Receiver<crate::add_ons::SyncNote>>,
    /// Hosting or joining asked for while [`Self::add_on_sync`] runs: it
    /// starts once the conversions are done, with the lists they leave, so
    /// no game's Add-Ons change under it.
    pub(super) after_sync: Option<ReloadResume>,
    /// The Add-On list last asked for, the list that loaded without the
    /// Add-Ons that broke it, and why each was left out.
    pub(super) left_out_add_ons: Option<(
        bri_package::packages::PackageSet,
        bri_package::packages::PackageSet,
        Vec<String>,
    )>,
    /// What the enabled Add-Ons name that could not be found or used.
    pub(super) add_on_health: crate::add_on_health::AddOnHealth,
}

impl App {
    /// Chat lines (and an optional question) for a host notice.
    pub(super) fn host_notice(&mut self, notice: HostNotice) -> Vec<(String, Option<UiUpdate>)> {
        match notice {
            HostNotice::Reach(report) => {
                bri_console::echo(format!("Hosting check: {report:?}"));
                let mut lines: Vec<_> = report.lines().into_iter().map(|l| (l, None)).collect();
                if let Some(invite) = report.invite.clone()
                    && matches!(
                        report.verdict,
                        bri_net::reach::Verdict::Reachable | bri_net::reach::Verdict::Likely
                    )
                {
                    let copied = copy_to_clipboard(&invite).is_ok();
                    self.net.invite = Some(invite);
                    lines.push((
                        if copied {
                            "Your invite is on the clipboard; paste it to friends. Type /invite to copy it again.".into()
                        } else {
                            "Type /invite to copy your invite.".into()
                        },
                        None,
                    ));
                } else if let Some(invite) = report.invite.or(report.lan_invite) {
                    // Without a public address, the home network invite is
                    // still something to copy.
                    self.net.invite = Some(invite);
                    lines.push(("Type /invite to copy an invite.".into(), None));
                }
                lines
            }
            HostNotice::Lan { invite } => {
                self.net.invite = Some(invite);
                vec![(
                    "Players on your network see this game in Join Server. Type /invite to copy an invite for them.".into(),
                    None,
                )]
            }
            HostNotice::Firewall { status, port } => match status.advice() {
                Some(advice) => vec![(
                    advice.to_string(),
                    Some(UiUpdate::Confirm {
                        title: "Windows Firewall".into(),
                        text: "Windows Firewall would stop friends from joining your game. Let Blockland ReImagined through? Windows will ask for permission once.".into(),
                        action: Box::new(UiAction::AllowFirewall { port }),
                    }),
                )],
                None => Vec::new(),
            },
        }
    }
    /// Enable mod packages from another root than the content root (tools
    /// and tests); replaces the packages loaded at startup. Their worlds
    /// join the Start Game list.
    pub fn enable_packages(
        &mut self,
        root: &std::path::Path,
        set: &bri_package::packages::PackageSet,
    ) -> Result<()> {
        ensure!(
            self.addons.reload.is_none(),
            "Add-On loading is still in progress"
        );
        let (client, problems) = crate::packages::load_set(root, set, false);
        let problems: Vec<String> = problems.iter().map(ToString::to_string).collect();
        ensure!(
            problems.is_empty(),
            "{}",
            problems.join(
                "
"
            )
        );
        let (server, problems) = crate::packages::load_set(root, set, true);
        let problems: Vec<String> = problems.iter().map(ToString::to_string).collect();
        ensure!(
            problems.is_empty(),
            "{}",
            problems.join(
                "
"
            )
        );
        self.content.maps.retain(|m| !m.id.contains(':'));
        if let Some(catalog) = &server {
            let worlds = crate::packages::world_maps(catalog, &self.content.maps);
            self.content.maps.extend(worlds);
        }
        self.ui.apply(UiUpdate::Maps(self.content.maps.clone()));
        self.ui
            .apply(UiUpdate::GameModes(crate::packages::modes(server.as_ref())));
        self.addons.package_catalog = client;
        self.addons.server_packages = server;
        self.addons.client_code = crate::client_code::ClientCode::load(root, set);
        self.addons.packages_from_tools = true;
        Ok(())
    }
    /// The Add-Ons screen changed which Add-Ons are on: the next game uses
    /// the new list, with no restart.
    /// Convert what is new or changed in the Add-Ons folder, and remove
    /// what was taken out, unless that is already under way.
    pub(super) fn sync_add_ons(&mut self) -> Result<()> {
        if self.addons.add_on_sync.is_none() {
            let importer = crate::add_ons::importer()?;
            self.addons.add_on_sync = Some(crate::add_ons::start_sync(
                &self.content.paths.root,
                &importer,
                self.net.attempt.is_some(),
            )?);
        }
        Ok(())
    }
    /// The Add-Ons folder's conversions are done: the game asked for
    /// meanwhile loads the lists they left, then starts.
    pub(super) fn resume_after_sync(&mut self) {
        let Some(resume) = self.addons.after_sync.take() else {
            return;
        };
        let id = match &resume {
            ReloadResume::Action { id, .. } | ReloadResume::Downloaded { id, .. } => *id,
        };
        if self.ui.session_request() != Some(id) {
            // Cancelled while it waited.
            return;
        }
        let result = bri_package::packages::PackageSet::load_root(&self.content.paths.root)
            .and_then(|set| self.queue_package_reload(set, None, Some(resume)));
        if let Err(error) = result {
            self.answer(id, Err(error));
        }
    }
    /// The lists changed: show them now and load them later
    /// ([`UiAction::ApplyAddOns`], or hosting), never on the click.
    pub(super) fn add_ons_listed(&mut self, view: AddOnsView) {
        self.addons.packages_from_tools = false;
        self.show_add_ons(view);
    }
    pub(super) fn add_ons_changed(&mut self, mut view: AddOnsView) {
        self.addons.packages_from_tools = false;
        if self.net.attempt.is_some() {
            // A game keeps the Add-Ons it started with; the next one loads
            // the lists as they are then.
            view.notice = format!(
                "{} Changes apply the next time you start a game.",
                view.notice
            )
            .trim_start()
            .to_string();
            self.show_add_ons(view);
            return;
        }
        let root = self.content.paths.root.clone();
        let applied = bri_package::packages::PackageSet::load_root(&root)
            .and_then(|set| self.queue_package_reload(set, None, None));
        if let Err(error) = applied {
            bri_console::warn(format!("Add-On change not applied: {error:#}"));
            view.notice = format!("{} It could not be loaded: {error:#}", view.notice);
        }
        self.show_add_ons(view);
    }
    /// Run with the Add-Ons `set` lists, loading again what depends on them:
    /// HUD panels, rules, game modes, worlds, bricks, weapons, items and
    /// vehicles. Also reloads reimports whose package list is unchanged.
    /// Synchronous for tools; UI actions queue preparation on a worker.
    /// Only between games; a game keeps what it started with.
    pub fn apply_packages(&mut self, set: &bri_package::packages::PackageSet) -> Result<()> {
        ensure!(
            self.net.attempt.is_none(),
            "Leave the game before changing Add-Ons"
        );
        ensure!(
            self.addons.reload.is_none(),
            "Add-On loading is still in progress"
        );
        let prepared = PreparedPackages::load(
            &self.content.paths.root,
            set,
            &self.state_dir,
            self.audio.sound_bank(),
        )?;
        self.install_packages(set, prepared);
        Ok(())
    }

    fn install_packages(
        &mut self,
        requested: &bri_package::packages::PackageSet,
        prepared: PreparedPackages,
    ) {
        let PreparedPackages {
            content,
            parts,
            avatar_assets,
            sounds,
            client,
            server,
            code,
            mut problems,
            left_out,
            mut health,
        } = prepared;
        let dir = content.paths.ui_pack.clone();
        let content = content.map_ui(|data| Rc::new(bri_ui::pack::Pack::from_parts(data, dir)));
        self.addons.left_out_add_ons = if left_out.is_empty() {
            None
        } else {
            self.notify_left_out_add_ons(&left_out);
            Some((requested.clone(), content.paths.packages.clone(), left_out))
        };
        self.fx.weapon_effects = parts.weapon_effects;
        self.fx.actor_effects = parts.actor_effects;
        // Texture indices belong to this exact pack. Even an equally sized
        // replacement can reorder textures or change their pixels.
        self.gpu.effects_renderer = None;
        self.fx.explosion_shapes = parts.explosion_shapes;
        self.fx.explosion_debris = parts.explosion_debris;
        self.build.tool_ui = parts.tool_ui;
        self.item_assets = parts.item_assets;
        self.item_ui = parts.item_ui;
        self.vehicle_assets = parts.vehicle_assets;
        self.world_items = parts.world_items;
        self.avatar.avatar_assets = avatar_assets;
        self.avatar.avatars.clear();
        let casing_problems = self
            .fx
            .weapon_shells
            .set_casings(&content.weapons.pack, |m| self.world_items.has_model(m));
        for mut problem in casing_problems.iter().cloned() {
            problem.add_on = health.owners.id(&problem.add_on);
            if health.health.note(problem.clone()) {
                bri_console::warn(format!(
                    "Add-On {}: {}",
                    health.owners.name(&problem.add_on),
                    problem.line()
                ));
            }
        }
        problems.extend(casing_problems);
        self.content_problems = problems;
        self.ui.core.pack = content.ui_pack.clone();
        self.audio.install_pack_sounds(sounds);
        self.content = content;
        self.files.saves = crate::saves::Store::new(
            &self.state_dir,
            &self.content,
            Some(self.files.old_saves.clone()),
        );
        if self.files.old_saves_started {
            self.start_old_saves();
        }
        self.ui.apply(UiUpdate::Maps(self.content.maps.clone()));
        self.ui
            .apply(UiUpdate::GameModes(crate::packages::modes(server.as_ref())));
        self.ui
            .apply(UiUpdate::Datablocks(self.content.datablocks.clone()));
        self.addons.package_catalog = client;
        self.addons.server_packages = server;
        self.addons.client_code = code;
        self.addons.packages_from_tools = true;
        self.addons.add_on_health = health;
        self.write_add_on_health();
        if self.ui.is_open(ScreenId::AddOns) {
            self.show_add_ons(self.ui.core.add_ons.clone());
        }
    }

    /// One worker at a time. A newer selection supersedes preparation still
    /// running, while carrying the actions waiting for it to the latest job.
    pub(super) fn queue_package_reload(
        &mut self,
        set: bri_package::packages::PackageSet,
        waiter: Option<RequestId>,
        resume: Option<ReloadResume>,
    ) -> Result<()> {
        ensure!(
            self.net.attempt.is_none(),
            "Leave the game before changing Add-Ons"
        );
        if let Some(ref resume) = resume {
            let id = match resume {
                ReloadResume::Action { id, .. } | ReloadResume::Downloaded { id, .. } => *id,
            };
            self.ui.apply_session(
                id,
                UiUpdate::Connection(ConnectionState::Connecting {
                    text: "Loading Add-Ons…".into(),
                }),
            );
        }
        let downloaded_for = match &resume {
            Some(ReloadResume::Downloaded { id, .. }) => Some(*id),
            _ => None,
        };
        let mut request = ReloadRequest {
            set,
            downloaded_for,
            waiters: waiter.into_iter().collect(),
            resume,
        };
        if let Some(job) = &mut self.addons.reload {
            if let Some(pending) = self.addons.reload_pending.take() {
                request.inherit(pending);
            } else {
                request.waiters.append(&mut job.request.waiters);
                request.inherit_resume(job.request.resume.take());
            }
            self.addons.reload_pending = Some(request);
        } else {
            self.start_package_reload(request);
        }
        Ok(())
    }
    fn start_package_reload(&mut self, request: ReloadRequest) {
        let root = self.content.paths.root.clone();
        let set = request.set.clone();
        let state = self.state_dir.clone();
        let bank = self.audio.sound_bank();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result =
                PreparedPackages::load(&root, &set, &state, bank).map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
        });
        self.addons.reload = Some(ReloadJob {
            receiver: rx,
            request,
        });
    }
    pub(super) fn poll_package_reload(&mut self) {
        let Some(job) = &self.addons.reload else {
            return;
        };
        let Some(result) = finished(&job.receiver, "Add-On loading") else {
            return;
        };
        let job = self.addons.reload.take().expect("polled job");
        if let Some(request) = self.addons.reload_pending.take() {
            self.start_package_reload(request);
            return;
        }
        let result = result.and_then(|r| r.map_err(anyhow::Error::msg));
        let request = job.request;
        // A cancelled remote join must not replace the player's own content.
        if request
            .downloaded_for
            .is_some_and(|id| self.ui.session_request() != Some(id))
        {
            return;
        }
        let error = result
            .as_ref()
            .err()
            .map(|e| format!("Your Add-On changes could not be loaded: {e:#}"));
        if let Ok(prepared) = result {
            self.install_packages(&request.set, prepared);
            if request.downloaded_for.is_none() {
                // The screen may have changed again while this worker ran.
                // Explicit synchronous tool choices do not use this check.
                self.addons.packages_from_tools =
                    bri_package::packages::PackageSet::load_root(&self.content.paths.root)
                        .is_ok_and(|current| current == request.set);
            }
        }
        let had_waiters = !request.waiters.is_empty();
        for id in request.waiters {
            self.answer(
                id,
                error
                    .as_ref()
                    .map_or(Ok(()), |e| Err(anyhow::anyhow!("{e}"))),
            );
        }
        match request.resume {
            Some(ReloadResume::Action { id, action }) if self.ui.session_request() == Some(id) => {
                if let Some(error) = error {
                    self.answer(id, Err(anyhow::anyhow!(error)));
                } else if let Err(error) = self.dispatch_action(id, *action, &mut Vec::new()) {
                    self.answer(id, Err(error));
                }
            }
            Some(ReloadResume::Downloaded { id, address }) => {
                self.addons.packages_from_tools = false;
                if let Some(error) = error {
                    let text = format!(
                        "Some of this server's Add-Ons could not be loaded on this computer, so you joined without them: {error}"
                    );
                    bri_console::warn(&text);
                    self.net.join_notices.push(text);
                    self.addons.skip_add_on_reload = true;
                }
                if let Err(error) = self.join(id, address, String::new()) {
                    self.net.join_notices.clear();
                    self.answer(id, Err(error));
                }
            }
            _ => {
                if let Some(error) = error {
                    bri_console::warn(&error);
                    if !had_waiters {
                        self.ui.apply(UiUpdate::MessageBox {
                            title: "Add-On Changes Could Not Load".into(),
                            text: error,
                        });
                    }
                }
            }
        }
    }
    /// Gather what the enabled Add-Ons name that this computer could not
    /// find or use ([`crate::add_on_health`]): what loading reported, the
    /// Add-Ons left out, `rules` (their rules, HUD and modes left out), the
    /// companions and dependencies each needs, and every weapon reference
    /// checked against what loaded. Logs each problem once and writes
    /// `logs/add-on-health.json`.
    pub(super) fn check_add_ons(&mut self, rules: &[bri_package::diag::Diagnostic]) {
        let root = self.content.paths.root.clone();
        let (requested, left_out) = match &self.addons.left_out_add_ons {
            Some((requested, _, left_out)) => (requested.clone(), left_out.clone()),
            None => (self.content.paths.packages.clone(), Vec::new()),
        };
        let mut problems = self.content_problems.clone();
        problems.extend(
            left_out
                .iter()
                .map(|l| crate::add_on_health::left_out_problem(l)),
        );
        problems.extend(rules.iter().map(crate::add_on_health::rules_problem));
        problems.extend(bri_package::health::check_set(&root, &requested));
        problems.extend(crate::add_on_health::check_references(
            &crate::add_on_health::Loaded {
                weapons: &self.content.weapons.pack,
                effects: &self.fx.weapon_effects,
                items: &self.item_assets,
                audio: Some(&self.audio),
            },
        ));
        let owners = crate::add_on_health::Owners::new(&root, &requested);
        self.addons.add_on_health = crate::add_on_health::AddOnHealth::new(owners, problems);
        self.write_add_on_health();
    }
    fn write_add_on_health(&self) {
        match self.addons.add_on_health.write_report(&self.state_dir) {
            Ok(path) => {
                if let Some(summary) = self.addons.add_on_health.summary() {
                    bri_console::warn(format!("{summary} ({})", path.display()));
                }
            }
            Err(error) => bri_console::warn(format!("Add-On health report not written: {error:#}")),
        }
    }
    /// Show the Add-Ons screen's `view` with each Add-On's problems from
    /// the last load listed under it.
    pub(super) fn show_add_ons(&mut self, mut view: AddOnsView) {
        crate::add_ons::with_health(&mut view, &self.addons.add_on_health);
        self.ui.apply(UiUpdate::AddOns(view));
    }
    /// What the enabled Add-Ons name that this computer could not find
    /// or use, as of the last load.
    pub fn add_on_health(&self) -> &crate::add_on_health::AddOnHealth {
        &self.addons.add_on_health
    }
    /// Tell the player which Add-Ons were left out and why: the game runs
    /// without them rather than not at all.
    pub(super) fn notify_left_out_add_ons(&mut self, left_out: &[String]) {
        for line in left_out {
            bri_console::warn(format!("Add-On left out: {line}"));
        }
        self.ui.apply(UiUpdate::MessageBox {
            title: "Add-Ons Left Out".into(),
            text: format!(
                "These Add-Ons could not be loaded, so the game started without them. Turn them off or fix them in Add-Ons.\n\n{}",
                left_out.join("\n")
            ),
        });
    }
    /// Package HUD panels and keys from the latest replicated state.
    pub(super) fn update_package_hud(&mut self) {
        let view = self
            .net
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref());
        let Some(view) = view else {
            self.ui.core.package_panels.clear();
            self.ui.core.package_keys.clear();
            self.ui.set_package_binds(Vec::new());
            self.ui.core.addon_help.clear();
            return;
        };
        let Some(catalog) = packages_for(&self.addons.package_catalog, view) else {
            self.ui.core.package_panels.clear();
            self.ui.core.package_keys.clear();
            self.ui.set_package_binds(Vec::new());
            self.ui.core.addon_help.clear();
            return;
        };
        let mac = self.ui.core.platform == Platform::MacOs;
        let binds = crate::packages::binds(catalog, &view.package_state, mac);
        self.ui.set_package_binds(binds);
        let help = crate::packages::help(catalog, &view.package_state);
        if self.ui.core.addon_help != help {
            self.ui.core.addon_help = help;
            self.ui.welcome_addons();
        }
        let binds = &self.ui.core.binds;
        let held = view
            .weapons
            .images
            .get(&view.owner)
            .and_then(|images| images.iter().find(|i| i.hand == 0))
            .map_or("", |i| i.image.as_str());
        let (panels, keys) =
            crate::packages::panels(catalog, &view.package_state, view.owner, held, |letter| {
                binds
                    .command_for_key(
                        bri_ui::input::Key::Letter(letter),
                        bri_ui::input::Modifiers::NONE,
                    )
                    .is_some()
            });
        self.ui.core.package_panels = panels;
        self.ui.core.package_keys = keys;
    }
}

pub(super) enum ReloadResume {
    Action {
        id: RequestId,
        action: Box<UiAction>,
    },
    Downloaded {
        id: RequestId,
        address: String,
    },
}
pub(super) struct ReloadRequest {
    set: bri_package::packages::PackageSet,
    /// Destination of the prepared content, independent of any inherited action.
    /// A local selection never becomes server content by inheriting a join.
    downloaded_for: Option<RequestId>,
    waiters: Vec<RequestId>,
    resume: Option<ReloadResume>,
}
impl ReloadRequest {
    fn inherit(&mut self, mut older: Self) {
        self.waiters.append(&mut older.waiters);
        self.inherit_resume(older.resume);
    }
    fn inherit_resume(&mut self, older: Option<ReloadResume>) {
        if self.resume.is_some() {
            return;
        }
        self.resume = match older {
            Some(continuation @ ReloadResume::Downloaded { id, .. })
                if self.downloaded_for == Some(id) =>
            {
                Some(continuation)
            }
            Some(continuation @ ReloadResume::Action { .. }) if self.downloaded_for.is_none() => {
                Some(continuation)
            }
            _ => None,
        };
    }
}
pub(super) struct ReloadJob {
    receiver: mpsc::Receiver<std::result::Result<PreparedPackages, String>>,
    request: ReloadRequest,
}
struct PreparedPackages {
    content: ClientContent<bri_ui::schema::UiPack>,
    parts: ContentParts,
    avatar_assets: Arc<crate::avatar::AvatarAssets>,
    sounds: crate::audio::PreparedSounds,
    client: Option<Arc<bri_package_runtime::Catalog>>,
    server: Option<Arc<bri_package_runtime::Catalog>>,
    code: crate::client_code::ClientCode,
    problems: Vec<bri_package::health::Problem>,
    left_out: Vec<String>,
    health: crate::add_on_health::AddOnHealth,
}
impl PreparedPackages {
    fn load(
        root: &Path,
        requested: &bri_package::packages::PackageSet,
        state: &Path,
        bank: Arc<bri_audio::SoundBank>,
    ) -> Result<Self> {
        let (loaded, mut problems) = crate::add_on_health::collecting(|| {
            ClientContent::load_leaving_out_broken(root, requested)
        });
        let (mut content, left_out) = loaded?;
        let effects = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
        let (parts, more) = crate::add_on_health::collecting(|| {
            ContentParts::build(&content, effects, &state.join(ITEM_ICONS))
        });
        problems.extend(more);
        let parts = parts?;
        let (avatar_assets, more) = crate::add_on_health::collecting(|| -> Result<_> {
            let mut assets = crate::avatar::AvatarAssets::load(&content.paths.avatar)?;
            assets.load_horse(&content.paths.vehicles)?;
            assets.load_bodies(&content.paths.root, &content.paths.packages);
            Ok(Arc::new(assets))
        });
        problems.extend(more);
        let avatar_assets = avatar_assets?;
        let set = &content.paths.packages;
        let (client, mut rules) = crate::packages::load_set(root, set, false);
        let (server, more) = crate::packages::load_set(root, set, true);
        rules.extend(more);
        content.maps.retain(|m| !m.id.contains(':'));
        if let Some(catalog) = &server {
            content
                .maps
                .extend(crate::packages::world_maps(catalog, &content.maps));
        }
        let code = crate::client_code::ClientCode::load(root, set);
        let sounds =
            crate::audio::PreparedSounds::load(bank, &content.weapons.pack, &content.paths.weapons);
        let mut health_problems = problems.clone();
        health_problems.extend(
            left_out
                .iter()
                .map(|l| crate::add_on_health::left_out_problem(l)),
        );
        health_problems.extend(rules.iter().map(crate::add_on_health::rules_problem));
        health_problems.extend(bri_package::health::check_set(root, requested));
        health_problems.extend(crate::add_on_health::check_references(
            &crate::add_on_health::Loaded {
                weapons: &content.weapons.pack,
                effects: &parts.weapon_effects,
                items: &parts.item_assets,
                audio: Some(&sounds),
            },
        ));
        let health = crate::add_on_health::AddOnHealth::new(
            crate::add_on_health::Owners::new(root, requested),
            health_problems,
        );
        // UI caches stay on their owning thread. Only the authored schema crosses.
        let content = content.map_ui(|pack| {
            Rc::try_unwrap(pack)
                .map(|p| p.data)
                .unwrap_or_else(|p| p.data.clone())
        });
        Ok(Self {
            content,
            parts,
            avatar_assets,
            sounds,
            client,
            server,
            code,
            problems,
            left_out,
            health,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::content_root::ContentRoot;

    fn app() -> Result<(ContentRoot, crate::testing::ScratchDir, Box<App>)> {
        let content = ContentRoot::synthetic()?;
        let state = content.state()?;
        let app = App::load(&content.root, state.path(), (320, 240))?;
        Ok((content, state, app))
    }
    fn held_job(
        app: &mut App,
        resume: Option<ReloadResume>,
    ) -> mpsc::Sender<std::result::Result<PreparedPackages, String>> {
        let (tx, receiver) = mpsc::channel();
        let downloaded_for = match &resume {
            Some(ReloadResume::Downloaded { id, .. }) => Some(*id),
            _ => None,
        };
        app.addons.reload = Some(ReloadJob {
            receiver,
            request: ReloadRequest {
                set: app.content.paths.packages.clone(),
                downloaded_for,
                waiters: vec![101],
                resume,
            },
        });
        tx
    }
    fn wait(app: &mut App) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while app.addons.reload.is_some() {
            app.poll_package_reload();
            assert!(
                std::time::Instant::now() < deadline,
                "reload never completed"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn rename_map(app: &App, name: &str) -> Result<()> {
        let path = app.content.paths.map_bundle.join("bundle.json");
        let mut data: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        data["maps"][0]["name"] = name.into();
        std::fs::write(path, serde_json::to_vec(&data)?)?;
        Ok(())
    }

    #[test]
    fn installed_effects_replace_the_atlas_but_failed_and_cancelled_reloads_do_not() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        assert!(app.gpu.effects_renderer.is_none());
        app.rebuild_effects_renderer(&device, &queue, format)?;
        let old_pack = app.fx.weapon_effects.world().pack().clone();

        let tx = held_job(&mut app, None);
        tx.send(Err("failed preparation".into())).ok();
        app.poll_package_reload();
        assert!(app.gpu.effects_renderer.is_some());
        assert!(Arc::ptr_eq(&old_pack, app.fx.weapon_effects.world().pack()));

        let set = app.content.paths.packages.clone();
        let prepared = || {
            PreparedPackages::load(
                &app.content.paths.root,
                &set,
                &app.state_dir,
                app.audio.sound_bank(),
            )
        };
        let cancelled = prepared()?;
        let mut replacement = prepared()?;
        let id = app.ui.core.request(UiAction::JoinServer {
            address: "localhost:28000".into(),
            password: String::new(),
        });
        let tx = held_job(
            &mut app,
            Some(ReloadResume::Downloaded {
                id,
                address: "localhost:28000".into(),
            }),
        );
        app.ui.core.request(UiAction::CancelConnect);
        tx.send(Ok(cancelled)).ok();
        app.poll_package_reload();
        assert!(app.gpu.effects_renderer.is_some());
        assert!(Arc::ptr_eq(&old_pack, app.fx.weapon_effects.world().pack()));

        // A decoded Add-On texture appended to the base pack has a valid CPU
        // index which the already initialized GPU atlas cannot contain.
        let base = bri_fx_runtime::EffectsPack::load(&app.content.paths.effects_runtime)?;
        let mut library = base.library.clone();
        library
            .textures
            .insert("reload-icon".into(), "reload-icon.png".into());
        let mut textures: Vec<_> = base
            .textures
            .iter()
            .map(|t| bri_fx_runtime::pack::TextureImage {
                id: t.id.clone(),
                width: t.width,
                height: t.height,
                rgba: t.rgba.clone(),
            })
            .collect();
        textures.push(bri_fx_runtime::pack::TextureImage {
            id: "reload-icon".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        });
        let pack =
            bri_fx_runtime::EffectsPack::from_parts(library, base.manifest.clone(), textures)?;
        let augmented_base = pack.clone();
        replacement.parts.weapon_effects = crate::weapon_effects::WeaponEffects::new(
            pack,
            Arc::new(replacement.content.weapons.pack.clone()),
            Default::default(),
        )?;
        let texture = replacement
            .parts
            .weapon_effects
            .world()
            .pack()
            .textures
            .iter()
            .position(|t| t.id == "reload-icon")
            .unwrap() as u32;
        assert!(texture as usize >= old_pack.textures.len());
        let camera = bri_fx_runtime::Camera {
            view_projection: glam::Mat4::IDENTITY,
            position: Vec3::Z * 2.,
            right: Vec3::X,
            up: Vec3::Y,
        };
        let effects = bri_fx_runtime::FrameEffects {
            particles: vec![bri_fx_runtime::ParticleInstance {
                position: Vec3::ZERO,
                size: 1.,
                color: glam::Vec4::ONE,
                spin: 0.,
                axis: Vec3::ZERO,
                texture,
                blend: bri_fx_runtime::BlendMode::Alpha,
                depth_test: true,
            }],
            lights: Vec::new(),
        };
        let stale = app
            .gpu
            .effects_renderer
            .as_mut()
            .unwrap()
            .prepare(&queue, &camera, &effects)
            .unwrap_err();
        assert!(stale.to_string().contains("Invalid effects instance"));
        app.install_packages(&set, replacement);
        assert!(
            app.gpu.effects_renderer.is_none(),
            "successful installation must invalidate the exact old atlas"
        );
        app.rebuild_effects_renderer(&device, &queue, format)?;
        assert_eq!(
            app.gpu
                .effects_renderer
                .as_mut()
                .unwrap()
                .prepare(&queue, &camera, &effects)?
                .instances,
            1
        );

        // Equal layer counts are not a proof of texture identity. A normal
        // reimport/install must invalidate even when the pack list is unchanged.
        let mut replacement = PreparedPackages::load(
            &app.content.paths.root,
            &set,
            &app.state_dir,
            app.audio.sound_bank(),
        )?;
        let mut textures: Vec<_> = augmented_base
            .textures
            .iter()
            .map(|t| bri_fx_runtime::pack::TextureImage {
                id: t.id.clone(),
                width: t.width,
                height: t.height,
                rgba: t.rgba.clone(),
            })
            .collect();
        let icon = textures.iter_mut().find(|t| t.id == "reload-icon").unwrap();
        assert_eq!(icon.rgba, [255; 4]);
        icon.rgba = vec![64, 128, 32, 255];
        let changed_pixels = bri_fx_runtime::EffectsPack::from_parts(
            augmented_base.library.clone(),
            augmented_base.manifest.clone(),
            textures,
        )?;
        replacement.parts.weapon_effects = crate::weapon_effects::WeaponEffects::new(
            changed_pixels,
            Arc::new(replacement.content.weapons.pack.clone()),
            Default::default(),
        )?;
        assert_eq!(
            replacement
                .parts
                .weapon_effects
                .world()
                .pack()
                .textures
                .len(),
            app.fx.weapon_effects.world().pack().textures.len()
        );
        app.install_packages(&set, replacement);
        assert!(app.gpu.effects_renderer.is_none());
        assert_eq!(
            app.fx.weapon_effects.world().pack().textures[texture as usize].rgba,
            [64, 128, 32, 255]
        );
        app.rebuild_effects_renderer(&device, &queue, format)?;
        assert_eq!(
            app.gpu
                .effects_renderer
                .as_mut()
                .unwrap()
                .prepare(&queue, &camera, &Default::default())?
                .instances,
            0
        );
        Ok(())
    }

    #[test]
    fn enabling_then_disabling_an_add_on_reloads_body_models() -> Result<()> {
        let (content, _state, mut app) = app()?;
        let dir = content.root.join("addons/fish");
        std::fs::create_dir_all(dir.join("assets/archetypes"))?;
        let rig = crate::testing::avatar::rig();
        let mut shape = rig.shape.clone();
        shape.animations = rig.sequences.values().cloned().collect();
        std::fs::write(
            dir.join("assets/fish.shape.json"),
            serde_json::to_vec(&shape)?,
        )?;
        let mut assets = vec![
            serde_json::json!({"kind":"asset", "id":"fish:asset/fish.dts", "file":"assets/fish.shape.json"}),
        ];
        for material in &shape.materials {
            let file = format!("assets/{}.png", material.name);
            image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])).save(dir.join(&file))?;
            assets.push(serde_json::json!({"kind":"asset", "id":format!("fish:asset/{}.png",material.name), "file":file}));
        }
        std::fs::write(
            dir.join("assets/content.json"),
            serde_json::to_vec(&serde_json::json!({"content":assets}))?,
        )?;
        std::fs::write(
            dir.join("assets/archetypes/fishbot.json"),
            br#"{"schema_version":1,"name":"Fish","model":"fish:asset/fish.dts"}"#,
        )?;
        std::fs::write(
            dir.join("package.json"),
            br#"{"schema_version":1,"id":"fish","version":"1.0.0","api":1,"name":"Fish","license":"CC0-1.0"}"#,
        )?;
        let original = app.content.paths.packages.clone();
        let mut selected = original.clone();
        selected.packages.push(serde_json::from_value(
            serde_json::json!({"id":"fish","version":"1.0.0","side":"shared","dir":"addons/fish"}),
        )?);
        assert!(!app.avatar.avatar_assets.has_body("fish:asset/fish.dts"));
        app.apply_packages(&selected)?;
        assert!(app.avatar.avatar_assets.has_body("fish:asset/fish.dts"));
        let mesh = app.avatar.avatar_assets.body_mesh(
            "fish:asset/fish.dts",
            app.avatar.avatar_assets.package.defaults.clone(),
        )?;
        app.avatar.avatars.insert(100, mesh);
        app.apply_packages(&original)?;
        assert!(!app.avatar.avatar_assets.has_body("fish:asset/fish.dts"));
        assert!(
            app.avatar.avatars.is_empty(),
            "old body meshes cannot survive reload"
        );
        Ok(())
    }

    #[test]
    fn a_pending_worker_keeps_the_ui_available_and_only_latest_choices_install() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let tx = held_job(&mut app, None);
        app.poll_package_reload(); // Empty receiver returns immediately.
        assert!(app.addons.reload.is_some());
        let set = app.content.paths.packages.clone();
        app.queue_package_reload(set.clone(), Some(102), None)?;
        app.queue_package_reload(set.clone(), Some(103), None)?;
        let pending = app.addons.reload_pending.as_ref().unwrap();
        assert_eq!(pending.waiters, [103, 102, 101]);
        rename_map(&app, "Newest authored map")?;
        tx.send(Err("superseded worker must not report this failure".into()))
            .ok();
        wait(&mut app);
        assert_eq!(app.content.maps[0].name, "Newest authored map");
        assert_eq!(app.content.paths.packages, set);
        Ok(())
    }

    #[test]
    fn cancelled_downloads_never_install_server_content() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let original = app.content.maps[0].name.clone();
        let id = app.ui.core.request(UiAction::JoinServer {
            address: "localhost:28000".into(),
            password: String::new(),
        });
        let tx = held_job(
            &mut app,
            Some(ReloadResume::Downloaded {
                id,
                address: "localhost:28000".into(),
            }),
        );
        rename_map(&app, "Server's content")?;
        let prepared = PreparedPackages::load(
            &app.content.paths.root,
            &app.content.paths.packages,
            &app.state_dir,
            app.audio.sound_bank(),
        )?;
        app.ui.core.request(UiAction::CancelConnect);
        tx.send(Ok(prepared)).ok();
        app.poll_package_reload();
        assert_eq!(app.content.maps[0].name, original);
        assert!(app.net.attempt.is_none());
        assert!(app.addons.reload.is_none());
        Ok(())
    }

    #[test]
    fn a_worker_that_stops_or_fails_preserves_loaded_content_and_can_retry() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let original = app.content.maps[0].name.clone();
        let tx = held_job(&mut app, None);
        drop(tx);
        app.poll_package_reload();
        assert!(app.addons.reload.is_none());
        assert_eq!(app.content.maps[0].name, original);
        let tx = held_job(&mut app, None);
        tx.send(Err("broken new pack".into())).ok();
        app.poll_package_reload();
        assert_eq!(app.content.maps[0].name, original);
        rename_map(&app, "Reimported without a package-list change")?;
        app.queue_package_reload(app.content.paths.packages.clone(), None, None)?;
        wait(&mut app);
        assert_eq!(
            app.content.maps[0].name,
            "Reimported without a package-list change"
        );
        Ok(())
    }

    #[test]
    fn a_cancelled_host_waiting_for_reload_does_not_start_a_server() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let tx = held_job(&mut app, None);
        let action = UiAction::HostGame {
            map: app.content.maps[0].id.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 8,
            server_name: "Test".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        };
        let id = app.ui.core.request(action.clone());
        app.dispatch_action(id, action, &mut Vec::new())?;
        assert!(app.net.attempt.is_none());
        assert!(matches!(
            app.addons.reload_pending.as_ref().unwrap().resume,
            Some(ReloadResume::Action { .. })
        ));
        app.ui.core.request(UiAction::CancelConnect);
        tx.send(Err("old preparation".into())).ok();
        wait(&mut app);
        assert!(app.net.attempt.is_none());
        Ok(())
    }
    #[test]
    fn local_choices_superseding_a_cancelled_download_install_and_answer_every_waiter() -> Result<()>
    {
        let (_content, _state, mut app) = app()?;
        let remote = app.ui.core.request(UiAction::JoinServer {
            address: "localhost:28000".into(),
            password: String::new(),
        });
        let tx = held_job(
            &mut app,
            Some(ReloadResume::Downloaded {
                id: remote,
                address: "localhost:28000".into(),
            }),
        );
        app.ui.core.request(UiAction::CancelConnect);
        let set = app.content.paths.packages.clone();
        let first = app
            .ui
            .core
            .request_pending(UiAction::ApplyAddOns, bri_ui::ui::Pending::Other);
        app.queue_package_reload(set.clone(), Some(first), None)?;
        let latest = app
            .ui
            .core
            .request_pending(UiAction::ApplyAddOns, bri_ui::ui::Pending::Other);
        app.queue_package_reload(set.clone(), Some(latest), None)?;
        let pending = app.addons.reload_pending.as_ref().unwrap();
        assert!(pending.downloaded_for.is_none());
        assert!(pending.resume.is_none());
        rename_map(&app, "Latest local choice after cancelled remote join")?;
        tx.send(Err("cancelled remote preparation".into())).ok();
        wait(&mut app);
        assert_eq!(
            app.content.maps[0].name,
            "Latest local choice after cancelled remote join"
        );
        assert_eq!(app.content.paths.packages, set);
        assert!(!app.ui.core.pending.contains_key(&first));
        assert!(!app.ui.core.pending.contains_key(&latest));
        assert!(app.net.attempt.is_none());
        assert!(app.addons.packages_from_tools);
        Ok(())
    }

    #[test]
    fn failed_server_reload_rejoins_with_the_loaded_content_and_reports_the_fallback() -> Result<()>
    {
        let (_content, _state, mut app) = app()?;
        let original = app.content.maps[0].name.clone();
        let id = app.ui.core.request(UiAction::JoinServer {
            address: "127.0.0.1:9".into(),
            password: String::new(),
        });
        let tx = held_job(
            &mut app,
            Some(ReloadResume::Downloaded {
                id,
                address: "127.0.0.1:9".into(),
            }),
        );
        tx.send(Err("damaged downloaded asset".into())).ok();
        app.poll_package_reload();
        assert_eq!(app.content.maps[0].name, original);
        assert!(app.net.attempt.is_some());
        assert!(!app.addons.packages_from_tools);
        assert!(
            app.net
                .join_notices
                .iter()
                .any(|line| line.contains("joined without them")
                    && line.contains("damaged downloaded asset"))
        );
        app.disconnect();
        Ok(())
    }
    #[test]
    fn explicit_tool_package_choices_are_preserved_even_without_editing_packages_json() -> Result<()>
    {
        let (_content, _state, mut app) = app()?;
        let configured = app.content.paths.packages.clone();
        let mut requested = configured.clone();
        requested.packages[0].version = "999.0.0".into();
        assert_ne!(requested, configured);
        app.apply_packages(&requested)?;
        assert_eq!(app.content.paths.packages, requested);
        assert!(app.addons.packages_from_tools);
        assert_eq!(
            bri_package::packages::PackageSet::load_root(&app.content.paths.root)?,
            configured
        );
        Ok(())
    }
    #[test]
    fn invalid_local_package_selection_answers_the_ui_without_aborting_its_pump() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let original = app.content.maps[0].name.clone();
        std::fs::write(
            app.content.paths.root.join("packages.json"),
            "not valid JSON",
        )?;
        app.addons.packages_from_tools = false;
        let id = app
            .ui
            .core
            .request_pending(UiAction::ApplyAddOns, bri_ui::ui::Pending::Other);
        app.dispatch_action(id, UiAction::ApplyAddOns, &mut Vec::new())?;
        assert!(!app.ui.core.pending.contains_key(&id));
        assert!(app.addons.reload.is_none());
        assert_eq!(app.content.maps[0].name, original);
        assert!(app.ui.is_open(ScreenId::MessageBox));
        Ok(())
    }

    /// The Add-Ons folder's conversions can replace or remove Add-Ons that
    /// are on: a game asked for meanwhile waits for them, as it waits for
    /// loading, and starts with the lists they leave.
    #[test]
    fn hosting_waits_for_the_add_ons_folder_to_finish_converting() -> Result<()> {
        let (_content, _state, mut app) = app()?;
        let (send, receive) = mpsc::channel();
        app.addons.add_on_sync = Some(receive);
        let action = UiAction::HostGame {
            map: app.content.maps[0].id.clone(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
            max_players: 8,
            server_name: "Test".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        };
        let id = app.ui.core.request(action.clone());
        app.dispatch_action(id, action, &mut Vec::new())?;
        assert!(app.net.attempt.is_none());
        assert!(
            app.addons.reload.is_none(),
            "nothing loads while the folder converts"
        );
        app.poll_background_jobs();
        assert!(app.addons.reload.is_none() && app.net.attempt.is_none());
        send.send(crate::add_ons::SyncNote {
            notice: String::new(),
            finished: true,
        })
        .ok();
        app.poll_background_jobs();
        assert!(app.addons.add_on_sync.is_none());
        let waiting = app.addons.reload_pending.as_ref().map(|r| &r.resume).or(app
            .addons
            .reload
            .as_ref()
            .map(|j| &j.request.resume));
        assert!(
            matches!(waiting, Some(Some(ReloadResume::Action { id: waiting, .. })) if *waiting == id),
            "the host starts once the lists are loaded"
        );
        // Not here: cancelled, it starts no server.
        app.ui.core.request(UiAction::CancelConnect);
        wait(&mut app);
        assert!(app.net.attempt.is_none());
        Ok(())
    }
}
