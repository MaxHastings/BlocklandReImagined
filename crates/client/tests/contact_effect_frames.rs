//! Invented native hit/particle fixtures. Strict ignored tests expose missing
//! transport; no recovered Halo sign, roll or attachment law is asserted.
use anyhow::{Context, Result, ensure};
use bri_content::effects::{Burst, ParticleOrientation};
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
            normal: Vec3::Z,
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
    let mut world = WeaponsWorld::new(Pack::from_json(GUN.as_bytes())?)?;
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
#[ignore = "strict transport requirement: native effect event drops actual incoming travel"]
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
#[ignore = "strict transport requirement: native reliable cue schema discards independent contact context"]
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
#[ignore = "strict authored reference requirement: Burst has no independent contact reference selector"]
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
