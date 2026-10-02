//! Add-On packages: enabling, applying and their HUD.
use super::*;

/// Add-On packages: the catalog, client code, server packages and imports.
pub(super) struct AddOns {
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
            )?);
        }
        Ok(())
    }
    /// The lists changed: show them now and load them later
    /// ([`UiAction::ApplyAddOns`], or hosting), never on the click.
    pub(super) fn add_ons_listed(&mut self, view: AddOnsView) {
        self.addons.packages_from_tools = false;
        self.show_add_ons(view);
    }
    pub(super) fn add_ons_changed(&mut self, mut view: AddOnsView) {
        self.addons.packages_from_tools = false;
        let root = self.content.paths.root.clone();
        let applied = bri_package::packages::PackageSet::load_root(&root)
            .and_then(|set| self.apply_packages(&set));
        if let Err(error) = applied {
            bri_console::warn(format!("Add-On change not applied: {error:#}"));
            view.notice = format!("{} It could not be loaded: {error:#}", view.notice);
        }
        self.show_add_ons(view);
    }
    /// Run with the Add-Ons `set` lists, loading again what depends on them:
    /// HUD panels, rules, game modes and worlds, and (when the list differs
    /// from the one loaded) bricks, weapons, items and vehicles. Only between
    /// games; a game in progress keeps what it started with.
    pub fn apply_packages(&mut self, set: &bri_package::packages::PackageSet) -> Result<()> {
        ensure!(
            self.net.attempt.is_none(),
            "Leave the game before changing Add-Ons"
        );
        let root = self.content.paths.root.clone();
        // An Add-On that broke loading this list before stays left out
        // without trying it again on every host.
        let known = self
            .addons
            .left_out_add_ons
            .as_ref()
            .is_some_and(|(requested, loaded, _)| {
                requested == set && *loaded == self.content.paths.packages
            });
        if *set != self.content.paths.packages && !known {
            let (loaded, mut collected) = crate::add_on_health::collecting(|| {
                ClientContent::load_leaving_out_broken(&root, set)
            });
            let (content, left_out) = loaded?;
            self.addons.left_out_add_ons = if left_out.is_empty() {
                None
            } else {
                self.notify_left_out_add_ons(&left_out);
                Some((set.clone(), content.paths.packages.clone(), left_out))
            };
            let effects_pack = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
            let (parts, more) = crate::add_on_health::collecting(|| {
                ContentParts::build(&content, effects_pack, &self.state_dir.join(ITEM_ICONS))
            });
            collected.extend(more);
            let parts = parts?;
            self.fx.weapon_effects = parts.weapon_effects;
            self.fx.actor_effects = parts.actor_effects;
            self.fx.explosion_shapes = parts.explosion_shapes;
            self.fx.explosion_debris = parts.explosion_debris;
            self.build.tool_ui = parts.tool_ui;
            self.item_assets = parts.item_assets;
            self.item_ui = parts.item_ui;
            self.vehicle_assets = parts.vehicle_assets;
            self.world_items = parts.world_items;
            let world_items = &self.world_items;
            collected.extend(
                self.fx
                    .weapon_shells
                    .set_casings(&content.weapons.pack, |m| world_items.has_model(m)),
            );
            self.content_problems = collected;
            self.ui.core.pack = content.ui_pack.clone();
            self.audio
                .set_pack_sounds(&content.weapons.pack, &content.paths.weapons);
            self.content = content;
        }
        // What actually loaded, less any Add-On left out above.
        let set = &self.content.paths.packages.clone();
        let (client, mut problems) = crate::packages::load_set(&root, set, false);
        let (server, more) = crate::packages::load_set(&root, set, true);
        problems.extend(more);
        self.content.maps.retain(|m| !m.id.contains(':'));
        if let Some(catalog) = &server {
            let worlds = crate::packages::world_maps(catalog, &self.content.maps);
            self.content.maps.extend(worlds);
        }
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
        self.addons.client_code = crate::client_code::ClientCode::load(&root, set);
        self.addons.packages_from_tools = true;
        self.check_add_ons(&problems);
        Ok(())
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
