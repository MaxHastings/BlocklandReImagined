//! Bind imported light/emitter names to native identities without changing source
//! records or dropping unknown community resources.
use anyhow::{Result, ensure};
use bri_content::effects::Library;
use bri_world::{Action, ContentRef, World};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default, Debug, Serialize)]
pub struct Report {
    pub resolved: usize,
    pub unresolved: BTreeMap<String, usize>,
}
pub fn bind(world: &mut World, library: &Library) -> Result<Report> {
    library.validate()?;
    let mut names = BTreeMap::new();
    for (prefix, id, name) in library
        .lights
        .iter()
        .map(|e| ("light", &e.id, &e.name))
        .chain(library.emitters.iter().map(|e| ("emitter", &e.id, &e.name)))
    {
        for (namespace, name) in [
            (
                format!("{prefix}_datablock"),
                id.rsplit('/').next().unwrap(),
            ),
            (format!("{prefix}_ui"), name.trim()),
        ] {
            if !name.is_empty() {
                ensure!(
                    names
                        .insert((namespace, name.to_lowercase()), id.clone())
                        .is_none(),
                    "Ambiguous native effect name {name}"
                );
            }
        }
    }
    let mut report = Report::default();
    let mut resolve = |reference: &mut ContentRef| {
        if let ContentRef::Unresolved { namespace, name } = reference {
            if let Some(id) = names.get(&(namespace.clone(), name.trim().to_lowercase())) {
                *reference = ContentRef::Resolved(id.clone());
                report.resolved += 1;
            } else {
                *report
                    .unresolved
                    .entry(format!("{namespace}:{name}"))
                    .or_default() += 1;
            }
        }
    };
    let action = |a: &mut Action, resolve: &mut dyn FnMut(&mut ContentRef)| match a {
        Action::Light(Some(r)) | Action::Emitter(Some(r)) => resolve(r),
        _ => {}
    };
    for brick in world.bricks.values_mut() {
        if let Some(light) = &mut brick.light {
            resolve(&mut light.asset);
        }
        if let Some(emitter) = &mut brick.emitter
            && let Some(asset) = &mut emitter.asset
        {
            resolve(asset);
        }
        for e in &mut brick.events {
            action(&mut e.action, &mut resolve);
        }
    }
    for event in &mut world.pending {
        action(&mut event.action, &mut resolve);
    }
    world.validate()?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::effects::Light;
    use bri_world::{Brick, Event, Input, SourceRecord, Target};
    #[test]
    fn binds_names_and_event_datablocks_but_preserves_unknowns_and_original_records() {
        let library = Library {
            schema_version: 1,
            lights: vec![Light {
                id: "v20/light/redlight".into(),
                name: "Red Light".into(),
                enabled: true,
                color: [1.0, 0.0, 0.0],
                brightness: 9.0,
                radius: 10.0,
                color_curves: None,
                brightness_curve: None,
                radius_curve: None,
                flare: None,
            }],
            particles: vec![],
            emitters: vec![],
            textures: BTreeMap::new(),
        };
        let mut world = World::new("test".into(), "map".into(), vec![[1.0; 4]]);
        let unresolved = |ns: &str, n: &str| ContentRef::Unresolved {
            namespace: ns.into(),
            name: n.into(),
        };
        let mut brick = Brick::new(ContentRef::Resolved("brick".into()), [0.0; 3], 0);
        brick.light = Some(bri_world::Light {
            asset: unresolved("light_ui", "RED LIGHT"),
            enabled: false,
        });
        brick.events.push(Event {
            enabled: true,
            input: Input::Activate,
            delay_ms: 10,
            target: Target::ThisBrick,
            action: Action::Light(Some(unresolved("light_datablock", "RedLight"))),
        });
        brick.emitter = Some(bri_world::Emitter {
            asset: Some(unresolved("emitter_ui", "Community Effect")),
            direction: 5,
        });
        brick.source_records.push(SourceRecord {
            line: 1,
            text: "original".into(),
            diagnostic: None,
        });
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let original = world.bricks[&1].source_records.clone();
        let report = bind(&mut world, &library).unwrap();
        assert_eq!(report.resolved, 2);
        assert_eq!(report.unresolved["emitter_ui:Community Effect"], 1);
        assert_eq!(world.bricks[&1].source_records, original);
        assert!(!world.bricks[&1].light.as_ref().unwrap().enabled);
        assert_eq!(bind(&mut world, &library).unwrap().resolved, 0);
        assert_eq!(
            bri_world::persistence::decode(&serde_json::to_vec(&world).unwrap()).unwrap(),
            world
        );
    }
}
