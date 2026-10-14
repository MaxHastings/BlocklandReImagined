//! Contacts of bodies part way through an opening (a portal,
//! [`bri_content::passage`]). A body whose middle is still in front of an
//! opening is drawn and carried as one piece there, but the part of it past
//! the opening's plane is really beyond the partner. So that part must not
//! touch what stands behind the opening here (a wall the portal is set
//! against, a car parked behind a free-standing one), and what stands
//! beyond the partner must touch it instead.
//!
//! For the length of one physics step, the owner of such a body brings what
//! stands beyond the partner back through the opening ([`fill`]): copies of
//! those colliders, carried back to lie past the opening's plane here,
//! which touch nothing but that body. This hook then filters the step's
//! solver contacts: the body keeps only the contacts outside the opening's
//! prism behind its plane, and the copies only those inside it. One solve
//! meets both sides at once, as walking players' collision soup does
//! (`Soup::open_passages` in bri-motor).
use bri_content::passage::{Passage, Passages};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::HashMap;

/// What a collider listed in [`PortalContacts`] keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rule {
    /// A body part way into the opening: all but what lies past its plane
    /// inside its rectangle.
    Here(Passage),
    /// A copy of something beyond the opening's partner, brought back: only
    /// what lies past the plane, and only against the one body.
    Fill(Passage, RigidBodyHandle),
    /// A kinematic body's stand-in beyond the partner ([`twin`]): only what,
    /// carried back, lies past the plane.
    Twin(Passage),
}

/// The solver-contact rules of the bodies part way through openings this
/// step: a [`PhysicsHooks`] for the step that knows them. Colliders it
/// lists need [`ACTIVE_HOOKS`] while listed.
#[derive(Clone, Debug, Default)]
pub struct PortalContacts {
    rules: HashMap<ColliderHandle, Vec<Rule>>,
}

/// The hooks a collider listed in [`PortalContacts`] needs.
pub const ACTIVE_HOOKS: ActiveHooks =
    ActiveHooks::FILTER_CONTACT_PAIRS.union(ActiveHooks::MODIFY_SOLVER_CONTACTS);

/// Whether `p` lies past `opening`'s plane within its rectangle (the prism
/// a body's far part fills).
pub fn past(opening: &Passage, p: Vec3) -> bool {
    opening.side(p) < 0.0 && opening.within(p, 0.0)
}

impl PortalContacts {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
    pub fn clear(&mut self) {
        self.rules.clear();
    }
    /// `colliders` (of one body) are part way into `opening`.
    pub fn cut(&mut self, opening: &Passage, colliders: &[ColliderHandle]) {
        for &c in colliders {
            self.rules.entry(c).or_default().push(Rule::Here(*opening));
        }
    }
    /// `fill` is a copy brought back through `opening` for `body` alone.
    pub fn fill(&mut self, opening: &Passage, fill: ColliderHandle, body: RigidBodyHandle) {
        self.rules
            .entry(fill)
            .or_default()
            .push(Rule::Fill(*opening, body));
    }
    /// Everything `other` lists too.
    pub fn extend(&mut self, other: &PortalContacts) {
        for (c, rules) in &other.rules {
            self.rules
                .entry(*c)
                .or_default()
                .extend(rules.iter().copied());
        }
    }
    /// Whether `collider` may touch a collider of `other` at all.
    fn pairs(&self, collider: ColliderHandle, other: Option<RigidBodyHandle>) -> bool {
        self.rules.get(&collider).is_none_or(|rules| {
            rules.iter().all(|rule| match rule {
                Rule::Here(_) | Rule::Twin(_) => true,
                Rule::Fill(_, body) => other == Some(*body),
            })
        })
    }
    /// Whether `collider` is a fill.
    fn is_fill(&self, collider: ColliderHandle) -> bool {
        self.rules
            .get(&collider)
            .is_some_and(|rules| rules.iter().any(|r| matches!(r, Rule::Fill(..))))
    }
    /// Whether a contact at `point` counts for `collider`, touching `other`.
    /// Against a fill, the part past the plane is just what touches.
    fn keeps(&self, collider: ColliderHandle, other: ColliderHandle, point: Vec3) -> bool {
        let fill = self.is_fill(other);
        self.rules.get(&collider).is_none_or(|rules| {
            rules.iter().all(|rule| match rule {
                Rule::Here(opening) => fill || !past(opening, point),
                Rule::Fill(opening, _) => {
                    opening.side(point) < 0.0 && opening.within(point, FILL_MARGIN)
                }
                Rule::Twin(opening) => {
                    let back = opening.carry.inverse().transform_point3(point);
                    opening.side(back) < 0.0 && opening.within(back, FILL_MARGIN)
                }
            })
        })
    }
}

