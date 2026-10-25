//! Zone portals: which of a map's zones a camera can see.
//!
//! A map that comes from a zone-and-portal engine says which zone each piece
//! of its world belongs to and where the openings between zones are. From
//! the zone the eye is in, the openings that face it and lie inside the view
//! are followed into the next zone with the view narrowed to what shows
//! through the opening, and so on; a zone no chain of openings reaches shows
//! nowhere, so none of its geometry need be drawn.
//!
//! The graph never decides what is hidden by guessing: when the eye's zone is
//! unknown, or the search outgrows its budget, every zone is visible.
use anyhow::{Result, ensure};
use glam::{Vec3, Vec4};
use serde::{Deserialize, Serialize};

/// Most zones and portals one map may declare.
pub const MAX_ZONES: usize = 1 << 12;
pub const MAX_PORTALS: usize = 1 << 16;
/// Most corners one portal polygon may have.
pub const MAX_PORTAL_POINTS: usize = 64;
/// Portal openings followed for one view before the search gives up and
/// calls every zone visible.
const SEARCH_BUDGET: usize = 1 << 14;
/// An eye this close to a portal's plane (world units) is taken to be in the
/// opening: the other side is as visible as this one, through the whole view.
const ON_PORTAL: f32 = 1.0e-3;
/// A clipped portal polygon with a corner pair closer than this has no area
/// worth following.
const DEGENERATE: f32 = 1.0e-9;

/// One zone: its box (when the map names one) and whether it is indoors.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    #[serde(default)]
    pub bounds: Option<[[f32; 3]; 2]>,
    #[serde(default)]
    pub indoor: bool,
}

/// An opening between two zones: a convex polygon in a plane. `plane` is
/// `[nx, ny, nz, w]` with `dot(n, p) - w >= 0` on the `front` zone's side.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Portal {
    pub points: Vec<[f32; 3]>,
    pub plane: [f32; 4],
    pub front: u16,
    pub back: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ZoneGraph {
    /// By zone number, the same numbers as the map's regions.
    pub zones: Vec<Zone>,
    pub portals: Vec<Portal>,
}

/// The zones a view can see.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibleZones {
    seen: Vec<bool>,
}

impl VisibleZones {
    pub fn contains(&self, zone: u16) -> bool {
        // A zone the graph does not know is never hidden.
        self.seen.get(usize::from(zone)).copied().unwrap_or(true)
    }

    pub fn count(&self) -> usize {
        self.seen.iter().filter(|s| **s).count()
    }
}

impl ZoneGraph {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.zones.len() <= MAX_ZONES && self.portals.len() <= MAX_PORTALS,
            "A zone graph is too large"
        );
        for z in &self.zones {
            if let Some(b) = z.bounds {
                ensure!(
                    b.iter().flatten().all(|v| v.is_finite()),
                    "A zone's bounds are not finite"
                );
            }
        }
        for p in &self.portals {
            ensure!(
                (3..=MAX_PORTAL_POINTS).contains(&p.points.len())
                    && p.points.iter().flatten().all(|v| v.is_finite())
                    && p.plane.iter().all(|v| v.is_finite())
                    && Vec3::new(p.plane[0], p.plane[1], p.plane[2]).length_squared() > 0.0,
                "A zone portal has no usable polygon or plane"
            );
            ensure!(
                usize::from(p.front) < self.zones.len() && usize::from(p.back) < self.zones.len(),
                "A zone portal joins a missing zone"
            );
        }
        Ok(())
    }

    /// The zones visible from `eye` (standing in `eye_zone`) through
    /// `frustum`, whose planes face inward: a point is inside when
    /// `dot(plane.xyz, p) + plane.w >= 0` for each.
    pub fn visible(&self, eye_zone: u16, eye: Vec3, frustum: &[Vec4]) -> VisibleZones {
        self.visible_within(eye_zone, eye, frustum, SEARCH_BUDGET)
    }

    fn visible_within(
        &self,
        eye_zone: u16,
        eye: Vec3,
        frustum: &[Vec4],
        budget: usize,
    ) -> VisibleZones {
        let everything = || VisibleZones {
            seen: vec![true; self.zones.len()],
        };
        if usize::from(eye_zone) >= self.zones.len() {
            return everything();
        }
        let mut search = Search {
            graph: self,
            eye,
            seen: vec![false; self.zones.len()],
            on_path: vec![false; self.zones.len()],
            budget,
        };
        if search.enter(eye_zone, frustum) {
            VisibleZones { seen: search.seen }
        } else {
            everything()
        }
    }
}

