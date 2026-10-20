//! How the map's sounds reach the listener: a wall in the way, rooms between (`bri_content::regions::SoundCarry`), and the room the
//! listener rings in.

use bri_audio::{Hearing, Room};
use bri_content::regions::{RegionMap, SoundCarry, ZoneSteps};

/// The map's sound rules with the region steps worked out once.
pub struct SoundCarryModel {
    carry: SoundCarry,
    steps: Option<ZoneSteps>,
}

impl SoundCarryModel {
    /// The model for `regions`, when the map says how its sounds carry.
    pub fn new(regions: &RegionMap) -> Option<Self> {
        Some(Self {
            carry: regions.sound?,
            steps: regions.zone_steps(),
        })
    }

    /// How a sound at `source` is heard from `listener`, given whether the straight line between them is `clear`. A clear line
    /// carries it as authored. A blocked one cuts the distance it is heard at by `zone_decay` raised to the square of the steps
    /// between their regions (none when the map does not connect its regions; no way at all when it connects them and these
    /// two are not), and brings the full-volume part in to `blocked_reference` of that.
    pub fn hearing(
        &self,
        regions: &RegionMap,
        listener: [f32; 3],
        source: [f32; 3],
        clear: bool,
    ) -> Hearing {
        if clear {
            return Hearing::CLEAR;
        }
        let steps = match &self.steps {
            Some(steps) => steps
                .between(regions.locate(listener), regions.locate(source))
                .map(f32::from),
            None => Some(0.0),
        };
        Hearing {
            reach: steps.map_or(0.0, |steps| self.carry.zone_decay.powf(steps * steps)),
            reference: self.carry.blocked_reference,
        }
    }
}

/// The reverb of the region the listener is in; dry where the region sets none.
pub fn room(regions: &RegionMap, listener: [f32; 3]) -> Room {
    regions
        .regions
        .get(usize::from(regions.locate(listener)))
        .and_then(|region| region.reverb)
        .map_or(Room::DRY, |reverb| Room {
            wet: reverb.wet,
            decay_seconds: reverb.decay_seconds,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::regions::{Region, RegionReverb};

    /// A wall at x = 0: region 1 in front, region 2 behind, joined (or not) as asked.
    fn rooms(connected: bool) -> RegionMap {
        let joined = |to: u16| Region {
            connects: if connected { vec![to] } else { vec![] },
            reverb: (to == 1).then_some(RegionReverb {
                wet: 0.5,
                decay_seconds: 2.0,
            }),
            ..Region::default()
        };
        RegionMap {
            planes: vec![[1.0, 0.0, 0.0, 0.0]],
            front: vec![-1],
            back: vec![-1],
            front_region: vec![1],
            back_region: vec![2],
            regions: vec![Region::default(), joined(2), joined(1)],
            sound: Some(SoundCarry {
                zone_decay: 0.5,
                blocked_reference: 0.35,
            }),
        }
    }

    #[test]
    fn a_clear_line_carries_a_sound_as_authored_and_a_blocked_one_is_cut_by_the_steps() {
        let regions = rooms(true);
        let model = SoundCarryModel::new(&regions).unwrap();
        let (listener, source) = ([3.0, 0.0, 0.0], [-3.0, 0.0, 0.0]);
        assert_eq!(
            model.hearing(&regions, listener, source, true),
            Hearing::CLEAR
        );
        // One doorway apart: 0.5 ^ (1 * 1).
        let blocked = model.hearing(&regions, listener, source, false);
        assert_eq!(blocked.reach, 0.5);
        assert_eq!(blocked.reference, 0.35);
        // The same room: no steps, so only the wall's own cut to the loud part.
        let same = model.hearing(&regions, listener, [4.0, 0.0, 0.0], false);
        assert_eq!(same.reach, 1.0);
    }

    #[test]
    fn rooms_the_map_does_not_join_do_not_hear_each_other_through_a_wall() {
        let regions = rooms(false);
        let model = SoundCarryModel::new(&regions).unwrap();
        let blocked = model.hearing(&regions, [3.0, 0.0, 0.0], [-3.0, 0.0, 0.0], false);
        assert_eq!(blocked.reach, 1.0, "no connections at all: the wall alone");
        let mut joined_elsewhere = rooms(true);
        joined_elsewhere.regions[1].connects.clear();
        joined_elsewhere.regions[2].connects = vec![0];
        let model = SoundCarryModel::new(&joined_elsewhere).unwrap();
        let blocked = model.hearing(&joined_elsewhere, [3.0, 0.0, 0.0], [-3.0, 0.0, 0.0], false);
        assert_eq!(blocked.reach, 0.0, "joined, but not to each other");
    }

    #[test]
    fn a_map_without_sound_rules_has_no_model_and_the_listener_rings_in_its_region() {
        let mut regions = rooms(true);
        assert_eq!(room(&regions, [-3.0, 0.0, 0.0]).decay_seconds, 2.0);
        assert_eq!(room(&regions, [3.0, 0.0, 0.0]), Room::DRY);
        regions.sound = None;
        assert!(SoundCarryModel::new(&regions).is_none());
    }
}
