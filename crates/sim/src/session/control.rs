//! Control objects (`GameConnection::setControlObject`): a connection's moves
//! drive exactly one thing at a time. The body walks, or drives its vehicle
//! seat while mounted; an admin camera leaves the body standing still.
use super::{MoveInput, Peer, Session};
use anyhow::{Context, Result, ensure};
use bri_world::OwnerId;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// What a connection's moves steer. Replicated in [`super::Vitals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlObject {
    /// The player's own body, or the vehicle seat it occupies.
    #[default]
    Player,
    /// Admin free camera (`dropCameraAtPlayer`). Each client flies its own
    /// camera and reports its [`CameraView`]; the server parks the body,
    /// shows the orb to everyone else and drops the player there on F7.
    Camera,
    /// Admin `spy`: an orbit camera around another player.
    Spy(OwnerId),
    /// `Corpse` camera after death: orbits the player's own body until
    /// respawn, so the corpse takes no more input.
    Corpse,
    /// An Add-On's orbit camera around another player (`orbit_camera`;
    /// v20's `%client.camera.setOrbitMode(%target, %xform, %min, %max,
    /// %cur)` then `setControlObject(%client.camera)`): `distance` units
    /// out, which the wheel zooms between `min` and `max`. The body takes
    /// no moves, the camera does not hand control back on a click (the
    /// click still reaches Add-Ons as an empty-hand trigger), and only the
    /// Add-On or the target leaving ends it.
    Orbit {
        target: OwnerId,
        min: u8,
        max: u8,
        distance: u8,
    },
    /// A package entity (a kart, a drone, a second body) that a package
    /// handed this player (`control(player, entity)`). The player's moves
    /// drive that entity's body with its archetype's movement; the avatar
    /// stands where it was.
    Entity(u64),
}

/// `%client.Camera`'s transform: the eye it sits at and where it looks. The
/// client flies or orbits it and reports it with its moves; the server keeps
/// the last report, so `dropPlayerAtCamera` lands where the camera was left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraView {
    pub eye: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// The seat a client's moves are made for: from move `since` on, it knows
/// it sits in `seat` of `vehicle` and shapes its moves for that seat (a
/// driver's mouse turn, a passenger's turn on the seat, a gunner's look along
/// the turret). Sent with its moves; the host reads a move by the seat it was
/// made for, so moves still in flight from the seat a rider just left never
/// steer, turn or aim anything in the new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatSince {
    pub vehicle: u64,
    pub seat: u8,
    pub since: u64,
}
impl SeatSince {
    /// The report to send with move `next` by a client that now knows it
    /// sits in `seat` (vehicle, seat index), or is on foot: `report`, the
    /// last one sent, while the seat is the same, else a new one from `next`.
    pub fn follow(report: Option<Self>, seat: Option<(u64, u8)>, next: u64) -> Option<Self> {
        let (vehicle, seat) = seat?;
        match report {
            Some(report) if (report.vehicle, report.seat) == (vehicle, seat) => Some(report),
            _ => Some(Self {
                vehicle,
                seat,
                since: next,
            }),
        }
    }
}

impl CameraView {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.eye.iter().all(|x| x.is_finite() && x.abs() < 1_000_000.0)
                && self.yaw.is_finite()
                && self.pitch.is_finite()
                && self.pitch.abs() <= std::f32::consts::FRAC_PI_2,
            "Invalid camera view"
        );
        Ok(())
    }
    /// `Camera::getTransform`: heading, then pitch (`zRot * xRot`).
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(-self.yaw) * Quat::from_rotation_x(self.pitch)
    }
    pub fn eye(&self) -> Vec3 {
        Vec3::from(self.eye)
    }
}

impl Peer {
    /// The motor input for this tick: controls only while the body is in
    /// control. Otherwise it idles and keeps its own aim, so neither a corpse
    /// nor a camera operator turns the blockhead.
    pub(super) fn body_input(&self, input: MoveInput) -> MoveInput {
        if self.control == ControlObject::Player {
            input
        } else {
            let state = self.player.state();
            MoveInput {
                yaw: state.yaw,
                pitch: state.pitch,
                ..Default::default()
            }
        }
    }
}

