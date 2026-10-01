//! What the engine does for the `effects` operations
//! (`bri_package_runtime::ops::effects`).
use super::*;

impl Perform for ops::ShowBox {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::ShowBox { player, area, tool } = self;
        session.show_box(player, area, &tool)
    }
}
impl Perform for ops::ShowShapes {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::ShowShapes { owner, key, shapes } = self;
        session.show_shapes(package, owner, &key, shapes)
    }
}
impl Perform for ops::Beam {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::Beam {
            from,
            to,
            color,
            width,
            seconds,
            muzzle,
        } = self;
        session.take_cue(package)?;
        session.cues.emit(
            tick,
            crate::presentation::CueKind::Beam {
                to,
                color,
                width,
                seconds,
                muzzle: muzzle.filter(|m| session.peers.contains_key(m)),
            },
            from,
        );
        Ok(())
    }
}
impl Perform for ops::PlayThread {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::PlayThread {
            player,
            thread,
            sequence,
            after,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.take_cue(package)?;
        if after > 0.0 {
            // A schedule is in whole milliseconds and fires on the
            // first tick at or past its time.
            let ms = (f64::from(after) * 1000.0).round() as u64;
            let ticks = (ms * bri_world::TICKS_PER_SECOND).div_ceil(1000);
            session.schedule_thread(player, tick + ticks, thread, &sequence)
        } else {
            session.play_thread(tick, player, thread, &sequence);
            Ok(())
        }
    }
}
impl Perform for ops::Sound {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::Sound { profile, at } = self;
        session.take_cue(package)?;
        match at {
            bri_package_runtime::ops::SoundAt::Player(player) => {
                ensure!(session.peers.contains_key(&player), "No such player");
                session.notify(player, Notice::Sound(profile));
            }
            bri_package_runtime::ops::SoundAt::Position(position) => session.cues.emit(
                tick,
                crate::presentation::CueKind::WeaponSound { profile },
                position,
            ),
        }
        Ok(())
    }
}
