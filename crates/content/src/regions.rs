//! A map's regions: the partition of space into numbered regions, each optionally with an atmosphere of its own.
//!
//! The partition is a plane tree. Starting at node 0, a point is on a node's front side when `dot(normal, point) - w >= 0`; that side
//! names a region and a child node, and the walk goes on into the child until it is -1. The region named last is the point's. A map
//! with no regions has region 0 everywhere.
//!
//! A region's [`Fog`] is linear distance fog the renderer applies while the viewer is in the region. Moving between regions blends
//! the fog over the entered region's `blend_time` ([`FogBlend`]); the blend keeps the original's rule that it restarts from the
//! fog it last settled on, not from the value mid-blend.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The distance the "no fog" sentinel is parked at while a blend fades fog out.
pub const FAR: f32 = 65536.0;

/// Linear distance fog.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fog {
    /// Red, green, blue as bytes.
    pub color: [u8; 3],
    pub start: f32,
    pub end: f32,
    /// Seconds to blend into this fog when entering the region; 0 is at once.
    #[serde(default)]
    pub blend_time: f32,
    /// The frame is cleared to the fog colour while the viewer is in the region.
    #[serde(default)]
    pub clear_to_fog: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Region {
    #[serde(default)]
    pub fog: Option<Fog>,
    /// The light actors in the region take from everywhere (linear RGB, as lit models add it).
    #[serde(default)]
    pub ambient: Option<[f32; 3]>,
    /// The regions sound passes into from this one (a doorway, a window left open), by number. Rooms joined this way hear each
    /// other; see [`ZoneSteps`].
    #[serde(default)]
    pub connects: Vec<u16>,
    /// The room sound effects ring in while the listener is in this region.
    #[serde(default)]
    pub reverb: Option<RegionReverb>,
}

/// A region's reverb: the level of the tail and how long it lasts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionReverb {
    /// Linear level of the reverb added to the effects (0 is dry, 1 is as loud as the sound).
    pub wet: f32,
    /// Seconds for the tail to fall 60 dB.
    pub decay_seconds: f32,
}

/// How a map's sounds carry, written once for the map. Without it every sound carries as authored.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundCarry {
    /// What a wall or rooms in the way do to a sound: the distance it is heard at is cut by `zone_decay` raised to the square of
    /// the steps between the two regions ([`ZoneSteps`]); a sound with no region data is cut by the wall alone.
    pub zone_decay: f32,
    /// Share of the cut distance still heard at full volume when something is in the way.
    pub blocked_reference: f32,
}

/// The most regions a [`ZoneSteps`] table is built for.
pub const MAX_STEP_REGIONS: usize = 256;

/// Steps of sound between every pair of regions. The reach of a region starts as the regions it connects to; round `d` gives step
/// `d` to every region the reach now holds that has none, then the reach widens to the regions reachable from each region in it. So
/// a step counts doublings of the path, not single doorways: a few rooms in a row cost little more than two. A pair never reached
/// has no steps.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneSteps {
    count: usize,
    steps: Vec<Option<u8>>,
}

impl ZoneSteps {
    pub fn new(map: &RegionMap) -> Option<Self> {
        let count = map.regions.len();
        if count == 0
            || count > MAX_STEP_REGIONS
            || map.regions.iter().all(|r| r.connects.is_empty())
        {
            return None;
        }
        let mut reach: Vec<Vec<bool>> = map
            .regions
            .iter()
            .map(|r| {
                let mut row = vec![false; count];
                for c in &r.connects {
                    row[usize::from(*c)] = true;
                }
                row
            })
            .collect();
        let mut steps = vec![None; count * count];
        for i in 0..count {
            steps[i * count + i] = Some(0);
        }
        for round in 1..=count.min(usize::from(u8::MAX)) {
            for (i, row) in reach.iter().enumerate() {
                for (j, held) in row.iter().enumerate() {
                    if *held && steps[i * count + j].is_none() {
                        steps[i * count + j] = Some(round as u8);
                    }
                }
            }
            let next: Vec<Vec<bool>> = reach
                .iter()
                .map(|row| {
                    let mut wider = row.clone();
                    for (j, held) in row.iter().enumerate() {
                        if *held {
                            for (k, far) in reach[j].iter().enumerate() {
                                wider[k] |= far;
                            }
                        }
                    }
                    wider
                })
                .collect();
            if next == reach {
                break;
            }
            reach = next;
        }
        Some(Self { count, steps })
    }