impl Session {
    pub fn control(&self, owner: OwnerId) -> Option<ControlObject> {
        self.peers.get(&owner).map(|p| p.control)
    }
    /// The seat the client's moves are made for, reported with the moves
    /// up to `newest`; `None` is on foot. A report older than one already
    /// heard (a reordered datagram) is ignored. See [`SeatSince`].
    pub fn seat_report(
        &mut self,
        owner: OwnerId,
        newest: u64,
        seat: Option<SeatSince>,
    ) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer.seat_since.is_none_or(|(heard, _)| newest >= heard) {
            peer.seat_since = Some((newest, seat));
        }
        Ok(())
    }
    /// The client's latest camera view, reported alongside its moves while a
    /// camera has control. Reports from the body are ignored.
    pub fn camera_report(&mut self, owner: OwnerId, view: CameraView) -> Result<()> {
        view.validate()?;
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer.control != ControlObject::Player {
            peer.camera = Some(view);
        }
        Ok(())
    }
    /// Where each admin's free camera is, for the `cameraImage` orb others
    /// see (`Observer` mode only: `Corpse` mode unmounts the image).
    pub fn camera_orbs(&self) -> Vec<(OwnerId, [f32; 3])> {
        self.peers
            .iter()
            .filter(|(_, p)| p.control == ControlObject::Camera)
            .filter_map(|(owner, p)| Some((*owner, p.camera?.eye)))
            .collect()
    }
    /// `serverCmdDropCameraAtPlayer`: the camera starts at the body's eye,
    /// looking where the body looks.
    pub(super) fn drop_camera_at_player(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        if peer.combat.alive || !peer.combat.corpse_cleared {
            let state = peer.player.state();
            peer.camera = Some(CameraView {
                eye: peer.player.eye().to_array(),
                yaw: state.yaw,
                pitch: state.pitch,
            });
        }
        peer.control = ControlObject::Camera;
        Ok(())
    }
    /// Grant an admin camera; callers own the administrator check.
    pub(super) fn set_control(&mut self, owner: OwnerId, control: ControlObject) -> Result<()> {
        if let ControlObject::Spy(target) = control {
            ensure!(target != owner, "You cannot spy on yourself");
            ensure!(self.peers.contains_key(&target), "Unknown player");
        }
        self.peers
            .get_mut(&owner)
            .context("Unknown connection")?
            .control = control;
        Ok(())
    }
    /// `setControlObject(player)`, or back to the corpse camera while dead.
    pub(super) fn return_to_body(&mut self, owner: OwnerId) -> Result<()> {
        let peer = self.peers.get_mut(&owner).context("Unknown connection")?;
        peer.control = if peer.combat.alive {
            ControlObject::Player
        } else {
            ControlObject::Corpse
        };
        Ok(())
    }
    /// Spies watching a departing player return to their own bodies.
    /// An Add-On's `orbit_camera`: `owner` watches `orbit.target`, or
    /// (`None`) has their body back. Only a player on their body or in
    /// another Add-On orbit is given one: an admin camera or a driven
    /// entity keeps control.
    pub(super) fn orbit_camera(
        &mut self,
        owner: OwnerId,
        orbit: Option<bri_package_runtime::ops::Orbit>,
    ) -> Result<()> {
        let peer = self.peers.get(&owner).context("No such player")?;
        let Some(orbit) = orbit else {
            if matches!(peer.control, ControlObject::Orbit { .. }) {
                self.return_to_body(owner)?;
            }
            return Ok(());
        };
        ensure!(
            peer.combat.alive,
            "Only living players are given an orbit camera"
        );
        ensure!(
            matches!(
                peer.control,
                ControlObject::Player | ControlObject::Orbit { .. }
            ),
            "That player's camera is in other hands"
        );
        let target = orbit.target;
        ensure!(orbit.valid(), "Invalid orbit distances");
        ensure!(target != owner, "A player cannot orbit themselves");
        ensure!(self.peers.contains_key(&target), "No such player to orbit");
        self.peers.get_mut(&owner).expect("checked").control = ControlObject::Orbit {
            target,
            min: orbit.min,
            max: orbit.max,
            distance: orbit.distance,
        };
        Ok(())
    }
    /// Whoever watched `target` (an admin spy, an Add-On orbit) has their
    /// body back.
    pub(super) fn release_spies(&mut self, target: OwnerId) {
        let spies: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, p)| {
                p.control == ControlObject::Spy(target)
                    || matches!(p.control, ControlObject::Orbit { target: t, .. } if t == target)
            })
            .map(|(owner, _)| *owner)
            .collect();
        for spy in spies {
            let _ = self.return_to_body(spy);
        }
    }
}
