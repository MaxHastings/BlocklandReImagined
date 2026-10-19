//! A blast has no contact to orient a particle group by: the engine drops a group framed on one without a word, and the
//! explosion then draws nothing. The Add-On health check names such a group (found by looking at an AA grenade render).
use anyhow::{Context, Result, ensure};
use bri_content::effects::{Burst, BurstFrame};
use bri_fx_runtime::{EffectsLimits, pack::Composite};
use bri_weapons::Pack;

fn pack(explode_death: bool) -> Result<Pack> {
    Pack::from_json(
        format!(
            r#"{{
  "schema_version":5,"id":"fixture-blast",
  "items":{{}},"images":{{}},
  "projectiles":{{"fixture:projectile/grenade":{{"speed":10,"lifetime_ticks":60,"explode_death":{explode_death},
    "explosion":{{"effect":"fixture:effect/blast","radius":3.0}}}}}}
}}"#
        )
        .as_bytes(),
    )
}

fn effects(frame: BurstFrame) -> Result<bri_client::weapon_effects::WeaponEffects> {
    let library = bri_fx_runtime::testing::library();
    let mut manifest = bri_fx_runtime::testing::manifest();
    let group = |frame| Burst {
        emitter: "emitter".into(),
        frame,
        ..Default::default()
    };
    manifest.composites.push(Composite {
        id: "fixture:effect/blast".into(),
        lifetime: 1.,
        emitters: vec![],
        light: None,
        burst: None,
        bursts: vec![group(BurstFrame::Source), group(frame)],
    });
    let fx = bri_fx_runtime::testing::pack_from(library, manifest);
    bri_client::weapon_effects::WeaponEffects::new(
        fx,
        std::sync::Arc::new(pack(true)?),
        EffectsLimits::default(),
    )
}

#[test]
fn a_group_framed_on_a_contact_is_named_when_the_projectile_can_go_off_without_one() -> Result<()> {
    let weapons = pack(true)?;
    for frame in [
        BurstFrame::ContactNormal,
        BurstFrame::ContactTravel,
        BurstFrame::OppositeTravel,
        BurstFrame::ReflectedTravel,
    ] {
        let found = bri_client::add_on_health::contact_problems(&weapons, &effects(frame)?);
        let problem = found
            .first()
            .with_context(|| format!("{frame:?} was not named"))?;
        ensure!(found.len() == 1, "only the framed group: {found:?}");
        let text = problem.to_string();
        ensure!(
            text.contains("group 1")
                && text.contains("emitter")
                && text.contains("fixture:effect/blast")
                && text.contains("fixture:projectile/grenade"),
            "names the group, its emitter, the effect and the projectile: {text}"
        );
    }
    // Negative controls: a source frame needs no contact, and a projectile that only ever goes off on something (no fuse, no
    // death explosion) is given one.
    ensure!(
        bri_client::add_on_health::contact_problems(&weapons, &effects(BurstFrame::Source)?)
            .is_empty()
    );
    ensure!(
        bri_client::add_on_health::contact_problems(
            &pack(false)?,
            &effects(BurstFrame::ContactNormal)?
        )
        .is_empty()
    );
    Ok(())
}
