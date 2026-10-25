//! An image's stance is laid over the holder while it is in their hand and
//! lifted by any change of tool.
use anyhow::Result;
use bri_package::packages::PackageSet;
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_weapons::Pack;
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

const MODEL: &str = "stance:body/native";
const WALKING: &str = "stance:archetype/walking";
const CROUCHED: &str = "stance:archetype/crouched";

fn game() -> Result<(tempfile::TempDir, Session)> {
    let root = tempfile::tempdir()?;
    let dir = root.path().join("stance");
    let rules = root.path().join("stancerules");
    std::fs::create_dir_all(&dir)?;
    std::fs::create_dir_all(&rules)?;
    let shape = json!({
        "schema_version":1,"id":MODEL,
        "nodes":[{"name":"root","parent":null,"translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
        "objects":[],"details":[],"meshes":[],"materials":[],"animations":[]
    });
    std::fs::write(dir.join("shape.json"), shape.to_string())?;
    let mut provides = vec![json!({"kind":"body","id":MODEL,"file":"shape.json"})];
    for name in ["walking", "crouched"] {
        std::fs::write(
            dir.join(format!("{name}.json")),
            json!({"schema_version":1,"name":name,"model":MODEL}).to_string(),
        )?;
        provides.push(
            json!({"kind":"archetype","id":format!("stance:archetype/{name}"),
            "file":format!("{name}.json")}),
        );
    }
    std::fs::write(
        dir.join("package.json"),
        json!({"schema_version":1,"id":"stance","version":"1.0.0","api":1,
            "name":"Invented stance","license":"CC0-1.0","provides":provides})
        .to_string(),
    )?;
    std::fs::write(
        rules.join("package.json"),
        json!({"schema_version":1,"id":"stancerules","version":"1.0.0","api":1,
            "name":"Invented stance controls","license":"CC0-1.0","capabilities":["player"],
            "dependencies":{"stance":"^1.0.0"},"provides":[
                {"kind":"behaviour","id":"stancerules:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"stancerules:script/main","file":"main.rhai"}]})
        .to_string(),
    )?;
    std::fs::write(
        rules.join("main.rhai"),
        r#"fn cmd_walk(p) { set_archetype(p, "stance:archetype/walking"); }
fn cmd_pull(p) { set_image_state(p, "Pulled"); }
fn cmd_tools(p) { set_tools(p, ["stance:weapon/grenade", "stance:weapon/rifle"]); }"#,
    )?;
    std::fs::write(
        rules.join("behaviour.json"),
        json!({"schema_version":1,"script":"main.rhai","commands":[{"name":"walk"},{"name":"tools"},{"name":"pull"}]})
            .to_string(),
    )?;
    let packages: PackageSet = serde_json::from_value(json!({"schema_version":1,"packages":[
        {"id":"stance","version":"1.0.0","side":"shared","dir":"stance"},
        {"id":"stancerules","version":"1.0.0","side":"server","dir":"stancerules"}]}))?;
    let mut session = Session::new(Simulation::new(
        World::new("Stance".into(), "test:stance".into(), vec![[1.; 4]]),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )?);
    session.install_packages(
        Arc::new(
            Catalog::load(root.path(), &packages, true).map_err(|e| anyhow::anyhow!("{e:#?}"))?,
        ),
        None,
    )?;
    session.set_weapon_pack(Pack::from_json(&serde_json::to_vec(&json!({
        "schema_version":5,"id":"stance",
        "items":{"stance:weapon/grenade":{"ui_name":"Invented grenade","image":"stance:image/grenade"},
            "stance:weapon/rifle":{"ui_name":"Invented rifle","image":"stance:image/rifle"}},
        "images":{
            "stance:image/grenade":{"model":MODEL,"states":[{"name":"Ready"},{"name":"Pulled","stance":{WALKING:WALKING}}],
                "stance":{WALKING:CROUCHED}},
            "stance:image/rifle":{"model":MODEL,"states":[{"name":"Ready"}]}
        },"projectiles":{}
    }))?)?)?;
    session.set_item_bounds(Default::default())?;
    Ok((root, session))
}

fn body(session: &Session, owner: u64) -> String {
    let state = session
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    session.archetypes().resolve(state.archetype).id.clone()
}

fn step(session: &mut Session, owner: u64) -> Result<()> {
    session.movement(
        owner,
        session.simulation().state().tick + 100,
        MoveInput::default(),
    )?;
    session.step()
}

#[test]
fn a_stance_lasts_exactly_as_long_as_its_image_is_in_hand() -> Result<()> {
    let (_root, mut session) = game()?;
    let owner = session.join("Holder".into(), Vec3::new(0., 0.05, 0.), true)?;
    let run = |session: &mut Session, name: &str, sequence| {
        session.command(
            owner,
            sequence,
            Command::Package(PackageCommand {
                package: "stancerules".into(),
                command: name.into(),
                args: vec![],
            }),
        )
    };
    run(&mut session, "walk", 1)?;
    run(&mut session, "tools", 2)?;
    step(&mut session, owner)?;
    assert_eq!(body(&session, owner), WALKING);

    session.equip_tool(owner, Some(0))?;
    step(&mut session, owner)?;
    assert_eq!(
        body(&session, owner),
        CROUCHED,
        "the grenade lays its stance"
    );
    let held = session.vitals().remove(&owner).unwrap();
    assert!(
        held.base_body.is_some(),
        "the body under the stance is replicated"
    );

    session.equip_tool(owner, Some(1))?;
    step(&mut session, owner)?;
    assert_eq!(body(&session, owner), WALKING, "any other tool lifts it");
    assert!(session.vitals()[&owner].base_body.is_none());

    session.equip_tool(owner, Some(0))?;
    step(&mut session, owner)?;
    session.equip_tool(owner, None)?;
    step(&mut session, owner)?;
    assert_eq!(
        body(&session, owner),
        WALKING,
        "putting tools away lifts it"
    );
    Ok(())
}

#[test]
fn a_state_may_lay_a_stance_of_its_own_in_place_of_the_images() -> Result<()> {
    let (_root, mut session) = game()?;
    let owner = session.join("Holder".into(), Vec3::new(0., 0.05, 0.), true)?;
    let run = |session: &mut Session, name: &str, sequence| {
        session.command(
            owner,
            sequence,
            Command::Package(PackageCommand {
                package: "stancerules".into(),
                command: name.into(),
                args: vec![],
            }),
        )
    };
    run(&mut session, "walk", 1)?;
    run(&mut session, "tools", 2)?;
    session.equip_tool(owner, Some(0))?;
    step(&mut session, owner)?;
    assert_eq!(body(&session, owner), CROUCHED, "ready: the image's own");
    run(&mut session, "pull", 3)?;
    step(&mut session, owner)?;
    step(&mut session, owner)?;
    assert_eq!(body(&session, owner), WALKING, "pulled: the state's own");
    Ok(())
}
