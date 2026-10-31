//! Bounded local evidence for movement delay that disappears with a new body.
//! No control changes or per-frame formatting/I/O; one line per ten seconds
//! and at a body transition, using the existing console and session log.
use super::{MoveInput, Predictor};

#[derive(Default)]
pub(super) struct Health {
    next_report: f64,
    pose_at: f64,
    pose_tick: u64,
    acknowledged: u64,
    host_feet: [f32; 3],
    host_velocity: [f32; 3],
    spawn: Option<u64>,
    peak_pending: usize,
    peak_ack_trail: u64,
    peak_reconcile: f32,
}

impl Health {
    pub(super) fn observe(&mut self, pose: &bri_net::protocol::Pose, now: f64) {
        if self.spawn != Some(pose.spawn_tick) {
            self.next_report = 0.0;
        }
        self.spawn = Some(pose.spawn_tick);
        self.pose_at = now;
        self.pose_tick = pose.tick;
        self.acknowledged = pose.acknowledged_input;
        self.host_feet = pose.player.feet;
        self.host_velocity = pose.player.velocity;
    }

    pub(super) fn reconciled(&mut self, distance: f32) {
        self.peak_reconcile = self.peak_reconcile.max(distance);
    }

    pub(super) fn sample(
        &mut self,
        now: f64,
        predictor: Option<&Predictor>,
        input: MoveInput,
        correction: f32,
        mounted: bool,
    ) {
        let Some(predictor) = predictor else { return };
        self.peak_pending = self.peak_pending.max(predictor.pending_len());
        self.peak_ack_trail = self
            .peak_ack_trail
            .max(predictor.sequence().saturating_sub(self.acknowledged));
        if now < self.next_report {
            return;
        }
        self.next_report = now + 10.0;
        let state = predictor.state();
        bri_console::echo(format!(
            "Movement health: body={:?} host_tick={} pose_age={:.3}s seq={} ack={} pending={} peak_pending={} peak_ack_trail={} reconcile_peak={:.4} presentation_error={:.4} feet={:?} velocity={:?} host_feet={:?} host_velocity={:?} grounded={} jetting={} mounted={} input=[{:.2},{:.2},jump:{},jet:{}]",
            self.spawn,
            self.pose_tick,
            now - self.pose_at,
            predictor.sequence(),
            self.acknowledged,
            predictor.pending_len(),
            self.peak_pending,
            self.peak_ack_trail,
            self.peak_reconcile,
            correction,
            state.feet,
            state.velocity,
            self.host_feet,
            self.host_velocity,
            state.grounded,
            state.jetting,
            mounted,
            input.forward,
            input.right,
            input.jump,
            input.jet,
        ));
        self.peak_pending = 0;
        self.peak_ack_trail = 0;
        self.peak_reconcile = 0.0;
    }
}