    /// Steps from region `a` to region `b`, or `None` when sound never gets there.
    pub fn between(&self, a: u16, b: u16) -> Option<u8> {
        let (a, b) = (usize::from(a), usize::from(b));
        if a >= self.count || b >= self.count {
            return None;
        }
        self.steps[a * self.count + b]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionMap {
    /// `[nx, ny, nz, w]` per node.
    pub planes: Vec<[f32; 4]>,
    /// Child node on each side, or -1.
    pub front: Vec<i32>,
    pub back: Vec<i32>,
    /// The region on each side.
    pub front_region: Vec<u16>,
    pub back_region: Vec<u16>,
    /// By region number.
    pub regions: Vec<Region>,
    /// How the map's sounds carry, when it says.
    #[serde(default)]
    pub sound: Option<SoundCarry>,
}

impl RegionMap {
    pub fn validate(&self) -> Result<()> {
        let n = self.planes.len();
        ensure!(
            [
                self.front.len(),
                self.back.len(),
                self.front_region.len(),
                self.back_region.len()
            ]
            .iter()
            .all(|l| *l == n),
            "A region map's arrays differ in length"
        );
        ensure!(
            n <= 1 << 22 && self.regions.len() <= 1 << 16,
            "A region map is too large"
        );
        for p in &self.planes {
            ensure!(
                p.iter().all(|v| v.is_finite()),
                "A region plane is not finite"
            );
        }
        for c in self.front.iter().chain(&self.back) {
            ensure!(
                *c >= -1 && i64::from(*c) < n as i64,
                "A region node names a missing child"
            );
        }
        for r in self.front_region.iter().chain(&self.back_region) {
            ensure!(
                usize::from(*r) < self.regions.len().max(1),
                "A region node names a missing region"
            );
        }
        if let Some(carry) = &self.sound {
            ensure!(
                (0.0..=1.0).contains(&carry.zone_decay)
                    && (0.0..=1.0).contains(&carry.blocked_reference),
                "A map's sound carry is outside 0 to 1"
            );
        }
        for r in &self.regions {
            ensure!(
                r.connects
                    .iter()
                    .all(|c| usize::from(*c) < self.regions.len()),
                "A region connects to a missing region"
            );
            if let Some(reverb) = &r.reverb {
                ensure!(
                    reverb.wet.is_finite()
                        && reverb.decay_seconds.is_finite()
                        && reverb.wet >= 0.0
                        && reverb.decay_seconds >= 0.0,
                    "A region's reverb is not a level and a time"
                );
            }
            if let Some(f) = &r.fog {
                ensure!(
                    f.start.is_finite() && f.end.is_finite() && f.blend_time.is_finite(),
                    "A region's fog is not finite"
                );
            }
        }
        Ok(())
    }

    /// How many steps sound takes between two regions, when this map's regions connect ([`ZoneSteps`]). `None` when the map has
    /// no connections or more regions than the table holds.
    pub fn zone_steps(&self) -> Option<ZoneSteps> {
        ZoneSteps::new(self)
    }

    /// The region a point is in.
    pub fn locate(&self, p: [f32; 3]) -> u16 {
        if self.planes.is_empty() {
            return 0;
        }
        let (mut node, mut region) = (0usize, 0u16);
        // A tree has no cycles; the bound keeps a damaged one from looping.
        for _ in 0..=self.planes.len() {
            let n = self.planes[node];
            let front = p[0] * n[0] + p[1] * n[1] + p[2] * n[2] - n[3] >= 0.0;
            let (r, child) = if front {
                (self.front_region[node], self.front[node])
            } else {
                (self.back_region[node], self.back[node])
            };
            region = r;
            if child < 0 {
                break;
            }
            node = child as usize;
        }
        region
    }

    /// Every region a box reaches, once each, in the order found: the zones the original engine's `UModel::BoxLeaves` gives an actor
    /// (`UpdateRenderData`, Engine 0xeed00). A box wholly on one side of a plane takes that side only; one that straddles it takes both.
    pub fn regions_in_box(&self, min: [f32; 3], max: [f32; 3]) -> Vec<u16> {
        let mut found = Vec::new();
        if self.planes.is_empty() {
            return vec![0];
        }
        let mut stack = vec![0usize];
        let mut budget = self.planes.len() * 2 + 2;
        while let Some(node) = stack.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let n = self.planes[node];
            // The nearest and farthest corners along the normal.
            let (mut low, mut high) = (-n[3], -n[3]);
            for axis in 0..3 {
                let (a, b) = (n[axis] * min[axis], n[axis] * max[axis]);
                low += a.min(b);
                high += a.max(b);
            }
            for (take, child, region) in [
                (high >= 0.0, self.front[node], self.front_region[node]),
                (low < 0.0, self.back[node], self.back_region[node]),
            ] {
                if !take {
                    continue;
                }
                if child < 0 {
                    if !found.contains(&region) {
                        found.push(region);
                    }
                } else {
                    stack.push(child as usize);
                }
            }
        }
        found
    }

    /// The ambient light an actor with this render box takes (`AActor::UpdateRenderData`): the mean of the ambient of the distinct zones
    /// its box reaches, `(0.13, 0.125, 0.12)` when it reaches none, plus `glow` (AmbientGlow / 255) on every channel.
    pub fn actor_ambient(&self, min: [f32; 3], max: [f32; 3], glow: f32) -> [f32; 3] {
        let zones = self.regions_in_box(min, max);
        let mut sum = [0.0f32; 3];
        let mut count = 0.0;
        for z in zones {
            let a = self.ambient(z).unwrap_or([0.0; 3]);
            for (s, v) in sum.iter_mut().zip(a) {
                *s += v;
            }
            count += 1.0;
        }
        if count == 0.0 {
            return [0.13 + glow, 0.125 + glow, 0.12 + glow];
        }
        sum.map(|s| s / count + glow)
    }

    pub fn ambient(&self, region: u16) -> Option<[f32; 3]> {
        self.regions
            .get(usize::from(region))
            .and_then(|r| r.ambient)
    }

    pub fn fog(&self, region: u16) -> Option<Fog> {
        self.regions.get(usize::from(region)).and_then(|r| r.fog)
    }
}

/// The fog to draw with this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveFog {
    pub enabled: bool,
    pub color: [u8; 3],
    pub start: f32,
    pub end: f32,
    /// The frame's clear colour, when the viewer's region clears to its fog.
    pub clear: Option<[u8; 3]>,
}

