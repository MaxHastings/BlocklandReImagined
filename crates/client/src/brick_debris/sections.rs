//! Cheap local section motion: one transform, four bounded floor probes,
//! then the existing whole-brick debris. No compound collider or pose stream.
use super::*;
use crate::building::SectionProbe;
use crate::local_physics::GRAVITY;
use std::collections::VecDeque;

pub(super) struct SectionPiece {
    pub cue: u64,
    pub brick: BrickId,
    pub look: Look,
    pub offset: Vec3,
    pub turn: Quat,
    pub half: Vec3,
    pub origin: Vec3,
    pub delay: f32,
}
pub(super) struct Section {
    pub pieces: Vec<SectionPiece>,
    pub age: f32,
    origin: Vec3,
    shown: f32,
    spin: Vec3,
    pub landed: bool,
    pending: VecDeque<SectionProbe>,
    target: f32,
    contact: Option<f32>,
}
impl Section {
    fn pose(&self, age: f32) -> (Vec3, Quat) {
        (
            self.origin + Vec3::NEG_Y * ((0.5 * GRAVITY) * age * age),
            Quat::from_scaled_axis(self.spin * age),
        )
    }
    // Pick corners of actual member bricks. Empty space inside a bounding
    // box never invents a floor contact. Work is bounded by Physics Quality.
    fn feet(&self) -> Vec<Vec3> {
        let mut feet = [Vec3::ZERO; 4];
        let mut scores = [f32::INFINITY; 4];
        let (_, turn) = self.pose(self.shown);
        for p in &self.pieces {
            let center = turn * p.offset;
            let basis = Mat4::from_quat(turn * p.turn);
            let half = basis.x_axis.truncate().abs() * p.half.x
                + basis.y_axis.truncate().abs() * p.half.y
                + basis.z_axis.truncate().abs() * p.half.z;
            for (i, (x, z)) in [(-1., -1.), (-1., 1.), (1., -1.), (1., 1.)]
                .into_iter()
                .enumerate()
            {
                let at = center + Vec3::new(x * half.x, -half.y, z * half.z);
                let score = at.y + 0.05 * (x * at.x + z * at.z);
                if score < scores[i] {
                    scores[i] = score;
                    feet[i] = at;
                }
            }
        }
        feet.to_vec()
    }
}
impl BrickDebris {
    pub(super) fn section_piece_count(&self) -> usize {
        self.sections.values().map(|s| s.pieces.len()).sum()
    }
    pub(super) fn add_section(
        &mut self,
        key: (u64, u64),
        mut pieces: Vec<SectionPiece>,
        _building: &Building,
    ) -> Result<()> {
        if pieces.is_empty() {
            return Ok(());
        }
        let origin = pieces[0].origin;
        let delay = pieces[0].delay;
        // Retired groups remain as small metadata until after cue delivery.
        // A late member never restarts an already landed/evicted section.
        if self
            .sections
            .get(&key)
            .is_some_and(|s| s.landed || s.age >= SOLID_SECONDS + FADE_SECONDS)
        {
            self.diagnostics.skipped += pieces.len() as u64;
            return Ok(());
        }
        if !self.sections.contains_key(&key) && self.sections.len() >= 1024 {
            self.diagnostics.skipped += pieces.len() as u64;
            return Ok(());
        }
        let available = self.limit.saturating_sub(self.section_piece_count());
        if pieces.len() > available {
            self.diagnostics.skipped += (pieces.len() - available) as u64;
            pieces.truncate(available);
        }
        if pieces.is_empty() {
            return Ok(());
        }
        self.diagnostics.evicted += self.evict_to(self.limit - pieces.len());
        let mut rng = Seeded::new(key.1);
        let section = self.sections.entry(key).or_insert_with(|| Section {
            pieces: Vec::new(),
            age: delay,
            shown: delay,
            origin,
            spin: Vec3::new(rng.signed() * 0.15, 0., rng.signed() * 0.15),
            landed: false,
            pending: VecDeque::new(),
            target: delay,
            contact: None,
        });
        for p in &mut pieces {
            p.offset -= origin;
        }
        section.pending.clear();
        section.pieces.extend(pieces);
        section.pieces.sort_by_key(|p| p.cue);
        section.pieces.dedup_by_key(|p| p.cue);
        Ok(())
    }
    pub(super) fn advance_sections(&mut self, dt: f32, building: &Building) -> Result<()> {
        let mut work = 256u32; // Shared candidate budget, never per section.
        let mut landed = Vec::new();
        for (key, s) in &mut self.sections {
            s.age += dt;
            if s.age >= SOLID_SECONDS + FADE_SECONDS {
                s.pieces.clear();
                s.landed = true;
            }
            if s.landed || s.pieces.is_empty() || s.age >= SOLID_SECONDS + FADE_SECONDS {
                continue;
            }
            if s.pending.is_empty() {
                if work < 4 {
                    continue;
                }
                work -= 4;
                s.target = s.age;
                s.contact = None;
                let (old, _) = s.pose(s.shown);
                let (next, _) = s.pose(s.target);
                for foot in s.feet() {
                    // Slight skin catches a section that starts already resting.
                    s.pending.push_back(
                        building.section_probe(old + foot + Vec3::Y * 0.02, next + foot),
                    );
                }
            }
            while let Some(p) = s.pending.front_mut() {
                let Some(hit) = building.step_section_probe(p, &mut work)? else {
                    break;
                };
                if let Some(distance) = hit {
                    s.contact = Some(s.contact.map_or(distance, |d| d.min(distance)));
                }
                s.pending.pop_front();
            }
            if s.pending.is_empty() {
                if let Some(distance) = s.contact {
                    let drop = (0.5 * GRAVITY) * (s.target * s.target - s.shown * s.shown);
                    let fraction = ((distance - 0.02) / drop.max(0.001)).clamped(0., 1.);
                    s.shown = (s.shown * s.shown
                        + (s.target * s.target - s.shown * s.shown) * fraction)
                        .sqrt();
                    s.landed = true;
                    landed.push(*key);
                } else {
                    s.shown = s.target;
                }
            }
        }
        for key in landed {
            let mut s = self.sections.remove(&key).expect("landed section");
            let (center, rotation) = s.pose(s.shown);
            for piece in s.pieces.drain(..) {
                let offset = rotation * piece.offset;
                let at = center + offset;
                let id = (piece.cue, 0);
                self.spawn(
                    id,
                    piece.brick,
                    piece.look,
                    at,
                    rotation * piece.turn,
                    piece.half,
                    None,
                    at,
                    0.,
                    0.,
                    None,
                );
                let body = self.bodies.get_mut(&id).expect("landed brick");
                body.age = s.age;
                let rb = &mut self.world.bodies[body.handle];
                rb.set_linvel(
                    Vector::from_array(
                        (Vec3::NEG_Y * (GRAVITY * s.shown).min(12.) + s.spin.cross(offset))
                            .clamp_length_max(MAX_SPEED)
                            .to_array(),
                    ),
                    true,
                );
                rb.set_angvel(Vector::from_array(s.spin.to_array()), true);
            }
            self.sections.insert(key, s);
        }
        self.sections.retain(|_, s| s.age < 30.);
        Ok(())
    }
    pub(super) fn section_instances(&self) -> impl Iterator<Item = (&Look, SceneTransform)> {
        self.sections
            .values()
            .filter(|s| s.age < SOLID_SECONDS + FADE_SECONDS)
            .flat_map(|s| {
                let (center, turn) = s.pose(s.shown);
                let fade = 1. - ((s.age - SOLID_SECONDS) / FADE_SECONDS).clamped(0., 1.);
                s.pieces.iter().map(move |p| {
                    (
                        &p.look,
                        SceneTransform {
                            transform: Mat4::from_rotation_translation(
                                turn * p.turn,
                                center + turn * p.offset,
                            ),
                            tint: [1., 1., 1., fade],
                        },
                    )
                })
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cue(id: u64, at: [f32; 3], group: u64) -> Cue {
        let mut c = super::super::tests::kill(id, id, at, [0., 3., 0.], 0., 0.);
        if let CueKind::BrickKill { death, .. } = &mut c.kind {
            *death = BrickDeath::Section { group, started: 0 };
        }
        c
    }
    #[test]
    fn section_falls_as_one_then_breaks_into_only_its_own_bricks() -> Result<()> {
        let (building, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.cues(
            &[cue(10, [0., 3., 0.], 7), cue(11, [0., 3.6, 0.], 7)],
            &building,
        )?;
        assert_eq!(d.section_piece_count(), 2);
        assert!(d.bodies.is_empty());
        for _ in 0..12 {
            d.advance(1. / 60., &building)?;
        }
        let at: Vec<_> = d
            .instances()
            .map(|(_, p)| p.transform.w_axis.truncate())
            .collect();
        assert!((at[0].distance(at[1]) - 0.6).abs() < 1e-4);
        for _ in 0..120 {
            d.advance(1. / 60., &building)?;
        }
        assert_eq!(d.section_piece_count(), 0);
        assert_eq!(d.bodies.len(), 2);
        d.cues(&[cue(12, [0., 4., 0.], 7)], &building)?;
        assert_eq!(d.bodies.len(), 2, "late members cannot start a second fall");
        for _ in 0..800 {
            d.advance(1. / 60., &building)?;
        }
        assert_eq!(d.instances().count(), 0);
        Ok(())
    }
    #[test]
    fn section_batches_share_transform_and_quality_cap() -> Result<()> {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(2);
        d.cues(&[cue(10, [0., 3., 0.], 7)], &b)?;
        d.advance(0.05, &b)?;
        d.cues(&[cue(11, [0., 3.6, 0.], 7)], &b)?;
        let at: Vec<_> = d
            .instances()
            .map(|(_, p)| p.transform.w_axis.truncate())
            .collect();
        assert_eq!(at.len(), 2);
        assert!((at[0].distance(at[1]) - 0.6).abs() < 1e-4);
        d.set_limit(0);
        assert_eq!(d.instances().count(), 0);
        Ok(())
    }
}