/// How far outside an opening's rectangle a contact past its plane still
/// counts against a fill. A body that fits through touches only within it,
/// but its contact points lie on the edge, within rounding; the rim's own
/// copy beyond the partner agrees with the rim here where they overlap.
const FILL_MARGIN: f32 = 0.3;

impl PhysicsHooks for PortalContacts {
    fn filter_contact_pair(&self, context: &PairFilterContext) -> Option<SolverFlags> {
        let ok = self.pairs(context.collider1, context.rigid_body2)
            && self.pairs(context.collider2, context.rigid_body1);
        ok.then(SolverFlags::default)
    }
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let (c1, c2) = (context.collider1, context.collider2);
        if !self.rules.contains_key(&c1) && !self.rules.contains_key(&c2) {
            return;
        }
        let Some(manifold) = context.rigid_mut() else {
            return;
        };
        manifold.solver_contacts.retain(|contact| {
            let at = (contact.anchor1 + contact.anchor2) * 0.5;
            self.keeps(c1, c2, at) && self.keeps(c2, c1, at)
        });
    }
}

/// The openings `body` is part way into (or will be by the end of the
/// step): its middle is in front, and its bounds, grown by a step's travel,
/// reach past the plane within the opening.
pub fn straddled(world: &PhysicsWorld, body: RigidBodyHandle, passages: &Passages) -> Vec<Passage> {
    let b = &world.bodies[body];
    let Some((min, max)) = b
        .colliders()
        .iter()
        .map(|c| world.colliders[*c].compute_aabb())
        .map(|a| (a.mins, a.maxs))
        .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    else {
        return vec![];
    };
    let ahead = Vec3::splat(b.linvel().length() * world.integration_parameters.dt + 0.05);
    let (min, max) = (min - ahead, max + ahead);
    let reach = (max - min).length();
    passages
        .near(b.center_of_mass(), reach)
        .filter(|o| {
            let (lo, hi) = o.bounds(0.0);
            let overlaps = min.cmple(hi).all() && max.cmpge(lo).all();
            let corners = (0..8).map(|i| {
                Vec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                )
            });
            overlaps && corners.into_iter().any(|p| o.side(p) < 0.0)
        })
        .copied()
        .collect()
}
/// A kinematic body's stand-in beyond the partner of an opening it is part
/// way into, for one step: its colliders carried there, moving as it does,
/// so free bodies beyond meet the part of it past the plane. Listed in
/// `rules` with the body's own cut. Take it out with
/// [`PhysicsWorld::remove_body`] after the step.
pub fn twin(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    opening: &Passage,
    rules: &mut PortalContacts,
) -> (RigidBodyHandle, Vec<ColliderHandle>) {
    let carry = opening.carry;
    let (_, turn, _) = carry.to_scale_rotation_translation();
    let moved = |p: &Pose| {
        Pose::from_parts(
            carry.transform_point3(p.translation),
            (turn * p.rotation).normalize(),
        )
    };
    let source = &world.bodies[body];
    let (now, next) = (moved(source.position()), moved(source.next_position()));
    let own = source.colliders().to_vec();
    let copies: Vec<Collider> = own
        .iter()
        .map(|c| {
            let mut copy = world.colliders[*c].clone();
            copy.set_active_hooks(ACTIVE_HOOKS);
            copy.set_active_events(ActiveEvents::empty());
            copy
        })
        .collect();
    let mut stand_in = RigidBodyBuilder::kinematic_position_based()
        .pose(now)
        .build();
    stand_in.set_next_kinematic_position(next);
    let handle = world.bodies.insert(stand_in);
    let colliders: Vec<ColliderHandle> = copies
        .into_iter()
        .map(|c| {
            world
                .colliders
                .insert_with_parent(c, handle, &mut world.bodies)
        })
        .collect();
    rules.cut(opening, &own);
    for c in &colliders {
        rules
            .rules
            .entry(*c)
            .or_default()
            .push(Rule::Twin(*opening));
    }
    (handle, colliders)
}

