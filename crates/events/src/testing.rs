//! A small catalog in the vanilla shape for host tests that have no
//! converted content.
use crate::{Catalog, InputDef, OutputDef, Param};

/// A small wrench event catalog in the vanilla shape: a few inputs with the
/// usual target slots and brick/player outputs.
pub fn catalog() -> Catalog {
    let input = |name: &str, targets: &[(&str, &str)]| InputDef {
        id: format!("in/{name}"),
        class_name: "fxDTSBrick".into(),
        name: name.into(),
        targets: targets
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        source: "fixture".into(),
        source_line: 1,
    };
    let output = |class: &str, name: &str, params| OutputDef {
        id: format!("out/{class}/{name}"),
        class_name: class.into(),
        name: name.into(),
        params,
        append_client: false,
        source: "fixture".into(),
        source_line: 1,
        package: None,
    };
    let player = [
        ("Self", "fxDTSBrick"),
        ("Player", "Player"),
        ("Client", "GameConnection"),
        ("MiniGame", "MiniGame"),
    ];
    Catalog {
        schema_version: 1,
        inputs: vec![
            input("onActivate", &player),
            input("onPlayerTouch", &player),
            input("onRelay", &[("Self", "fxDTSBrick")]),
            input("onRespawn", &[("Self", "fxDTSBrick")]),
            input("onToolBreak", &player),
        ],
        outputs: vec![
            output(
                "fxDTSBrick",
                "setColor",
                vec![Param::PaintColor { default: 0 }],
            ),
            output("fxDTSBrick", "setColliding", vec![Param::Bool]),
            output(
                "fxDTSBrick",
                "setLight",
                vec![Param::Datablock {
                    class_name: "FxLightData".into(),
                }],
            ),
            output(
                "fxDTSBrick",
                "fakeKillBrick",
                vec![
                    Param::Vector { max_length: 200.0 },
                    Param::Int {
                        min: 0,
                        max: 300,
                        default: 5,
                    },
                ],
            ),
            output(
                "fxDTSBrick",
                "setColorFX",
                vec![Param::List {
                    items: (0..7).map(|i| (format!("fx{i}"), i)).collect(),
                }],
            ),
            output("fxDTSBrick", "setRendering", vec![Param::Bool]),
            output("fxDTSBrick", "respawnVehicle", vec![]),
            output(
                "fxDTSBrick",
                "setEventEnabled",
                vec![Param::IntList { width: 157 }, Param::Bool],
            ),
            output("fxDTSBrick", "fireRelay", vec![]),
            output("Player", "kill", vec![]),
            output(
                "Player",
                "addVelocity",
                vec![Param::Vector { max_length: 200.0 }],
            ),
            output(
                "Player",
                "setPlayerScale",
                vec![Param::Float {
                    min: 0.25,
                    max: 4.0,
                    step: 0.25,
                    default: 1.0,
                }],
            ),
            output(
                "Player",
                "ChangeDatablock",
                vec![Param::Datablock {
                    class_name: "PlayerData".into(),
                }],
            ),
            output(
                "GameConnection",
                "CenterPrint",
                vec![
                    Param::String {
                        max_length: 200,
                        width: 150,
                    },
                    Param::Int {
                        min: 0,
                        max: 30,
                        default: 3,
                    },
                ],
            ),
        ],
        targets: vec![],
        sources: vec![],
        scope: serde_json::Value::Null,
    }
}

/// [`catalog`] plus the brick outputs that spawn, push, reset and cancel
/// (`spawnExplosion`, `spawnItem`, `radiusImpulse`, `setRayCasting`,
/// `setEmitter`, `cancelEvents`, `incrementPrintCount`, the mini-game's
/// `Reset`) and the bot input `onBotTouch` with its Bot and
/// Driver targets. Kept apart so the fuzzers' catalog stays as it is.
/// Parameter ranges are made up.
pub fn catalog_extended() -> Catalog {
    let mut c = catalog();
    let template = c.inputs[0].clone();
    c.inputs.push(InputDef {
        id: "in/onBotTouch".into(),
        name: "onBotTouch".into(),
        targets: [
            ("Self", "fxDTSBrick"),
            ("Bot", "Player"),
            ("Driver", "Player"),
            ("Client", "GameConnection"),
            ("MiniGame", "MiniGame"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect(),
        ..template
    });
    let template = c.outputs[0].clone();
    let output = |class: &str, name: &str, params| OutputDef {
        id: format!("out/{class}/{name}"),
        class_name: class.into(),
        name: name.into(),
        params,
        ..template.clone()
    };
    c.outputs.extend([
        output("fxDTSBrick", "setRayCasting", vec![Param::Bool]),
        output(
            "fxDTSBrick",
            "spawnExplosion",
            vec![
                Param::Datablock {
                    class_name: "ProjectileData".into(),
                },
                Param::Float {
                    min: 0.25,
                    max: 4.0,
                    step: 0.25,
                    default: 1.0,
                },
            ],
        ),
        output(
            "fxDTSBrick",
            "spawnItem",
            vec![
                Param::Vector { max_length: 200.0 },
                Param::Datablock {
                    class_name: "ItemData".into(),
                },
            ],
        ),
        output(
            "fxDTSBrick",
            "radiusImpulse",
            vec![
                Param::Int {
                    min: 1,
                    max: 40,
                    default: 6,
                },
                Param::Int {
                    min: -8000,
                    max: 8000,
                    default: 30,
                },
                Param::Int {
                    min: -8000,
                    max: 8000,
                    default: 15,
                },
            ],
        ),
        output(
            "fxDTSBrick",
            "setEmitter",
            vec![Param::Datablock {
                class_name: "ParticleEmitterData".into(),
            }],
        ),
        output("fxDTSBrick", "cancelEvents", vec![]),
        output(
            "fxDTSBrick",
            "incrementPrintCount",
            vec![Param::Int {
                min: 1,
                max: 9,
                default: 1,
            }],
        ),
        output("MiniGame", "Reset", vec![]),
    ]);
    c
}
