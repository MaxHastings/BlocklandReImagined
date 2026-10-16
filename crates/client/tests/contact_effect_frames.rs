//! Invented native hit/particle fixtures for accepted contact transport.
//! No recovered Halo sign, roll or attachment law is asserted.
use anyhow::{Context, Result, ensure};
use bri_content::effects::{Burst, BurstFrame, ContactGeometry, ParticleOrientation};
use bri_fx_runtime::{Camera, EffectsLimits, SourceOptions, SourceTransform, pack::Composite};
use bri_weapons::{
    ActorId, ContactResponse, Event, Filter, Frame, Hit, Nearby, Pack, ProjectileContact, Query,
    TargetId, WeaponsWorld,
};
use glam::{Mat4, Quat, Vec3};

const GUN: &str = r#"{
  "schema_version":5,"id":"fixture-contact",
  "items":{"fixture:weapon/rifle":{"ui_name":"Fixture","image":"fixture:image/rifle"}},
  "images":{"fixture:image/rifle":{"projectile":"fixture:projectile/ray",
    "shot":{"projectiles":1,"spread":0.08,"hitscan":{"range":100}},
    "states":[{"name":"Activate","ticks":2,"timeout":1},{"name":"Ready","down":2},
      {"name":"Fire","ticks":4,"script":"onFire","timeout":3},{"name":"Wait","up":1}]}},
  "projectiles":{"fixture:projectile/ray":{"speed":100,"lifetime_ticks":120,
    "explosion":{"effect":"fixture:effect/contact"}}}
}"#;

#[derive(Default)]
struct Wall {
    cast: Option<Vec3>,
    contact: Option<ProjectileContact>,
    normal_override: Option<Vec3>,
}
impl Query for Wall {
    fn sweep(&mut self, from: Vec3, to: Vec3, _: Filter) -> Option<Hit> {
        let delta = to - from;
        if from.z <= -10. || to.z >= -10. {
            return None;
        }
        let fraction = (-10. - from.z) / delta.z;
        self.cast = Some(delta.normalize());
        Some(Hit {
            target: TargetId::Map(9),
            position: from + delta * fraction,
            normal: self.normal_override.unwrap_or(Vec3::Z),
            fraction,
            color: None,
        })
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        vec![]
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        false
    }
    fn on_contact(&mut self, contact: &ProjectileContact) -> ContactResponse {
        self.contact = Some(contact.clone());
        ContactResponse::Continue
    }
}
fn hit() -> Result<(ProjectileContact, Event, Vec3, Vec3)> {
    hit_pack(Pack::from_json(GUN.as_bytes())?)
}
fn hit_pack(pack: Pack) -> Result<(ProjectileContact, Event, Vec3, Vec3)> {
    hit_pack_with_wall(pack, Wall::default())
}
fn hit_pack_with_wall(
    pack: Pack,
    mut wall: Wall,
) -> Result<(ProjectileContact, Event, Vec3, Vec3)> {
    let mut world = WeaponsWorld::new(pack)?;
    let owner = ActorId(17);
    world.add_actor(owner, 5)?;
    let nominal = Vec3::new(0.35, 0.1, -1.).normalize();
    world.set_frame(
        owner,
        Frame {
            direction: nominal,
            ..Default::default()
        },
    )?;
    let slot = world.give(owner, "fixture:weapon/rifle")?;
    world.equip(owner, Some(slot))?;
    for _ in 0..5 {
        world.step(&mut wall);
    }
    world.trigger(owner, true)?;
    let mut events = Vec::new();
    for _ in 0..2 {
        events.extend(world.step(&mut wall));
    }
    world.trigger(owner, false)?;
    let contact = wall.contact.context("native accepted contact")?;
    let effect = events
        .into_iter()
        .find(|e| matches!(e,Event::Effect {definition,..} if definition=="fixture:effect/contact"))
        .context("native effect event")?;
    Ok((
        contact,
        effect,
        wall.cast.context("actual randomized cast")?,
        nominal,
    ))
}

