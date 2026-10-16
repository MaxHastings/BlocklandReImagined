use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{
    Catalog, Dynamic,
    ops::{self, Op},
    script::{Budget, Call, Runtime, Snapshot},
    state::Namespace,
};
use serde_json::json;
use std::sync::Arc;

fn catalog(script: &str, observer: bool) -> (tempfile::TempDir, Catalog) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("package.json"),json!({
        "schema_version":1,"id":"animationtest","version":"1.0.0","api":1,
        "name":"Animation API test","license":"CC0-1.0","capabilities":["player"],
        "provides":[{"kind":"behaviour","id":"animationtest:behaviour/main","file":"behaviour.json"},
            {"kind":"script","id":"animationtest:script/main","file":"main.rhai"}]
    }).to_string()).unwrap();
    std::fs::write(
        root.path().join("behaviour.json"),
        json!({
            "schema_version":1,"script":"main.rhai","on_weapon_animation":observer,
            "commands":[{"name":"start"}]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(root.path().join("main.rhai"), script).unwrap();
    let loaded = Catalog::load(
        root.path().parent().unwrap(),
        &PackageSet {
            schema_version: 1,
            packages: vec![PackageEntry {
                id: "animationtest".into(),
                version: "1.0.0".into(),
                side: Side::Server,
                dir: root
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                role: None,
            }],
        },
        true,
    )
    .unwrap();
    (root, loaded)
}

#[test]
fn queued_handle_is_implicitly_required_and_native_hook_declaration_is_checked() {
    let (_root, catalog) = catalog(
        r#"
        fn cmd_start(p) { return start_weapon_animation(p,"native:body/rifle","Reload",1,12,31,"Busy"); }
        fn on_weapon_animation(p, info) {}
    "#,
        true,
    );
    let runtime = Runtime::compile(&catalog).unwrap();
    let result = runtime
        .call(
            "animationtest",
            Call {
                function: "cmd_start",
                args: vec![Dynamic::from_int(1)],
                budget: Budget::Command,
                snapshot: Arc::new(Snapshot::default()),
                caller: Some(1),
                aim: None,
                entity: None,
                state: Namespace::default(),
                entity_vars: Arc::new(Default::default()),
                world: None,
            },
        )
        .unwrap();
    assert_eq!(result.returned.as_int().unwrap(), 0);
    assert_eq!(result.ops.len(), 1);
    assert!(result.required.contains(&0));
    assert!(
        matches!(&result.ops[0],Op::StartWeaponAnimation(op) if op.nonce==1 && op.rounds==Some(31))
    );
    let (_root, missing) = self::catalog("fn cmd_start(p) {}", true);
    assert!(Runtime::compile(&missing).is_err());
    let (_root, wrong_arity) =
        self::catalog("fn cmd_start(p) {} fn on_weapon_animation(p) {}", true);
    assert!(Runtime::compile(&wrong_arity).is_err());
}

#[test]
fn player_capability_and_bounded_body_resource_are_required() {
    let valid = ops::StartWeaponAnimation {
        player: 1,
        resource: "native:body/rifle".into(),
        sequence: "Reload".into(),
        nonce: 1,
        expected_rounds: Some(12),
        rounds: Some(31),
        image_state: Some("Busy".into()),
    };
    assert!(
        ops::authorize(
            "animationtest",
            &["player".into()],
            &Op::StartWeaponAnimation(valid.clone())
        )
        .is_ok()
    );
    assert!(
        ops::authorize(
            "animationtest",
            &[],
            &Op::StartWeaponAnimation(valid.clone())
        )
        .is_err()
    );
    for case in 0..7 {
        let mut op = valid.clone();
        match case {
            0 => op.resource = "native:body/../rifle".into(),
            1 => op.resource = "native:weapon/rifle".into(),
            2 => op.nonce = 0,
            3 => op.nonce = u64::MAX,
            4 => op.rounds = Some(100_001),
            5 => op.sequence = "Reload\n".into(),
            6 => op.image_state = Some("Busy\n".into()),
            _ => unreachable!(),
        }
        assert!(
            ops::authorize(
                "animationtest",
                &["player".into()],
                &Op::StartWeaponAnimation(op)
            )
            .is_err(),
            "case {case}"
        );
    }
}
