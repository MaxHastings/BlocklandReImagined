//! The actual sample hook can hear kills from players whose spawn it did not own.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Dynamic, PlayerKey,
    script::{Budget, Call, PlayerView, Runtime, Snapshot},
    state::Namespace,
};
use std::{path::PathBuf, sync::Arc};

#[test]
fn commando_death_hook_counts_an_uninitialized_killer_and_ignores_uncredited_deaths() {
    let set = PackageSet {
        schema_version: 1,
        packages: [
            ("sample-commando", Side::Server),
            ("sample-commando-rifle", Side::Shared),
            ("sample-commando-look", Side::Client),
        ]
        .into_iter()
        .map(|(id, side)| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        })
        .collect(),
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples");
    let catalog = Catalog::load(&root, &set, true).unwrap();
    let runtime = Runtime::compile(&catalog).unwrap();
    let snapshot = Arc::new(Snapshot {
        players: [1, 2]
            .into_iter()
            .map(|id| PlayerView {
                id,
                key: PlayerKey::session(id),
                alive: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let call = |state, killer| {
        runtime
            .call(
                "sample-commando",
                Call {
                    function: "on_death",
                    args: vec![2_i64.into(), killer],
                    budget: Budget::Command,
                    snapshot: snapshot.clone(),
                    caller: None,
                    aim: None,
                    entity: None,
                    state,
                    entity_vars: Default::default(),
                    world: None,
                },
            )
            .unwrap()
    };
    for killer in [Dynamic::UNIT, 2_i64.into()] {
        let ignored = call(Namespace::default(), killer);
        assert_eq!(ignored.state, Namespace::default());
        assert!(ignored.ops.is_empty());
    }
    let first = call(Namespace::default(), 1_i64.into());
    let key = PlayerKey::session(1);
    assert_eq!(first.state.players[&key]["streak"], 1);
    assert_eq!(first.state.players[&key]["score"], 1);
    let second = call(first.state, 1_i64.into());
    assert_eq!(second.state.players[&key]["streak"], 2);
    assert_eq!(second.state.players[&key]["score"], 2);
}