#[test]
fn native_hit_has_independent_actual_randomized_travel_normal_and_attribution() -> Result<()> {
    let (contact, effect, cast, nominal) = hit()?;
    let travel = contact.velocity.normalize();
    ensure!(travel.distance(cast) < 1e-6);
    ensure!(
        travel.distance(nominal) > 1e-4,
        "fixture must exercise real spread"
    );
    ensure!(
        travel.cross(contact.normal).length() > 0.2,
        "non-collinear counterexample required"
    );
    ensure!(contact.target == TargetId::Map(9) && contact.source == ActorId(17));
    let Event::Effect {
        source,
        position,
        direction,
        ..
    } = effect
    else {
        unreachable!()
    };
    ensure!(
        source == TargetId::Actor(ActorId(17)),
        "source remains shooter attribution"
    );
    ensure!(position.distance(contact.position) < 1e-6 && direction == Some(contact.normal));
    Ok(())
}

#[test]
fn legacy_burst_local_axes_and_plane_orientation_stay_in_the_source_frame() -> Result<()> {
    let mut library = bri_fx_runtime::testing::library();
    library.lights.clear();
    library.emitters[0].speed = 2.;
    library.particles[0].spin_degrees = 0.;
    library.particles[0].orientation = Some(ParticleOrientation::Plane);
    let mut manifest = bri_fx_runtime::testing::manifest();
    manifest.composites.push(Composite {
        id: "fixture:effect/groups".into(),
        lifetime: 1.,
        emitters: vec![],
        light: None,
        burst: None,
        bursts: vec![
            Burst {
                emitter: "emitter".into(),
                direction: Vec3::Y.to_array(),
                ..Default::default()
            },
            Burst {
                emitter: "emitter".into(),
                direction: Vec3::X.to_array(),
                ..Default::default()
            },
        ],
    });
    let pack = bri_fx_runtime::testing::pack_from(library, manifest);
    let mut world = bri_fx_runtime::EffectsWorld::new(pack, EffectsLimits::default(), 41)?;
    let rotation = Quat::from_rotation_arc(Vec3::Y, Vec3::Z);
    world.play_composite(
        "fixture:effect/groups",
        SourceTransform {
            rotation,
            ..Default::default()
        },
        SourceOptions::default(),
    )?;
    world.advance(0.1, Vec3::ZERO)?;
    let camera = Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::Z * 5.,
        right: Vec3::X,
        up: Vec3::Y,
    };
    let frame = world.snapshot(&camera);
    ensure!(frame.particles.len() == 2);
    for expected in [rotation * Vec3::Y, rotation * Vec3::X] {
        let particle = frame
            .particles
            .iter()
            .find(|p| p.position.normalize().distance(expected) < 1e-5)
            .context("authored local direction")?;
        ensure!(particle.position.distance(expected * 0.2) < 1e-5);
        ensure!(particle.axis.distance(expected) < 1e-5);
        ensure!(
            particle.plane_right.length() > 0.9 && particle.plane_right.dot(expected).abs() < 1e-5
        );
    }
    // Existing authored groups default to source/socket/legacy contact frame.
    let burst: Burst = serde_json::from_value(serde_json::json!({"emitter":"emitter"}))?;
    ensure!(burst.direction == [0., 1., 0.] && !burst.follow_source);
    Ok(())
}

#[test]
fn accepted_effect_retains_independent_contact_vectors() -> Result<()> {
    let (contact, effect, _, _) = hit()?;
    // Proposed bounded context shape for review; root may rename the fields.
    // The required behavior is retaining these two actual quantities separately.
    let value = serde_json::to_value(effect)?;
    let frame = &value["Effect"]["contact"];
    ensure!(
        frame["travel"] == serde_json::json!(contact.velocity.normalize().to_array()),
        "effect lost actual travel: {value}"
    );
    ensure!(frame["normal"] == serde_json::json!(contact.normal.to_array()));
    Ok(())
}

#[test]
fn native_cue_roundtrip_retains_contact_context_separately_from_legacy_direction() -> Result<()> {
    let (contact, _, _, _) = hit()?;
    let expected = serde_json::json!({"travel":contact.velocity.normalize().to_array(),"normal":contact.normal.to_array()});
    let input = serde_json::json!({"id":1,"tick":1,"position":contact.position.to_array(),"kind":{"WeaponEffect":{
        "source":{"Actor":17},"definition":"fixture:effect/contact","node":"","seconds":0.,"image":null,
        "hand":null,"direction":contact.normal.to_array(),"scale":1.,"contact":expected}}});
    // Actual native Cue serde, not an invented wire/Session adapter.
    let cue: bri_sim::presentation::Cue = serde_json::from_value(input)?;
    cue.validate()?;
    let roundtrip = serde_json::to_value(cue)?;
    ensure!(
        roundtrip["kind"]["WeaponEffect"]["contact"] == expected,
        "cue dropped context: {roundtrip}"
    );
    ensure!(
        roundtrip["kind"]["WeaponEffect"]["direction"]
            == serde_json::json!(contact.normal.to_array())
    );
    Ok(())
}

