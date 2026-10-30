use super::*;

fn pressure_section(d: &mut BrickDebris, b: &Building, positions: &[Vec3]) -> Result<()> {
    let pieces: Vec<_> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut piece = landing_regressions::piece(i as u64 + 1, *p);
            piece.members = positions.len();
            piece
        })
        .collect();
    d.add_section((0, 1), pieces, b)?;
    let s = d.sections.get_mut(&(0, 1)).unwrap();
    s.sealed = true;
    // Isolate post-contact independent motion: every group still starts above
    // the floor. The ordinary mast tests cover arrival through shared flight.
    s.landed = true;
    Ok(())
}
fn actor(center: Vec3) -> Pusher {
    Pusher {
        id: 9,
        center,
        rotation: Quat::IDENTITY,
        half: Vec3::new(0.5, 1.2, 0.5),
        shape: None,
    }
}
fn centers(d: &BrickDebris) -> Vec<Vec3> {
    d.instances()
        .map(|(_, t)| t.transform.w_axis.truncate())
        .collect()
}
fn run(d: &mut BrickDebris, b: &Building, frames: usize) -> Result<()> {
    for _ in 0..frames {
        d.advance(1. / 60., b)?;
    }
    Ok(())
}

#[test]
fn zero_pool_still_falls_lands_and_is_pushed_against_a_real_wall() -> Result<()> {
    let (mut b, mut world) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    d.set_limit(0);
    pressure_section(&mut d, &b, &[Vec3::new(0., 3.3, 0.)])?;
    run(&mut d, &b, 120)?;
    let before = centers(&d)[0];
    assert!(
        (0.25..0.4).contains(&before.y),
        "must land, not hover or tunnel: {before}"
    );
    assert_eq!(d.chunk_handles().count(), 0);
    for frame in 0..90 {
        d.push(&[actor(Vec3::new(-2. + frame as f32 / 30., 1.2, 0.))]);
        d.advance(1. / 60., &b)?;
    }
    let pushed = centers(&d)[0];
    assert!(
        pushed.x > before.x + 0.5,
        "bodyless checked motion must react: {before} -> {pushed}"
    );
    // Add a tall wall after preparation; the next player push must not tunnel.
    for y in 0..7 {
        world.bricks.insert(
            100 + y,
            bri_world::Brick::new(
                bri_world::ContentRef::Resolved("brick".into()),
                [3., 0.3 + y as f32 * 0.6, 0.],
                1,
            ),
        );
    }
    b.sync_world(&world)?;
    for frame in 0..90 {
        d.push(&[actor(Vec3::new(1. + frame as f32 / 45., 1.2, 0.))]);
        d.advance(1. / 60., &b)?;
    }
    let stopped = centers(&d)[0];
    assert!(stopped.x < 2.1, "new wall must remain solid: {stopped}");
    assert!(stopped.y >= 0.25);
    assert_eq!(
        d.world.bodies.len(),
        d.pushers.len(),
        "fallback buys no dynamic body"
    );
    Ok(())
}

#[test]
fn one_free_slot_does_not_weld_a_large_collapse_and_eviction_keeps_gravity() -> Result<()> {
    let (b, _) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    d.set_limit(1);
    let positions: Vec<_> = (0..256)
        .map(|i| Vec3::new((i % 16) as f32, 5.3 + (i / 16) as f32 * 0.6, 0.))
        .collect();
    pressure_section(&mut d, &b, &positions)?;
    run(&mut d, &b, 20)?;
    let s = &d.sections[&(0, 1)];
    assert!(s.chunks.len() > 1);
    for chunk in 0..s.chunks.len() {
        assert!(s.assignment.iter().filter(|a| **a == Some(chunk)).count() <= 16);
        assert!(s.chunks[chunk].half.max_element() * 2. <= 6.001);
    }
    let old = centers(&d);
    d.set_limit(0);
    run(&mut d, &b, 90)?;
    let new = centers(&d);
    assert_eq!(new.len(), 256);
    assert!(
        new.iter().zip(&old).all(|(a, b)| a.y < b.y - 0.1),
        "every group keeps gravity under eviction"
    );
    assert!(new.iter().all(|p| p.y > 0.2), "no floor tunneling");
    assert_eq!(d.chunk_handles().count(), 0);
    d.set_limit(8);
    run(&mut d, &b, 60)?;
    assert!(d.chunk_handles().count() > 0 && d.chunk_handles().count() <= 8);
    assert_eq!(centers(&d).len(), 256);
    Ok(())
}

#[test]
fn a_player_at_the_end_of_a_large_body_is_admitted() -> Result<()> {
    let (b, _) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    // An individual authored member may itself be long. Admission must use
    // extents even though grouping no longer welds many bricks into a beam.
    let mut p = landing_regressions::piece(1, Vec3::new(0., 0.3, 0.));
    p.half.x = 12.;
    d.add_section((0, 1), vec![p], &b)?;
    let s = d.sections.get_mut(&(0, 1)).unwrap();
    s.sealed = true;
    s.landed = true;
    run(&mut d, &b, 10)?;
    assert_eq!(d.chunk_handles().count(), 1);
    d.push(&[actor(Vec3::new(11.5, 1.2, 0.))]);
    assert_eq!(
        d.pushers.len(),
        1,
        "end contact is not a center-distance query"
    );
    let mut crowd: Vec<_> = (0..32)
        .map(|id| {
            let mut p = actor(Vec3::new(0., 5.8, 0.));
            p.id = id;
            p
        })
        .collect();
    let mut near = actor(Vec3::new(11.5, 1.2, 0.));
    near.id = 999;
    crowd.push(near);
    d.push(&crowd);
    assert!(
        d.pushers.handle(999).is_some(),
        "nearby actor must not lose its resident contact to farther low IDs"
    );
    assert_eq!(d.pushers.len(), 32);
    Ok(())
}