struct Search<'a> {
    graph: &'a ZoneGraph,
    eye: Vec3,
    seen: Vec<bool>,
    on_path: Vec<bool>,
    budget: usize,
}

impl Search<'_> {
    /// Marks `zone` seen and follows its openings. False when the budget ran out.
    fn enter(&mut self, zone: u16, frustum: &[Vec4]) -> bool {
        let index = usize::from(zone);
        self.seen[index] = true;
        self.on_path[index] = true;
        let mut finished = true;
        for portal in &self.graph.portals {
            let (other, from_front) = if portal.front == zone {
                (portal.back, true)
            } else if portal.back == zone {
                (portal.front, false)
            } else {
                continue;
            };
            if self.on_path[usize::from(other)] {
                continue;
            }
            if self.budget == 0 {
                finished = false;
                break;
            }
            self.budget -= 1;
            let normal = Vec3::new(portal.plane[0], portal.plane[1], portal.plane[2]);
            let side = (normal.dot(self.eye) - portal.plane[3]) / normal.length();
            let narrowed = if side.abs() <= ON_PORTAL {
                Some(frustum.to_vec())
            } else if (side > 0.0) != from_front {
                // The eye is behind the opening as seen from this zone.
                None
            } else {
                self.through(portal, frustum)
            };
            if let Some(next) = narrowed
                && !self.enter(other, &next)
            {
                finished = false;
                break;
            }
        }
        self.on_path[index] = false;
        finished
    }

    /// The view narrowed to what shows through `portal`, or `None` when none does.
    fn through(&self, portal: &Portal, frustum: &[Vec4]) -> Option<Vec<Vec4>> {
        let mut polygon: Vec<Vec3> = portal.points.iter().map(|p| Vec3::from(*p)).collect();
        for plane in frustum {
            polygon = clip(&polygon, *plane);
            if polygon.len() < 3 {
                return None;
            }
        }
        let center = polygon.iter().sum::<Vec3>() / polygon.len() as f32;
        let mut narrowed = Vec::with_capacity(polygon.len());
        for (i, a) in polygon.iter().enumerate() {
            let b = polygon[(i + 1) % polygon.len()];
            let normal = (*a - self.eye).cross(b - self.eye);
            if normal.length_squared() <= DEGENERATE {
                continue;
            }
            let normal = normal.normalize();
            // Facing the polygon's middle, the way the other planes face inward.
            let sign = if normal.dot(center - self.eye) >= 0.0 {
                1.0
            } else {
                -1.0
            };
            narrowed.push((normal * sign).extend(-(normal * sign).dot(self.eye)));
        }
        (narrowed.len() >= 3).then_some(narrowed)
    }
}