#[test]
fn native_client_groups_can_select_normal_and_actual_travel_independently() -> Result<()> {
    let (contact, _, _, _) = hit()?;
    let travel = contact.velocity.normalize();
    let mut library = bri_fx_runtime::testing::library();
    library.lights.clear();
    library.emitters[0].speed = 2.;
    library.particles[0].spin_degrees = 0.;
    let mut manifest = bri_fx_runtime::testing::manifest();
    // Reviewable proposed authored field names; desired output is generic native
    // geometry. These do not bind any original group's names, sign or roll.
    let groups: Vec<Burst> = serde_json::from_value(serde_json::json!([
        {"emitter":"emitter","frame":"contact_normal"},
        {"emitter":"emitter","frame":"contact_travel"}
    ]))
    .context("independent authored Burst references are unsupported")?;
    manifest.composites.push(Composite {
        id: "fixture:effect/contact".into(),
        lifetime: 1.,
        emitters: vec![],
        light: None,
        burst: None,
        bursts: groups,
    });
    let effects = bri_fx_runtime::testing::pack_from(library, manifest);
    let weapons = std::sync::Arc::new(Pack::from_json(GUN.as_bytes())?);
    let mut client =
        bri_client::weapon_effects::WeaponEffects::new(effects, weapons, EffectsLimits::default())?;
    let cue: bri_sim::presentation::Cue = serde_json::from_value(serde_json::json!({
        "id":1,"tick":1,"position":contact.position.to_array(),"kind":{"WeaponEffect":{
            "source":{"Actor":17},"definition":"fixture:effect/contact","node":"","seconds":0.,"image":null,
            "hand":null,"direction":contact.normal.to_array(),"scale":1.,
            "contact":{"travel":travel.to_array(),"normal":contact.normal.to_array()}}}
    }))?;
    client.cues(&[cue], |_| None)?;
    client.advance(0.1, Vec3::ZERO, |_| None)?;
    let frame = client.world().snapshot(&Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::Z * 5.,
        right: Vec3::X,
        up: Vec3::Y,
    });
    ensure!(frame.particles.len() == 2);
    for axis in [contact.normal, travel] {
        let expected = contact.position + axis * 0.2;
        ensure!(
            frame
                .particles
                .iter()
                .any(|p| p.position.distance(expected) < 1e-5),
            "missing independent group axis {axis:?}"
        );
    }
    Ok(())
}

fn group(frame: BurstFrame) -> Burst {
    Burst {
        emitter: "emitter".into(),
        frame,
        ..Default::default()
    }
}
fn geometry() -> ContactGeometry {
    ContactGeometry {
        travel: [0.6, 0., -0.8],
        normal: Vec3::Z.to_array(),
    }
}
fn effect_pack(groups: Vec<Burst>, headers: bool) -> std::sync::Arc<bri_fx_runtime::EffectsPack> {
    let mut library = bri_fx_runtime::testing::library();
    library.emitters[0].speed = 2.;
    library.particles[0].spin_degrees = 0.;
    let mut manifest = bri_fx_runtime::testing::manifest();
    manifest.composites.push(Composite {
        id: "fixture:effect/contact".into(),
        lifetime: 1.,
        emitters: if headers {
            vec!["emitter".into()]
        } else {
            vec![]
        },
        light: headers.then(|| "light".into()),
        burst: None,
        bursts: groups,
    });
    bri_fx_runtime::testing::pack_from(library, manifest)
}
fn client(
    groups: Vec<Burst>,
    limits: EffectsLimits,
) -> Result<bri_client::weapon_effects::WeaponEffects> {
    bri_client::weapon_effects::WeaponEffects::new(
        effect_pack(groups, false),
        std::sync::Arc::new(Pack::from_json(GUN.as_bytes())?),
        limits,
    )
}
fn effect_cue(id: u64, contact: Option<ContactGeometry>) -> bri_sim::presentation::Cue {
    serde_json::from_value(serde_json::json!({"id":id,"tick":1,"position":[1.,2.,3.],"kind":{"WeaponEffect":{
        "source":{"Actor":17},"definition":"fixture:effect/contact","node":"","seconds":0.,"image":null,
        "hand":null,"direction":[0.,0.,1.],"scale":1.,"contact":contact}}})).unwrap()
}
fn camera() -> Camera {
    Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::Z * 5.,
        right: Vec3::X,
        up: Vec3::Y,
    }
}

