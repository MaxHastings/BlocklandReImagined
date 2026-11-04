use super::*;

#[test]
fn collision_budget_reaches_the_tail_of_a_large_bodyless_collapse() -> Result<()> {
    let (building, _) = super::super::tests::building(&[]);
    let mut debris = BrickDebris::new();
    debris.set_limit(0);
    // More independent clumps than one frame can sweep. One lower foot has
    // already reached the floor while the upper material still needs to fall.
    let count = (2 * COLLISION_QUERIES_PER_STEP) as usize;
    let mut pieces: Vec<_> = (0..count)
        .map(|i| {
            super::landing_regressions::piece(
                i as u64 + 1,
                Vec3::new((i % 32) as f32 * 2. - 31., 3.3, (i / 32) as f32 * 2. - 15.),
            )
        })
        .collect();
    pieces.push(super::landing_regressions::piece(
        count as u64 + 1,
        Vec3::Y * 0.3,
    ));
    for piece in &mut pieces {
        piece.members = count + 1;
        piece.origin = Vec3::Y * 1.8;
        piece.section_half = Vec3::new(32., 1.8, 16.);
        piece.pivot = Vec3::ZERO;
    }
    debris.add_section((0, 1), pieces, &building)?;
    debris.sections.get_mut(&(0, 1)).unwrap().sealed = true;
    let frames = 120;
    for _ in 0..frames {
        debris.advance(1. / 60., &building)?;
    }
    let section = &debris.sections[&(0, 1)];
    assert_eq!(section.chunks.len(), count + 1);
    assert_eq!(debris.instances().count(), count + 1);
    assert_eq!(debris.chunk_handles().count(), 0);
    let hovering: Vec<_> = section
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.pose(&debris.world).0.y > 1.3)
        .map(|(i, c)| (i, c.center.y, c.debt))
        .collect();
    assert!(
        hovering.is_empty(),
        "tail chunks were starved of collision work: {hovering:?}"
    );
    let work = debris.diagnostics.section_preparation_work + debris.diagnostics.section_motion_work;
    assert!(
        work <= u64::from(
            frames * 2 * COLLISION_QUERIES_PER_STEP * crate::local_physics::COLLISION_QUERY_COST
        ),
        "fairness must not increase the work envelope"
    );
    Ok(())
}