#[test]
fn fallback_loses_support_and_never_glues_disconnected_bricks() -> Result<()> {
    let (mut b, mut world) = super::super::tests::building(&[(100, [0., 0.3, 0.])]);
    let mut d = BrickDebris::new();
    d.set_limit(0);
    pressure_section(
        &mut d,
        &b,
        &[Vec3::new(0., 2.1, 0.), Vec3::new(3., 2.1, 0.)],
    )?;
    run(&mut d, &b, 100)?;
    assert_eq!(d.sections[&(0, 1)].chunks.len(), 2);
    let on_support = centers(&d);
    assert!(
        on_support[0].y > 0.8 && on_support[1].y < 0.4,
        "independent support: {on_support:?}"
    );
    world.bricks.clear();
    b.sync_world(&world)?;
    run(&mut d, &b, 60)?;
    assert!(
        centers(&d).iter().all(|p| (0.25..0.4).contains(&p.y)),
        "support removal must resume falling"
    );
    Ok(())
}

#[test]
fn pressure_chunks_collide_with_each_other_and_with_resident_debris() -> Result<()> {
    for slots in [0, 1] {
        let (b, _) = super::super::tests::building(&[]);
        let mut d = BrickDebris::new();
        d.set_limit(slots);
        pressure_section(
            &mut d,
            &b,
            &[
                Vec3::new(0., 3.3, 0.),
                Vec3::new(0., 5.1, 0.),
                Vec3::new(0., 6.9, 0.),
            ],
        )?;
        run(&mut d, &b, 240)?;
        let mut y: Vec<_> = centers(&d).iter().map(|p| p.y).collect();
        y.sort_by(f32::total_cmp);
        assert!(
            y[0] > 0.2 && y[2] < 2.,
            "all pieces settle: slots={slots} {y:?}"
        );
        assert!(
            y.windows(2).all(|p| p[1] - p[0] > 0.5),
            "rubble must not ghost through rubble: slots={slots} {y:?}"
        );
        assert!(d.chunk_handles().count() <= slots);
    }
    Ok(())
}

#[test]
fn pressure_actor_contact_preserves_real_compound_openings() -> Result<()> {
    let (b, _) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    d.set_limit(0);
    let positions = [
        Vec3::new(-1., 0.3, 0.),
        Vec3::new(-1., 0.9, 0.),
        Vec3::new(-1., 1.5, 0.),
        Vec3::new(0., 1.5, 0.),
        Vec3::new(1., 1.5, 0.),
        Vec3::new(1., 0.9, 0.),
        Vec3::new(1., 0.3, 0.),
    ];
    pressure_section(&mut d, &b, &positions)?;
    run(&mut d, &b, 90)?;
    assert_eq!(d.sections[&(0, 1)].chunks.len(), 1);
    let before = centers(&d);
    for frame in 0..120 {
        let mut p = actor(Vec3::new(0., 0.4, -2. + frame as f32 / 30.));
        p.half = Vec3::new(0.15, 0.4, 0.15);
        d.push(&[p]);
        d.advance(1. / 60., &b)?;
    }
    assert!(
        centers(&d)
            .iter()
            .zip(&before)
            .all(|(a, b)| a.distance(*b) < 0.05),
        "walking through empty arch space must not shove an invisible box"
    );
    for frame in 0..120 {
        d.push(&[actor(Vec3::new(1., 1.2, -2. + frame as f32 / 30.))]);
        d.advance(1. / 60., &b)?;
    }
    assert!(
        centers(&d)
            .iter()
            .zip(&before)
            .all(|(a, b)| a.z > b.z + 0.5),
        "the actual side of the arch remains pushable"
    );
    Ok(())
}

#[test]
fn an_actor_crossing_between_frames_still_pushes_pressure_debris() -> Result<()> {
    let (b, _) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    d.set_limit(0);
    pressure_section(&mut d, &b, &[Vec3::new(0., 0.3, 0.)])?;
    run(&mut d, &b, 30)?;
    let before = centers(&d)[0];
    d.push(&[actor(Vec3::new(-2., 1.2, 0.))]);
    d.advance(0.1, &b)?;
    d.push(&[actor(Vec3::new(2., 1.2, 0.))]);
    d.advance(0.1, &b)?;
    assert!(
        centers(&d)[0].x > before.x + 0.1,
        "sweep the actor path, not just its endpoint"
    );
    assert_eq!(d.chunk_handles().count(), 0);
    Ok(())
}

#[test]
fn later_blasts_move_pressure_debris_without_allocating_more_bodies() -> Result<()> {
    let (b, _) = super::super::tests::building(&[]);
    let mut d = BrickDebris::new();
    d.set_limit(0);
    pressure_section(&mut d, &b, &[Vec3::new(0., 0.3, 0.)])?;
    run(&mut d, &b, 30)?;
    let before = centers(&d)[0];
    d.blast(before - Vec3::X * 2., 30., 5., None);
    run(&mut d, &b, 20)?;
    let right = centers(&d)[0];
    assert!(right.x > before.x + 0.25);
    d.blast(right + Vec3::X * 2., 30., 5., None);
    run(&mut d, &b, 20)?;
    assert!(
        centers(&d)[0].x < right.x - 0.25,
        "later hits stay interactive"
    );
    assert_eq!(d.chunk_handles().count(), 0);
    Ok(())
}