#[test]
fn missing_context_omits_only_explicit_groups_once_and_budgets_only_eligible_groups() -> Result<()>
{
    let mut unavailable = group(BurstFrame::ContactTravel);
    unavailable.count = [32000; 2];
    let mut fx = client(
        vec![group(BurstFrame::Source), unavailable],
        EffectsLimits {
            sources: 1,
            emissions_per_advance: 1,
            ..Default::default()
        },
    )?;
    let cue = effect_cue(1, None);
    fx.cues(std::slice::from_ref(&cue), |_| None)?;
    ensure!(
        fx.world().particle_count() == 1 && fx.world().diagnostics().missing_contact_groups == 1
    );
    fx.cues(&[cue], |_| None)?;
    ensure!(
        fx.world().particle_count() == 1 && fx.world().diagnostics().missing_contact_groups == 1
    );
    ensure!(fx.diagnostics.capacity_rejections == 0 && fx.diagnostics.duplicate_cues == 1);
    Ok(())
}

#[test]
fn invalid_geometry_or_late_pose_rejects_the_whole_batch_before_cursor_or_emission() -> Result<()> {
    let mut fx = client(vec![group(BurstFrame::Source)], EffectsLimits::default())?;
    let first = effect_cue(1, Some(geometry()));
    for invalid in [
        ContactGeometry {
            travel: [0.; 3],
            ..geometry()
        },
        ContactGeometry {
            normal: [0.; 3],
            ..geometry()
        },
        ContactGeometry {
            travel: [f32::NAN, 0., 1.],
            ..geometry()
        },
        ContactGeometry {
            normal: [0., 2., 0.],
            ..geometry()
        },
    ] {
        let mut bad = effect_cue(2, None);
        if let bri_sim::presentation::CueKind::WeaponEffect { contact, .. } = &mut bad.kind {
            *contact = Some(invalid);
        }
        ensure!(fx.cues(&[first.clone(), bad], |_| None).is_err());
        ensure!(fx.world().particle_count() == 0 && fx.diagnostics.accepted_cues == 0);
    }
    let mut second = effect_cue(2, Some(geometry()));
    if let bri_sim::presentation::CueKind::WeaponEffect { node, seconds, .. } = &mut second.kind {
        *node = "socket".into();
        *seconds = 0.1;
    }
    let invalid = SourceTransform {
        rotation: Quat::from_xyzw(0., 0., 0., 2.),
        ..Default::default()
    };
    ensure!(
        fx.cues(&[first.clone(), second.clone()], |_| Some(invalid))
            .is_err()
    );
    ensure!(fx.world().particle_count() == 0 && fx.diagnostics.accepted_cues == 0);
    fx.cues(&[first, second], |_| Some(SourceTransform::default()))?;
    ensure!(
        fx.world().particle_count() == 2 && fx.diagnostics.accepted_cues == 2,
        "rejected batch must not consume its IDs"
    );
    Ok(())
}

