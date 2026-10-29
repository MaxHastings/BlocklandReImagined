//! How the map's sounds reach the listener: a wall in the way, rooms between (`bri_content::regions::SoundCarry`), and the room the
//! listener rings in.

use bri_audio::{Hearing, Room};
use bri_content::regions::{RegionMap, SoundCarry, ZoneSteps};

/// Bounded listener exposure from current physical map/brick shapes. This is
/// a local approximation, not a room classifier or an acoustic simulation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Exposure {
    pub cover: f32,
    pub enclosure: f32,
}

#[derive(Default)]
pub struct EnclosureProbe {
    last: Option<(std::time::Instant, [f32; 3], u64)>,
    exposure: Exposure,
}

impl EnclosureProbe {
    pub const RAYS: usize = 17;
    pub const REACH: f32 = 96.0;

    pub fn sample(
        &mut self,
        listener: [f32; 3],
        revision: u64,
        blocked: impl FnMut([f32; 3]) -> bool,
    ) -> Exposure {
        self.sample_at(std::time::Instant::now(), listener, revision, blocked)
    }

    fn sample_at(
        &mut self,
        now: std::time::Instant,
        listener: [f32; 3],
        revision: u64,
        mut blocked: impl FnMut([f32; 3]) -> bool,
    ) -> Exposure {
        if let Some((last, position, old_revision)) = self.last {
            let movement = listener
                .iter()
                .zip(position)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>();
            if now.duration_since(last).as_secs_f32() < 0.25
                || (movement < 0.25 && old_revision == revision)
            {
                return self.exposure;
            }
        }
        let roof = [
            [0., 1., 0.],
            [0.5, 1., 0.5],
            [-0.5, 1., 0.5],
            [0.5, 1., -0.5],
            [-0.5, 1., -0.5],
            [0.8, 0.6, 0.],
            [-0.8, 0.6, 0.],
            [0., 0.6, 0.8],
            [0., 0.6, -0.8],
        ];
        let sides = [
            [1., 0., 0.],
            [-1., 0., 0.],
            [0., 0., 1.],
            [0., 0., -1.],
            [1., 0., 1.],
            [-1., 0., 1.],
            [1., 0., -1.],
            [-1., 0., -1.],
        ];
        let cover = roof.into_iter().filter(|d| blocked(*d)).count() as f32 / 9.0;
        let surround = sides.into_iter().filter(|d| blocked(*d)).count() as f32 / 8.0;
        self.exposure = Exposure {
            cover,
            enclosure: surround * (0.35 + 0.65 * cover),
        };
        self.last = Some((now, listener, revision));
        self.exposure
    }
}