/// `polygon` clipped to the inside of `plane` (Sutherland-Hodgman).
fn clip(polygon: &[Vec3], plane: Vec4) -> Vec<Vec3> {
    let distance = |p: Vec3| plane.truncate().dot(p) + plane.w;
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let (da, db) = (distance(*a), distance(b));
        if da >= 0.0 {
            out.push(*a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(*a + (b - *a) * (da / (da - db)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inward-facing planes of a perspective view from `eye` towards `target`.
    fn frustum(eye: Vec3, target: Vec3) -> Vec<Vec4> {
        let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(1.2, 1.0, 0.05, 1000.0);
        let m = projection * view;
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        vec![r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
    }

    /// A doorway in the plane `x = at`, `width` wide and 3 high, between the
    /// zone on the `-x` side (`back`) and the one on the `+x` side (`front`).
    fn doorway(at: f32, z: f32, width: f32, back: u16, front: u16) -> Portal {
        let h = width / 2.0;
        Portal {
            points: vec![
                [at, 0.0, z - h],
                [at, 0.0, z + h],
                [at, 3.0, z + h],
                [at, 3.0, z - h],
            ],
            plane: [1.0, 0.0, 0.0, at],
            front,
            back,
        }
    }

    /// Rooms A, B, C along +x: A|B doorway at x=10 on the z=0 axis, B|C at
    /// x=20 but offset to z=40, and D off to one side of A with no opening.
    fn rooms() -> ZoneGraph {
        ZoneGraph {
            zones: vec![Zone::default(); 4],
            portals: vec![
                doorway(10.0, 0.0, 2.0, 0, 1),
                doorway(20.0, 40.0, 2.0, 1, 2),
            ],
        }
    }

    fn seen(graph: &ZoneGraph, zone: u16, eye: Vec3, target: Vec3) -> Vec<bool> {
        let visible = graph.visible(zone, eye, &frustum(eye, target));
        (0..graph.zones.len() as u16)
            .map(|z| visible.contains(z))
            .collect()
    }

    #[test]
    fn a_zone_is_seen_through_an_opening_in_view() {
        let eye = Vec3::new(0.0, 1.5, 0.0);
        let seen = seen(&rooms(), 0, eye, Vec3::new(30.0, 1.5, 0.0));
        // B shows through the first doorway; C's doorway is far off the axis.
        assert_eq!(seen, [true, true, false, false]);
    }

    #[test]
    fn looking_away_from_every_opening_sees_only_the_eyes_zone() {
        let eye = Vec3::new(0.0, 1.5, 0.0);
        let seen = seen(&rooms(), 0, eye, Vec3::new(-30.0, 1.5, 0.0));
        assert_eq!(seen, [true, false, false, false]);
    }

    #[test]
    fn the_view_narrows_through_each_opening_in_turn() {
        // Standing in B looking along the line from the A|B doorway into
        // C's offset doorway: aimed at it, C shows.
        let graph = rooms();
        let eye = Vec3::new(12.0, 1.5, 0.0);
        let at_c = seen(&graph, 1, eye, Vec3::new(20.0, 1.5, 40.0));
        assert_eq!(at_c, [false, true, true, false]);
        let elsewhere = seen(&graph, 1, eye, Vec3::new(30.0, 1.5, -40.0));
        assert_eq!(elsewhere, [false, true, false, false]);
    }

    #[test]
    fn a_portal_seen_from_behind_does_not_open() {
        // The eye stands in A but A's zone number is the doorway's front
        // side: the same opening, seen from the wrong side, shows nothing.
        let graph = ZoneGraph {
            zones: vec![Zone::default(); 2],
            portals: vec![doorway(10.0, 0.0, 2.0, 1, 0)],
        };
        let eye = Vec3::new(0.0, 1.5, 0.0);
        let seen = seen(&graph, 0, eye, Vec3::new(30.0, 1.5, 0.0));
        assert_eq!(seen, [true, false]);
    }

    #[test]
    fn an_eye_in_the_opening_sees_both_sides() {
        let eye = Vec3::new(10.0, 1.5, 0.0);
        let seen = seen(&rooms(), 0, eye, Vec3::new(30.0, 1.5, 0.0));
        assert!(seen[0] && seen[1]);
    }

    #[test]
    fn an_unknown_eye_zone_hides_nothing() {
        let graph = rooms();
        let eye = Vec3::ZERO;
        let visible = graph.visible(99, eye, &frustum(eye, Vec3::X));
        assert_eq!(visible.count(), graph.zones.len());
        assert!(
            visible.contains(500),
            "a zone the graph lacks is never hidden"
        );
    }

    #[test]
    fn a_search_over_its_budget_falls_back_to_every_zone_visible() {
        let graph = rooms();
        let eye = Vec3::new(12.0, 1.5, 0.0);
        let view = frustum(eye, Vec3::new(20.0, 1.5, 40.0));
        assert_eq!(graph.visible_within(1, eye, &view, 8).count(), 2);
        // One opening followed, then no budget for C's: nothing is culled.
        assert_eq!(graph.visible_within(1, eye, &view, 0).count(), 4);
    }

    #[test]
    fn validate_rejects_a_portal_to_a_missing_zone() {
        let mut graph = rooms();
        assert!(graph.validate().is_ok());
        graph.portals[0].back = 9;
        assert!(graph.validate().is_err());
    }
}