#[test]
fn all_native_references_compose_local_offset_and_axis_once_while_source_is_unchanged() -> Result<()>
{
    let d = Vec3::from(geometry().travel);
    let n = Vec3::Z;
    for (reference, axis) in [
        (BurstFrame::ContactNormal, n),
        (BurstFrame::ContactTravel, d),
        (BurstFrame::OppositeTravel, -d),
        (BurstFrame::ReflectedTravel, d - 2. * d.dot(n) * n),
        (BurstFrame::WorldDown, Vec3::NEG_Y),
    ] {
        let mut b = group(reference);
        b.offset = [0.3, 0.2, -0.1];
        b.direction = Vec3::X.to_array();
        let mut fx = client(vec![b], EffectsLimits::default())?;
        fx.cues(&[effect_cue(1, Some(geometry()))], |_| None)?;
        fx.advance(0.1, Vec3::ZERO, |_| None)?;
        let frame = fx.world().snapshot(&camera());
        let basis = Quat::from_rotation_arc(Vec3::Y, axis.normalize());
        let expected =
            Vec3::new(1., 2., 3.) + basis * Vec3::new(0.3, 0.2, -0.1) + basis * Vec3::X * 0.2;
        ensure!(
            frame.particles.len() == 1 && frame.particles[0].position.distance(expected) < 1e-5,
            "{reference:?}"
        );
    }
    let mut a = client(vec![group(BurstFrame::Source)], EffectsLimits::default())?;
    let mut b = client(vec![group(BurstFrame::Source)], EffectsLimits::default())?;
    a.cues(&[effect_cue(1, None)], |_| None)?;
    b.cues(&[effect_cue(1, Some(geometry()))], |_| None)?;
    a.advance(0.1, Vec3::ZERO, |_| None)?;
    b.advance(0.1, Vec3::ZERO, |_| None)?;
    let a = a.world().snapshot(&camera());
    let b = b.world().snapshot(&camera());
    ensure!(
        a.particles[0].position == b.particles[0].position
            && a.particles[0].axis == b.particles[0].axis
            && a.particles[0].plane_right == b.particles[0].plane_right
            && a.particles[0].color == b.particles[0].color
    );
    Ok(())
}

#[test]
fn capacity_rejection_has_no_partial_sources_or_particles_and_consumes_cue_once() -> Result<()> {
    for limits in [
        EffectsLimits {
            sources: 1,
            ..Default::default()
        },
        EffectsLimits {
            emissions_per_advance: 1,
            ..Default::default()
        },
    ] {
        let mut fx = client(
            vec![group(BurstFrame::Source), group(BurstFrame::ContactTravel)],
            limits,
        )?;
        let cue = effect_cue(1, Some(geometry()));
        fx.cues(std::slice::from_ref(&cue), |_| None)?;
        ensure!(
            fx.world().source_count() == 0
                && fx.world().particle_count() == 0
                && fx.diagnostics.capacity_rejections == 1
        );
        fx.cues(&[cue], |_| None)?;
        ensure!(fx.diagnostics.capacity_rejections == 1 && fx.diagnostics.duplicate_cues == 1);
    }
    let pack = effect_pack(vec![group(BurstFrame::Source)], true);
    let mut world = bri_fx_runtime::EffectsWorld::new(
        pack,
        EffectsLimits {
            sources: 2,
            ..Default::default()
        },
        7,
    )?;
    let plan = world.prepare_composite(
        "fixture:effect/contact",
        SourceTransform::default(),
        SourceOptions::default(),
        None,
    )?;
    ensure!(world.play_prepared_composite(plan).is_err());
    ensure!(
        world.source_count() == 0
            && world.particle_count() == 0
            && world.snapshot(&camera()).lights.is_empty()
    );
    let pack = effect_pack(vec![group(BurstFrame::Source)], true);
    let mut world = bri_fx_runtime::EffectsWorld::new(
        pack,
        EffectsLimits {
            lights: 1,
            ..Default::default()
        },
        7,
    )?;
    world.start_light(
        "light",
        SourceTransform::default(),
        SourceOptions::default(),
    )?;
    let plan = world.prepare_composite(
        "fixture:effect/contact",
        SourceTransform::default(),
        SourceOptions::default(),
        None,
    )?;
    ensure!(world.play_prepared_composite(plan).is_err());
    ensure!(world.source_count() == 1 && world.particle_count() == 0);
    // Keep conservative count-max admission even when a random draw could be lower.
    let mut variable = group(BurstFrame::Source);
    variable.count = [1, 2];
    let mut fx = client(
        vec![variable],
        EffectsLimits {
            emissions_per_advance: 1,
            ..Default::default()
        },
    )?;
    fx.cues(&[effect_cue(1, None)], |_| None)?;
    ensure!(fx.world().particle_count() == 0 && fx.diagnostics.capacity_rejections == 1);
    Ok(())
}

#[test]
fn unavailable_contact_attachment_is_explicit_and_never_borrows_shooter_pose() -> Result<()> {
    let mut attached = group(BurstFrame::ContactTravel);
    attached.follow_source = true;
    let mut fx = client(
        vec![group(BurstFrame::Source), attached],
        EffectsLimits::default(),
    )?;
    fx.cues(&[effect_cue(1, Some(geometry()))], |_| {
        panic!("world contact must not look up shooter attachment")
    })?;
    ensure!(
        fx.world().particle_count() == 1
            && fx.world().diagnostics().unsupported_contact_attachments == 1
    );
    Ok(())
}

