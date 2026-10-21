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
impl Perform for ops::BodyFlash {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::BodyFlash {
            player,
            color,
            seconds,
        } = self;
        let feet = session
            .peers
            .get(&player)
            .map(|peer| peer.player.state().feet)
            .context("No such player")?;
        session.take_cue(package)?;
        session.cues.emit(
            tick,
            crate::presentation::CueKind::BodyFlash {
                owner: player,
                color,
                seconds,
            },
            feet,
        );
        Ok(())
    }
}
impl Perform for ops::BodyShell {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::BodyShell {
            player,
            shell,
            strength,
            seconds,
        } = self;
        let feet = session
            .peers
            .get(&player)
            .map(|peer| peer.player.state().feet)
            .context("No such player")?;
        session.take_cue(package)?;
        session.cues.emit(
            tick,
            crate::presentation::CueKind::BodyShell {
                owner: player,
                shell,
                strength,
                seconds,
            },
            feet,
        );
        Ok(())
    }
}
impl Perform for ops::EffectAt {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, tick, .. } = cx;
        let ops::EffectAt {
            effect,
            position,
            direction,
            scale,
        } = self;
        session.take_cue(package)?;
        session.cues.emit(
            tick,
            crate::presentation::CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Map(0),
                definition: effect,
                node: String::new(),
                seconds: 0.,
                image: None,
                hand: None,
                direction: Some(direction),
                contact: None,
                scale,
            },
            position,
        );
        Ok(())
    }
}
impl Perform for ops::SetTeleportSound {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        session.take_cue(cx.package)?;
        session.map_teleporters.set_sound(self.profile);
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
        let ops::Sound {
            profile,
            at,
            volume,
        } = self;
        session.take_cue(package)?;
        // Full volume keeps the plain messages every client already knows.
        let scaled = volume != 1.0;
        match at {
            bri_package_runtime::ops::SoundAt::Player(player) => {
                ensure!(session.peers.contains_key(&player), "No such player");
                session.notify(
                    player,
                    if scaled {
                        Notice::SoundScaled { profile, volume }
                    } else {
                        Notice::Sound(profile)
                    },
                );
            }
            bri_package_runtime::ops::SoundAt::Position(position) => session.cues.emit(
                tick,
                if scaled {
                    crate::presentation::CueKind::SoundScaled { profile, volume }
                } else {
                    crate::presentation::CueKind::WeaponSound { profile }
                },
                position,
            ),
        }
        Ok(())
    }
}
impl Perform for ops::Ambient {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Ambient {
            player,
            key,
            track,
            volume,
            fade,
            at,
            range,
        } = self;
        session.take_cue(package)?;
        let play = crate::session::AmbientPlay {
            key,
            track,
            volume,
            fade,
            at,
            range,
        };
        match player {
            Some(player) => {
                ensure!(session.peers.contains_key(&player), "No such player");
                session.notify(player, Notice::Ambient(play));
            }
            None => ensure!(
                session.play_ambient_all(play),
                "at most {} ambient sounds play for everyone at once",
                ops::MAX_AMBIENT
            ),
        }
        Ok(())
    }
}
impl Perform for ops::Music {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Music {
            player,
            track,
            looping,
            fade,
        } = self;
        session.take_cue(package)?;
        let play = crate::session::MusicPlay {
            track,
            looping,
            fade,
        };
        match player {
            Some(player) => {
                ensure!(session.peers.contains_key(&player), "No such player");
                session.notify(player, Notice::Music(play));
            }
            None => session.play_music_all(play),
        }
        Ok(())
    }
}
