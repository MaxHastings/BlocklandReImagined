//! Building tools and their dialogs.
use super::*;

/// Building: the brick hand and ghosts, tool dialogs and build macros.
pub(super) struct BuildState {
    /// Last ghost brick reported to the server, and when.
    pub(super) ghost_report: Option<(Option<bri_sim::session::GhostBrick>, std::time::Instant)>,
    /// Other players' ghost bricks as uploaded, by owner.
    pub(super) remote_ghosts:
        BTreeMap<bri_world::OwnerId, (bri_sim::session::GhostBrick, Option<GpuScene>)>,
    /// The HUD was told the tool in hand takes the paint cans.
    pub(super) tool_takes_paint: bool,
    pub(super) building: Option<crate::building::Building>,
    pub(super) tool_ui: crate::tool_ui::ToolUi,
    pub(super) macro_recording: Option<Vec<UiAction>>,
    pub(super) build_macro: Vec<UiAction>,
    pub(super) macro_playback: VecDeque<UiAction>,
}

impl App {
    pub(super) fn invalidate_tool_dialogs(&mut self) {
        self.net.dialog_epoch = self.net.dialog_epoch.wrapping_add(1);
        self.build.tool_ui.invalidate();
    }
    pub(super) fn handle_building(&mut self, id: RequestId, action: &UiAction) -> Result<bool> {
        if self.net.attempt.as_ref().is_none_or(|a| !a.entered) {
            return Ok(false);
        }
        if self
            .net
            .attempt
            .as_ref()
            .is_some_and(|a| self.ui.session_request() != Some(a.id))
        {
            return Ok(true); // action queued before a newer disconnect/rehost
        }
        if matches!(
            action,
            UiAction::UseTool { .. }
                | UiAction::UseBrickSlot { .. }
                | UiAction::InstantUseBrick { .. }
                | UiAction::BuyBricks { .. }
                | UiAction::UnUseTool
                | UiAction::UseSprayCan { .. }
                | UiAction::UseFxCan { .. }
                | UiAction::CancelWrench { .. }
                | UiAction::ClosePrintSelector
        ) {
            self.invalidate_tool_dialogs();
        }
        if let Some(command) = self.build.tool_ui.action_command(action)? {
            self.command(id, command, action.clone())?;
            return Ok(true);
        }
        if matches!(
            action,
            UiAction::CancelWrench { .. } | UiAction::ClosePrintSelector
        ) {
            self.answer(id, Ok(()));
            return Ok(true);
        }
        let view = self.network_view().context("No active network view")?;
        let archetypes = view.archetypes.clone();
        let seated = sit_posed(view, &self.vehicle_assets, self.motion.presented(), view.owner);
        let mut player = self
            .motion
            .presented()
            .get(&view.owner)
            .or_else(|| view.poses.get(&view.owner).map(|pose| &pose.player))
            .context("No local player pose")?
            .clone();
        // The latest local body aim drives ghost input. Free-look only changes
        // the camera; server tool targeting still uses its authoritative pose.
        player.yaw = self.controls.yaw;
        player.pitch = self.controls.pitch;
        let ghost_before = self
            .build
            .building
            .as_ref()
            .and_then(|b| b.ghost().cloned());
        let copy_before = self.build.building.as_ref().and_then(|b| b.copy_pose());
        let building = self
            .build
            .building
            .as_mut()
            .context("Building controller not ready")?;
        building.set_archetypes(archetypes);
        building.set_seated(seated);
        let response = building.ui_action(action, &player)?;
        let Some(response) = response else {
            return Ok(false);
        };
        if let Some((anchor, turns)) = self.build.building.as_ref().and_then(|b| b.copy_pose()) {
            let cue = match copy_before {
                Some((_, before)) if before != turns => Some("brick.rotate"),
                Some((before, _)) if before != anchor => Some("brick.move"),
                _ => None,
            };
            if let Some(cue) = cue {
                self.audio.trigger(cue, bri_audio::Placement::World(anchor));
            }
        } else if let Some(ghost) = self.build.building.as_ref().and_then(|b| b.ghost()) {
            let cue = if ghost_before
                .as_ref()
                .is_none_or(|b| b.definition != ghost.definition)
            {
                Some("brick.change")
            } else if ghost_before
                .as_ref()
                .is_some_and(|b| b.quarter_turns != ghost.quarter_turns)
            {
                Some("brick.rotate")
            } else if ghost_before
                .as_ref()
                .is_some_and(|b| b.position != ghost.position)
            {
                Some("brick.move")
            } else {
                None
            };
            if let Some(cue) = cue {
                self.audio
                    .trigger(cue, bri_audio::Placement::World(ghost.position));
            }
        }
        let session = self.net.attempt.as_ref().unwrap().id;
        for update in response.updates {
            self.ui.apply_session(session, update);
        }
        ensure!(
            response.commands.len() <= 1,
            "One UI request cannot own multiple server commands"
        );
        if let Some(command) = response.commands.into_iter().next() {
            if matches!(command, Command::Tool(ToolAction::Inspect { .. })) {
                self.invalidate_tool_dialogs();
            }
            if matches!(command, Command::WeaponTrigger { down: true }) {
                self.net.trigger_epoch = Some(self.net.dialog_epoch);
            }
            if let Err(error) = self
                .build
                .building
                .as_mut()
                .unwrap()
                .command_sent(id, &command)
            {
                for update in self
                    .build
                    .building
                    .as_mut()
                    .unwrap()
                    .command_finished(id, &command, None)
                {
                    self.ui.apply_session(session, update);
                }
                return Err(error);
            }
            if let Err(error) = self.command(id, command.clone(), action.clone()) {
                let updates = self
                    .build
                    .building
                    .as_mut()
                    .unwrap()
                    .command_finished(id, &command, None);
                for update in updates {
                    self.ui.apply_session(session, update);
                }
                // Closing the session clears a host-held trigger if its release
                // cannot enter the bounded transport queue.
                if matches!(command, Command::WeaponTrigger { down: false }) {
                    self.disconnect();
                }
                return Err(error);
            }
        } else {
            self.answer(id, Ok(()));
        }
        Ok(true)
    }
}