#[test]
fn authored_bad_group_is_rejected_before_it_can_emit() -> Result<()> {
    let mut library = bri_fx_runtime::testing::library();
    library.lights.clear();
    let mut manifest = bri_fx_runtime::testing::manifest();
    let mut bad = group(BurstFrame::ContactTravel);
    bad.direction = [0.; 3];
    manifest.composites.push(Composite {
        id: "bad".into(),
        lifetime: 1.,
        emitters: vec![],
        light: None,
        burst: None,
        bursts: vec![group(BurstFrame::Source), bad],
    });
    ensure!(
        bri_fx_runtime::EffectsPack::from_parts(
            library,
            manifest,
            vec![bri_fx_runtime::pack::TextureImage {
                id: bri_fx_runtime::testing::TEXTURE.into(),
                width: 1,
                height: 1,
                rgba: vec![255; 4]
            }]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn hitscan_explosion_override_keeps_the_incoming_ray_instead_of_replacement_normal() -> Result<()> {
    let mut pack = Pack::from_json(GUN.as_bytes())?;
    let mut blast = pack.projectiles["fixture:projectile/ray"].clone();
    blast.id = "fixture:projectile/blast".into();
    pack.projectiles.insert(blast.id.clone(), blast);
    pack.images
        .get_mut("fixture:image/rifle")
        .unwrap()
        .shot
        .as_mut()
        .unwrap()
        .hitscan
        .as_mut()
        .unwrap()
        .explosion = "fixture:projectile/blast".into();
    let (contact, effect, cast, _) = hit_pack(pack)?;
    let Event::Effect {
        contact: Some(geometry),
        direction,
        ..
    } = effect
    else {
        anyhow::bail!("missing geometry")
    };
    ensure!(Vec3::from(geometry.travel).distance(cast) < 1e-6);
    ensure!(
        direction == Some(contact.normal)
            && Vec3::from(geometry.travel).distance(contact.normal) > 0.2
    );
    Ok(())
}

#[test]
fn flying_bounce_stick_and_expiry_keep_pre_response_or_absent_geometry() -> Result<()> {
    for mode in ["bounce", "stick", "expiry"] {
        let mut pack = Pack::from_json(GUN.as_bytes())?;
        pack.images
            .get_mut("fixture:image/rifle")
            .unwrap()
            .shot
            .as_mut()
            .unwrap()
            .hitscan = None;
        let p = pack.projectiles.get_mut("fixture:projectile/ray").unwrap();
        p.ballistic = true;
        p.elasticity = 1.;
        p.friction = 0.;
        p.bounce_effect = "fixture:bounce".into();
        p.stick_effect = "fixture:stick".into();
        if mode != "expiry" {
            p.arm_ticks = 120;
        }
        if mode == "stick" {
            p.min_stick_speed = 1.;
            p.bounce_angle = 360.;
        }
        if mode == "expiry" {
            p.lifetime_ticks = 2;
            p.fade_ticks = 2;
            p.explode_death = true;
        }
        let mut world = WeaponsWorld::new(pack)?;
        let owner = ActorId(17);
        world.add_actor(owner, 5)?;
        world.set_frame(
            owner,
            Frame {
                direction: Vec3::new(0.2, 0., -1.).normalize(),
                ..Default::default()
            },
        )?;
        let slot = world.give(owner, "fixture:weapon/rifle")?;
        world.equip(owner, Some(slot))?;
        let mut wall = Wall::default();
        for _ in 0..5 {
            world.step(&mut wall);
        }
        world.trigger(owner, true)?;
        let mut events = Vec::new();
        for _ in 0..2 {
            events.extend(world.step(&mut wall));
        }
        world.trigger(owner, false)?;
        for _ in 0..90 {
            events.extend(world.step(&mut wall));
        }
        let effect=events.iter().find(|e|matches!(e,Event::Effect {definition,..} if definition==&format!("fixture:{mode}") || mode=="expiry" && definition=="fixture:effect/contact")).with_context(|| format!("{mode} effect; accepted {:?}; events {events:?}", wall.contact))?;
        let Event::Effect {
            contact, direction, ..
        } = effect
        else {
            unreachable!()
        };
        if mode == "expiry" {
            ensure!(contact.is_none() && direction.is_none() && wall.contact.is_none());
        } else {
            let actual = wall.contact.context("accepted projectile contact")?;
            let geometry = contact.context("pre-response geometry")?;
            ensure!(
                Vec3::from(geometry.travel).distance(actual.velocity.normalize()) < 1e-6
                    && geometry.normal == actual.normal.to_array()
            );
            if mode == "bounce" {
                ensure!(
                    events
                        .iter()
                        .any(|e| matches!(e,Event::Bounced {velocity,..} if velocity.z>0.))
                );
            } else {
                ensure!(
                    world
                        .projectiles()
                        .any(|p| p.stuck && p.velocity == Vec3::ZERO)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn ordinary_session_cue_carries_actual_contact_without_changing_attribution() -> Result<()> {
    use bri_sim::{
        definitions::{Definitions, Special},
        player::MoveInput,
        session::{Command, Session},
        simulation::Simulation,
    };
    use bri_world::{Brick, ContentRef, World};
    use rapier3d::prelude::*;
    let defs = Definitions {
        entries: [(
            "wall".into(),
            bri_sim::testing::definition("wall", [64, 1], 40, Special::None, false),
        )]
        .into(),
    };
    let mut world = World::new(
        "contact fixture".into(),
        "fixture:map".into(),
        vec![[1.; 4]],
    );
    world.bricks.insert(
        1,
        Brick::new(ContentRef::Resolved("wall".into()), [0., 4., -4.25], 1),
    );
    world.next_brick_id = 2;
    let floor = ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.));
    let mut session = Session::new(Simulation::new(world, defs, vec![floor])?);
    let mut pack = Pack::from_json(GUN.as_bytes())?;
    pack.images
        .get_mut("fixture:image/rifle")
        .unwrap()
        .shot
        .as_mut()
        .unwrap()
        .spread = 0.0005;
    session.set_weapon_pack(pack)?;
    let owner = session.join("fixture".into(), Vec3::new(0., 0.05, 0.), true)?;
    let slot = session.give_item(owner, "fixture:weapon/rifle")?;
    session.equip_tool(owner, Some(slot))?;
    let step = |s: &mut Session| -> Result<()> {
        s.movement(
            owner,
            s.simulation().state().tick + 1,
            MoveInput {
                yaw: 0.35,
                ..Default::default()
            },
        )?;
        s.step()?;
        Ok(())
    };
    for _ in 0..20 {
        step(&mut session)?;
    }
    session.take_cues();
    session.command(owner, 1, Command::WeaponTrigger { down: true })?;
    let mut cues = Vec::new();
    for _ in 0..3 {
        step(&mut session)?;
        cues.extend(session.take_cues());
    }
    session.release_trigger(owner)?;
    let cue=cues.iter().find(|c|matches!(&c.kind,bri_sim::presentation::CueKind::WeaponEffect {definition,..} if definition=="fixture:effect/contact")).context("ordinary Session contact cue")?;
    let bri_sim::presentation::CueKind::WeaponEffect {
        contact: Some(geometry),
        source,
        direction,
        ..
    } = &cue.kind
    else {
        anyhow::bail!("lost Session geometry")
    };
    ensure!(*source == TargetId::Actor(ActorId(owner)) && direction == &Some(geometry.normal));
    ensure!(
        geometry.valid()
            && Vec3::from(geometry.travel)
                .cross(Vec3::from(geometry.normal))
                .length()
                > 0.2
    );
    ensure!(
        geometry.normal == Vec3::Z.to_array(),
        "fixture must meet wall, not floor"
    );
    Ok(())
}

#[test]
fn absent_real_normal_does_not_promote_legacy_facing_fallback_to_contact_geometry() -> Result<()> {
    let (_, effect, _, _) = hit_pack_with_wall(
        Pack::from_json(GUN.as_bytes())?,
        Wall {
            normal_override: Some(Vec3::ZERO),
            ..Default::default()
        },
    )?;
    let Event::Effect {
        contact, direction, ..
    } = effect
    else {
        unreachable!()
    };
    ensure!(
        contact.is_none(),
        "legacy ray-facing fallback is not an observed normal"
    );
    ensure!(
        direction.is_some(),
        "existing legacy facing behavior remains available"
    );
    Ok(())
}