/// One viewer's fog across region changes.
/// Starts with no fog settled (the sentinel distance), which the original's initial state is read as [O].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogBlend {
    last_zone: u16,
    last_color: [u8; 3],
    last_start: f32,
    last_end: f32,
    seconds: f32,
}

impl Default for FogBlend {
    fn default() -> Self {
        Self {
            last_zone: 0,
            last_color: [0; 3],
            last_start: FAR,
            last_end: FAR,
            seconds: 0.0,
        }
    }
}

impl FogBlend {
    /// This frame's fog for a viewer in `region` after `dt` seconds of game time.
    pub fn step(&mut self, map: &RegionMap, region: u16, dt: f32) -> ActiveFog {
        self.seconds += dt.max(0.0);
        let fog = map.fog(region);
        if self.last_zone != region {
            if let Some(f) = fog
                && self.last_start == FAR
            {
                self.last_color = f.color;
            }
            self.seconds = if self.last_zone == 0 {
                1_000_000.0
            } else {
                0.0
            };
            self.last_zone = region;
        }
        let blend_time = fog.map_or(0.0, |f| f.blend_time);
        let mut ratio = if blend_time == 0.0 {
            1.0
        } else {
            self.seconds / blend_time
        };
        let (target_color, target_start, target_end) = match fog {
            Some(f) => (f.color, f.start, f.end),
            None => (self.last_color, FAR, FAR),
        };
        if ratio > 1.0 || region == 0 {
            self.last_color = target_color;
            self.last_start = target_start;
            self.last_end = target_end;
            self.seconds = 1_000_000.0;
            ratio = 1.0;
        }
        let start = self.last_start + (target_start - self.last_start) * ratio;
        let end = self.last_end + (target_end - self.last_end) * ratio;
        let mut color = [0u8; 3];
        for (out, (target, last)) in color
            .iter_mut()
            .zip(target_color.iter().zip(self.last_color))
        {
            let last = f32::from(last);
            *out = ((f32::from(*target) - last) * ratio + last) as u8;
        }
        ActiveFog {
            enabled: fog.is_some() || ratio < 1.0,
            color,
            start,
            end,
            clear: fog.filter(|f| f.clear_to_fog).map(|f| f.color),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fog(color: u8, start: f32, end: f32, blend: f32) -> Fog {
        Fog {
            color: [color; 3],
            start,
            end,
            blend_time: blend,
            clear_to_fog: true,
        }
    }

    /// A wall at x = 0: region 1 in front (x >= 0), region 2 behind.
    fn two_rooms(a: Option<Fog>, b: Option<Fog>) -> RegionMap {
        RegionMap {
            planes: vec![[1.0, 0.0, 0.0, 0.0]],
            front: vec![-1],
            back: vec![-1],
            front_region: vec![1],
            back_region: vec![2],
            regions: vec![
                Region::default(),
                Region {
                    fog: a,
                    ambient: None,
                    ..Region::default()
                },
                Region {
                    fog: b,
                    ambient: None,
                    ..Region::default()
                },
            ],
            sound: None,
        }
    }

    #[test]
    fn a_point_takes_the_region_of_its_side() {
        let map = two_rooms(None, None);
        map.validate().unwrap();
        assert_eq!(map.locate([3.0, 0.0, 0.0]), 1);
        assert_eq!(map.locate([0.0, 0.0, 0.0]), 1, "the plane itself is front");
        assert_eq!(map.locate([-3.0, 0.0, 0.0]), 2);
        assert_eq!(RegionMap::default().locate([1.0, 2.0, 3.0]), 0);
    }

    #[test]
    fn a_walk_goes_into_the_child_and_keeps_the_last_region() {
        let map = RegionMap {
            planes: vec![[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]],
            front: vec![1, -1],
            back: vec![-1, -1],
            front_region: vec![1, 2],
            back_region: vec![3, 4],
            regions: vec![Region::default(); 5],
            sound: None,
        };
        map.validate().unwrap();
        assert_eq!(map.locate([1.0, 1.0, 0.0]), 2);
        assert_eq!(map.locate([1.0, -1.0, 0.0]), 4);
        assert_eq!(map.locate([-1.0, 5.0, 0.0]), 3);
    }

    #[test]
    fn a_damaged_tree_is_refused() {
        let mut map = two_rooms(None, None);
        map.front = vec![7];
        assert!(map.validate().is_err());
        let mut map = two_rooms(None, None);
        map.back_region = vec![9];
        assert!(map.validate().is_err());
    }

    #[test]
    fn entering_fog_with_no_blend_time_is_at_once() {
        let map = two_rooms(Some(fog(100, 3000.0, 8000.0, 0.0)), None);
        let mut state = FogBlend::default();
        let active = state.step(&map, 1, 0.016);
        assert!(active.enabled);
        assert_eq!((active.start, active.end), (3000.0, 8000.0));
        assert_eq!(active.color, [100; 3]);
        assert_eq!(active.clear, Some([100; 3]));
    }

    #[test]
    fn a_blend_runs_over_the_entered_regions_time() {
        let map = two_rooms(Some(fog(200, 1000.0, 2000.0, 10.0)), None);
        let mut state = FogBlend::default();
        // Settle outside (a region with no fog), then enter the fogged room.
        state.step(&map, 2, 1.0);
        let enter = state.step(&map, 1, 0.0);
        assert!(enter.enabled, "blending in keeps fog on");
        let half = state.step(&map, 1, 5.0);
        assert!(half.enabled, "still blending");
        assert!(half.start > 1000.0 && half.start < FAR, "{half:?}");
        let done = state.step(&map, 1, 6.0);
        assert_eq!((done.start, done.end), (1000.0, 2000.0));
    }

    #[test]
    fn sound_steps_count_doublings_and_never_reach_a_closed_off_region() {
        let room = |connects: &[u16]| Region {
            connects: connects.to_vec(),
            ..Region::default()
        };
        // 0 - 1 - 2 - 3 - 4 in a row, and 5 shut away.
        let map = RegionMap {
            regions: vec![
                room(&[1]),
                room(&[0, 2]),
                room(&[1, 3]),
                room(&[2, 4]),
                room(&[3]),
                room(&[]),
            ],
            ..RegionMap::default()
        };
        map.validate().unwrap();
        let steps = map.zone_steps().unwrap();
        assert_eq!(steps.between(2, 2), Some(0));
        assert_eq!(steps.between(0, 1), Some(1));
        // Two doorways away is the second round; four away, the third (the reach doubles).
        assert_eq!(steps.between(0, 2), Some(2));
        assert_eq!(steps.between(0, 4), Some(3));
        assert_eq!(steps.between(0, 5), None);
        assert_eq!(steps.between(0, 99), None);
        assert!(RegionMap::default().zone_steps().is_none());
    }

    #[test]
    fn a_connection_to_a_missing_region_or_a_bad_carry_is_refused() {
        let mut map = RegionMap {
            regions: vec![Region {
                connects: vec![3],
                ..Region::default()
            }],
            ..RegionMap::default()
        };
        assert!(map.validate().is_err());
        map.regions[0].connects.clear();
        map.sound = Some(SoundCarry {
            zone_decay: 1.5,
            blocked_reference: 0.3,
        });
        assert!(map.validate().is_err());
    }
}