/// A collider beyond the partner of an opening, brought back through it
/// for one step, for the one body part way into the opening.
#[derive(Clone, Copy, Debug)]
pub struct Fill {
    /// The copy, lying past the opening's plane.
    pub collider: ColliderHandle,
    /// What it copies, and that collider's body.
    pub source: ColliderHandle,
    pub source_body: Option<RigidBodyHandle>,
}

/// Bring what stands beyond `opening`'s partner, near `body`'s part past
/// the plane, back through it for the next step: fixed copies of those
/// colliders, listed in `rules` to touch only `body`'s colliders and only
/// past the plane. `skip` are colliders never copied (other bodies' fills).
/// Take them out with [`unfill`] after the step.
pub fn fill(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    opening: &Passage,
    skip: &[ColliderHandle],
    rules: &mut PortalContacts,
) -> Vec<Fill> {
    let b = &world.bodies[body];
    let Some((min, max)) = b
        .colliders()
        .iter()
        .map(|c| world.colliders[*c].compute_aabb())
        .map(|a| (a.mins, a.maxs))
        .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
    else {
        return vec![];
    };
    // Whatever the step can bring it to.
    let ahead = Vec3::splat(b.linvel().length() * world.integration_parameters.dt + 0.1);
    let (min, max) = (min - ahead, max + ahead);
    let carry = opening.carry;
    let (lo, hi) = (0..8)
        .map(|i| {
            carry.transform_point3(Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            ))
        })
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
    // What the body meets at all (copies keep their groups, so the pair
    // test agrees).
    let groups = b.colliders().first().map_or(InteractionGroups::all(), |c| {
        world.colliders[*c].collision_groups()
    });
    let found: Vec<ColliderHandle> = world
        .query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_rigid_body(body)
                .exclude_sensors()
                .groups(groups),
        )
        .intersect_aabb_conservative(Aabb::new(lo, hi))
        .map(|(handle, _)| handle)
        .filter(|h| !skip.contains(h))
        .collect();
    let back = carry.inverse();
    let (_, turn, _) = back.to_scale_rotation_translation();
    let mut fills = Vec::with_capacity(found.len());
    for source in found {
        let original = &world.colliders[source];
        let pose = *original.position();
        let source_body = original.parent();
        let mut copy = original.clone();
        copy.set_position(Pose::from_parts(
            back.transform_point3(pose.translation),
            (turn * pose.rotation).normalize(),
        ));
        copy.set_active_hooks(ACTIVE_HOOKS);
        copy.set_active_events(ActiveEvents::empty());
        let collider = world.colliders.insert(copy);
        rules.fill(opening, collider, body);
        fills.push(Fill {
            collider,
            source,
            source_body,
        });
    }
    fills
}

