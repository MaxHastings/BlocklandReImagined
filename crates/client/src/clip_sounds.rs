//! Sounds an archetype ties to one of its clips (`sounds.clips`): when a body
//! starts the clip, the sound plays where the body is once the clip has run
//! `seconds`. Halo's body-falls are such sounds: a death animation names the
//! sound and the frame the body lands on. Each client plays them from the
//! poses it already draws, so they cost nothing on the wire.
use bri_sim::archetype::Sounds;

#[derive(Default)]
pub struct ClipSounds {
    /// (seconds left, sound profile, where).
    pending: Vec<(f32, String, [f32; 3])>,
}
impl ClipSounds {
    /// At most this many sounds wait at once; a crowd of deaths drops the rest.
    const MAX_PENDING: usize = 64;
    /// A body started `clip` (lower case) at `at`: queue its sound.
    pub fn started(&mut self, sounds: &Sounds, clip: &str, at: [f32; 3]) {
        let Some(sound) = sounds.clips.get(clip) else {
            return;
        };
        if self.pending.len() < Self::MAX_PENDING && at.iter().all(|v| v.is_finite()) {
            self.pending.push((sound.seconds, sound.sound.clone(), at));
        }
    }
    /// Advance `seconds` and return the sounds now due.
    pub fn advance(&mut self, seconds: f32) -> Vec<(String, [f32; 3])> {
        if !(seconds.is_finite() && seconds > 0.0) {
            return Vec::new();
        }
        let mut due = Vec::new();
        self.pending.retain_mut(|(left, sound, at)| {
            *left -= seconds.min(0.25);
            if *left <= 0.0 {
                due.push((std::mem::take(sound), *at));
                false
            } else {
                true
            }
        });
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Sounds {
        let mut body = Sounds::default();
        body.clips.insert(
            "fall".into(),
            bri_sim::archetype::ClipSound {
                sound: "thud".into(),
                seconds: 0.5,
            },
        );
        body
    }

    #[test]
    fn a_clips_sound_plays_where_the_body_was_once_its_time_has_run() {
        let mut clips = ClipSounds::default();
        clips.started(&body(), "fall", [1.0, 2.0, 3.0]);
        clips.started(&body(), "other", [0.0; 3]);
        assert!(clips.advance(0.2).is_empty(), "not yet");
        assert!(clips.advance(0.2).is_empty(), "not yet");
        assert_eq!(
            clips.advance(0.2),
            vec![("thud".to_string(), [1.0, 2.0, 3.0])]
        );
        assert!(clips.advance(1.0).is_empty(), "once");
    }
}
