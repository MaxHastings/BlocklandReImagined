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
        ],
        sources: vec![],
        scope: serde_json::Value::Null,
    }
}