/// The impulse the solver gave `body` against `fill` in the last step (from
/// the fill toward the body), and the point it acted at on average.
pub fn impulse(world: &PhysicsWorld, fill: &Fill, body: RigidBodyHandle) -> Option<(Vec3, Vec3)> {
    let pair = world.contact_pairs_with(fill.collider).find(|p| {
        let other = if p.collider1 == fill.collider {
            p.collider2
        } else {
            p.collider1
        };
        world.colliders.get(other).and_then(|c| c.parent()) == Some(body)
    })?;
    // Each manifold's normal points from collider1 to collider2: what
    // collider2's body was pushed along.
    let first = pair.collider1 == fill.collider;
    let sign = if first { 1.0 } else { -1.0 };
    let pose = *world.colliders[fill.collider].position();
    let (mut total, mut at, mut weight) = (Vec3::ZERO, Vec3::ZERO, 0.0);
    for m in pair.manifolds() {
        for p in &m.points {
            let j = p.data.impulse;
            if j > 0.0 {
                total += m.data.normal * (j * sign);
                let local = if first { p.local_p1 } else { p.local_p2 };
                at += pose.transform_point(local) * j;
                weight += j;
            }
        }
    }
    (weight > 0.0 && total.is_finite()).then(|| (total, at / weight))
}

/// Take a step's fills out of the world again.
pub fn unfill(world: &mut PhysicsWorld, fills: impl IntoIterator<Item = Fill>) {
    for fill in fills {
        world.remove_collider(fill.collider);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Affine3A, Vec2};

    /// An opening in the plane x = 0, its front toward -x, carried 100
    /// along x: what goes in comes out at x = 100.
    fn opening() -> Passage {
        Passage {
            brick: 1,
            centre: Vec3::new(0.0, 2.0, 0.0),
            normal: Vec3::NEG_X,
            u: Vec3::Z,
            v: Vec3::Y,
            half: Vec2::new(3.0, 2.0),
            carry: Affine3A::from_translation(Vec3::new(100.0, 0.0, 0.0)),
        }
    }

    /// A cube sent along +x into the opening with `fixed` colliders about,
    /// stepped `ticks` with its contacts filtered: where it ends.
    fn run(fixed: Vec<ColliderBuilder>, ticks: usize) -> Vec3 {
        let mut world = crate::new_world();
        for c in fixed {
            world.colliders.insert(c);
        }
        let body = world.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vec3::new(-2.0, 2.0, 0.0))
                .linvel(Vec3::new(10.0, 0.0, 0.0))
                .gravity_scale(0.0),
        );
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0).active_hooks(ACTIVE_HOOKS),
            body,
            &mut world.bodies,
        );
        let o = opening();
        for _ in 0..ticks {
            let mut rules = PortalContacts::default();
            rules.cut(&o, world.bodies[body].colliders());
            let fills = fill(&mut world, body, &o, &[], &mut rules);
            world.step_with_events(&rules, &());
            unfill(&mut world, fills);
        }
        world.bodies[body].translation()
    }

    #[test]
    fn a_wall_beyond_the_partner_stops_the_part_past_the_plane() {
        // A wall 2 past the partner: the cube's front meets it at x = 2.
        let wall = ColliderBuilder::cuboid(0.5, 5.0, 5.0).translation(Vec3::new(102.5, 2.0, 0.0));
        let at = run(vec![wall], 120);
        // As the same wall would, standing right there.
        let plain = ColliderBuilder::cuboid(0.5, 5.0, 5.0).translation(Vec3::new(2.5, 2.0, 0.0));
        let mut world = crate::new_world();
        world.colliders.insert(plain);
        let body = world.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vec3::new(-2.0, 2.0, 0.0))
                .linvel(Vec3::new(10.0, 0.0, 0.0))
                .gravity_scale(0.0),
        );
        world.colliders.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 1.0, 1.0),
            body,
            &mut world.bodies,
        );
        for _ in 0..120 {
            world.step();
        }
        let there = world.bodies[body].translation();
        // (Rapier's own solve leaves the stopped cube a slight drift aside,
        // either way: along the opening the two stop alike.)
        assert!((at.x - there.x).abs() < 0.05, "{at} against {there}");
    }

    #[test]
    fn a_wall_behind_the_opening_is_not_met() {
        let wall = ColliderBuilder::cuboid(0.5, 5.0, 5.0).translation(Vec3::new(2.5, 2.0, 0.0));
        let at = run(vec![wall], 60);
        assert!(at.x > 2.9, "{at}");
    }
}