impl Exposure {
    pub fn acoustics(
        self,
        intent: Option<bri_weapons::SoundEnvironment>,
        blocked: bool,
    ) -> bri_audio::Acoustics {
        let mut acoustic = match intent {
            Some(bri_weapons::SoundEnvironment::Exterior) => bri_audio::Acoustics {
                gain: 1.0 - 0.25 * self.cover - 0.65 * self.enclosure,
                muffle: 0.05 * self.cover + 0.9 * self.enclosure,
            },
            Some(bri_weapons::SoundEnvironment::Interior) => bri_audio::Acoustics {
                gain: 0.1 + 0.9 * self.enclosure,
                muffle: 0.0,
            },
            None => bri_audio::Acoustics::CLEAR,
        };
        if blocked {
            acoustic.muffle = acoustic.muffle.max(0.7);
        }
        acoustic
    }
}

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

    #[test]
    fn live_brick_shapes_distinguish_roof_only_rooms_windows_and_open_rafters() {
        use crate::building::Building;
        use bri_content::collision::{CollisionBody, Part};
        use bri_sim::definitions::{Definition, Definitions};
        use bri_world::{Brick, ContentRef};
        use glam::Vec3;
        let box_part = |center, size| Part::Box { center, size };
        let definition = |id: &str, footprint: [u8; 2], plates: u16, parts| {
            let collision = CollisionBody {
                id: id.into(),
                parts,
            };
            let shape = bri_physics::content::collider(&collision)
                .unwrap()
                .build()
                .shared_shape()
                .clone();
            Definition {
                mesh: bri_sim::testing::definition(
                    id,
                    footprint,
                    plates,
                    Default::default(),
                    false,
                )
                .mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.; 4],
                bot: None,
            }
        };
        let definitions = Definitions {
            entries: [
                (
                    "wall".into(),
                    definition("wall", [16, 1], 20, vec![box_part([0.; 3], [8., 4., 0.5])]),
                ),
                (
                    "sidewall".into(),
                    definition(
                        "sidewall",
                        [14, 1],
                        20,
                        vec![box_part([0.; 3], [7., 4., 0.5])],
                    ),
                ),
                (
                    "roof".into(),
                    definition("roof", [16, 16], 1, vec![box_part([0.; 3], [8., 0.2, 8.])]),
                ),
                (
                    "window".into(),
                    definition(
                        "window",
                        [16, 1],
                        20,
                        vec![
                            box_part([-3., 0., 0.], [2., 4., 0.5]),
                            box_part([3., 0., 0.], [2., 4., 0.5]),
                            box_part([0., 1.75, 0.], [4., 0.5, 0.5]),
                            box_part([0., -1.75, 0.], [4., 0.5, 0.5]),
                        ],
                    ),
                ),
                (
                    "rafters".into(),
                    definition(
                        "rafters",
                        [16, 16],
                        1,
                        [-3.5, -2.5, -1.5, -0.5, 0.5, 1.5, 2.5, 3.5]
                            .into_iter()
                            .map(|x| box_part([x, 0., 0.], [0.2, 0.2, 8.]))
                            .collect(),
                    ),
                ),
            ]
            .into(),
        };
        let mut building = Building::new(definitions.clone(), vec![]).unwrap();
        let mut world = bri_net::protocol::PublicWorld {
            name: "acoustic fixture".into(),
            map_id: "fixture".into(),
            palette: vec![[1.; 4]],
            bricks: Default::default(),
        };
        let ear = Vec3::new(0., 2., 0.);
        let sample = |probe: &mut EnclosureProbe, building: &Building, now| {
            probe.sample_at(now, ear.to_array(), building.query_generation(), |d| {
                building
                    .solid_segment(ear, ear + Vec3::from(d).normalize() * EnclosureProbe::REACH)
                    .unwrap()
                    .is_some()
            })
        };
        building.sync_world(&world).unwrap();
        let mut probe = EnclosureProbe::default();
        let mut now = std::time::Instant::now();
        assert_eq!(sample(&mut probe, &building, now).cover, 0.0);
        world.bricks.insert(
            1,
            Brick::new(ContentRef::Resolved("roof".into()), [0., 4.1, 0.], 0),
        );
        building.sync_world(&world).unwrap();
        now += std::time::Duration::from_millis(250);
        let roof = sample(&mut probe, &building, now);
        assert_eq!(roof.cover, 1.0);
        assert_eq!(roof.enclosure, 0.0);
        // One-stud-thick panels have quarter-unit center offsets. The shorter
        // sides abut the front/back panels without overlapping corner cells.
        for (id, at, turn, name) in [
            (2, [0., 2., 3.75], 0, "wall"),
            (3, [0., 2., -3.75], 0, "wall"),
            (4, [3.75, 2., 0.], 1, "sidewall"),
            (5, [-3.75, 2., 0.], 1, "sidewall"),
        ] {
            let mut brick = Brick::new(ContentRef::Resolved(name.into()), at, 0);
            brick.quarter_turns = turn;
            bri_sim::grid::Bounds::new(&brick, &definitions.entries[name].mesh).unwrap();
            world.bricks.insert(id, brick);
        }
        building.sync_world(&world).unwrap();
        now += std::time::Duration::from_millis(250);
        assert_eq!(sample(&mut probe, &building, now).enclosure, 1.0);
        // Exercise an ordinary authoritative hammer swing, then feed its real
        // world state through the same client geometry sync as a replica update.
        // A manual map removal alone cannot establish this destruction seam.
        {
            use bri_sim::{
                player::{MoveInput, PlayerState, PlayerTuning},
                session::{ActionAim, Command, Session},
                simulation::Simulation,
            };
            let mut authoritative = bri_world::World::new(
                world.name.clone(),
                world.map_id.clone(),
                world.palette.clone(),
            );
            authoritative.bricks = world.bricks.clone();
            let floor = rapier3d::prelude::ColliderBuilder::cuboid(30., 0.5, 30.)
                .translation(Vec3::new(0., -0.5, 0.));
            let sim = Simulation::new(authoritative, definitions, vec![floor]).unwrap();
            let mut session = Session::new(sim);
            session.set_lan_host(true);
            session
                .set_weapon_pack(bri_weapons::testing::pack())
                .unwrap();
            let owner = session
                .join("Builder".into(), Vec3::new(2., 0.05, 0.), true)
                .unwrap();
            let hammer = bri_weapons::testing::HAMMER;
            let slot = session.tool_inventories()[&owner]
                .slots
                .iter()
                .position(|item| item.as_deref() == Some(hammer))
                .unwrap_or_else(|| session.give_item(owner, hammer).unwrap());
            session
                .command(owner, 1, Command::EquipTool { slot: Some(slot) })
                .unwrap();
            for _ in 0..30 {
                session.step().unwrap();
            }
            let player = session
                .snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == owner)
                .unwrap();
            let target = Vec3::from(world.bricks[&4].position);
            let flat = target - Vec3::from(player.feet);
            let yaw = flat.x.atan2(-flat.z);
            let facing = PlayerState { yaw, ..player };
            let delta = target - facing.eye(&PlayerTuning::default());
            let aim = ActionAim {
                yaw,
                pitch: delta.y.atan2(Vec3::new(delta.x, 0., delta.z).length()),
            };
            session
                .movement(
                    owner,
                    1,
                    MoveInput {
                        yaw: aim.yaw,
                        pitch: aim.pitch,
                        ..Default::default()
                    },
                )
                .unwrap();
            session.take_cues();
            session
                .command_with_aim(owner, 2, Command::WeaponTrigger { down: true }, Some(aim))
                .unwrap();
            for _ in 0..8 {
                session.step().unwrap();
            }
            session
                .command_with_aim(owner, 3, Command::WeaponTrigger { down: false }, Some(aim))
                .unwrap();
            for _ in 0..52 {
                session.step().unwrap();
            }
            assert!(
                !session.simulation().state().bricks.contains_key(&4),
                "ordinary hammer failed to open the wall"
            );
            assert!(
                session.take_cues().iter().any(|cue| matches!(
                    cue.kind,
                    bri_sim::presentation::CueKind::BrickKill { brick: 4, .. }
                )),
                "no authoritative destruction cue"
            );
            let mut damaged = world.clone();
            damaged.bricks = session.simulation().state().bricks.clone();
            assert!(
                building
                    .solid_segment(ear, Vec3::new(8., 2., 0.))
                    .unwrap()
                    .is_some()
            );
            building.sync_world(&damaged).unwrap();
            now += std::time::Duration::from_millis(250);
            let opened = sample(&mut probe, &building, now);
            assert!(
                opened.enclosure < 1.0,
                "stationary listener did not observe the destruction opening"
            );
            assert!(
                building
                    .solid_segment(ear, Vec3::new(8., 2., 0.))
                    .unwrap()
                    .is_none(),
                "removed wall still obstructs source"
            );
            building.sync_world(&world).unwrap();
            now += std::time::Duration::from_millis(250);
            assert_eq!(sample(&mut probe, &building, now).enclosure, 1.0);
        }
        world.bricks.get_mut(&2).unwrap().definition = ContentRef::Resolved("window".into());
        building.sync_world(&world).unwrap();
        now += std::time::Duration::from_millis(250);
        let window = sample(&mut probe, &building, now);
        assert!(window.enclosure < 1.0 && window.enclosure > 0.0);
        world.bricks.get_mut(&1).unwrap().definition = ContentRef::Resolved("rafters".into());
        building.sync_world(&world).unwrap();
        now += std::time::Duration::from_millis(250);
        let rafters = sample(&mut probe, &building, now);
        assert!(rafters.cover < 0.25 && rafters.enclosure < window.enclosure);
        // This tests the live query's removal invalidation; actual weapon
        // destruction integration is separately required, not inferred here.
        world.bricks.remove(&4);
        building.sync_world(&world).unwrap();
        now += std::time::Duration::from_millis(250);
        assert!(sample(&mut probe, &building, now).enclosure < rafters.enclosure);
        let mut traced = 0;
        probe.sample_at(
            now + std::time::Duration::from_millis(10),
            ear.to_array(),
            building.query_generation(),
            |_| {
                traced += 1;
                false
            },
        );
        assert_eq!(traced, 0, "cadence caps probes even after edits");
    }

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
