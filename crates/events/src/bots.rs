//! Builder controls for a brick's bot. Distances use native world units;
//! these are explicit native controls, not guessed legacy event parameters.
use crate::*;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BotOp {
    Powered(bool),
    Respawn,
    SightRadius(f32),
    WanderRadius(f32),
    ReturnHome(bool),
    AutoRespawn(bool),
    MeleeDamage(f32),
}

pub fn catalog(base: &Catalog) -> Result<Catalog> {
    let mut out = base.clone();
    for name in ["onBotSpawn", "onBotDeath"] {
        if out.input(name).is_none() {
            let mut targets = vec![
                ("Self".into(), "fxDTSBrick".into()),
                ("Bot".into(), "Player".into()),
            ];
            if name == "onBotDeath" {
                targets.extend([
                    ("Player".into(), "Player".into()),
                    ("Client".into(), "GameConnection".into()),
                    ("MiniGame".into(), "MiniGame".into()),
                ]);
            }
            out.inputs.push(InputDef {
                id: format!("core:bots/{name}"),
                class_name: "fxDTSBrick".into(),
                name: name.into(),
                targets,
                source: "core:bots".into(),
                source_line: 0,
            });
        }
    }
    let radius = |max, default| Param::Float {
        min: 0.0,
        max,
        step: 0.5,
        default,
    };
    for (name, params) in [
        ("setBotPowered", vec![Param::Bool]),
        ("respawnBot", vec![]),
        ("setBotSightRadius", vec![radius(400.0, 80.0)]),
        ("setBotWanderRadius", vec![radius(64.0, 12.0)]),
        ("setBotReturnHome", vec![Param::Bool]),
        ("setBotAutoRespawn", vec![Param::Bool]),
        ("setBotMeleeDamage", vec![radius(1000.0, 15.0)]),
    ] {
        if out.output(Class::Brick, name).is_none() {
            out.outputs.push(OutputDef {
                id: format!("core:bots/{name}"),
                class_name: "fxDTSBrick".into(),
                name: name.into(),
                params,
                append_client: false,
                source: "core:bots".into(),
                source_line: 0,
                package: None,
            });
        }
    }
    out.validate()?;
    Ok(out)
}

pub(crate) fn compile(name: &str, values: &[Value]) -> Option<Result<BotOp>> {
    let number = || match values {
        [Value::Float(n)] => Ok(*n),
        _ => bail!("Bot control needs one numeric parameter"),
    };
    let boolean = || match values {
        [Value::Bool(b)] => Ok(*b),
        _ => bail!("Bot control needs one boolean parameter"),
    };
    Some(match name {
        "setbotpowered" => boolean().map(BotOp::Powered),
        "respawnbot" if values.is_empty() => Ok(BotOp::Respawn),
        "setbotsightradius" => number().map(BotOp::SightRadius),
        "setbotwanderradius" => number().map(BotOp::WanderRadius),
        "setbotreturnhome" => boolean().map(BotOp::ReturnHome),
        "setbotautorespawn" => boolean().map(BotOp::AutoRespawn),
        "setbotmeleedamage" => number().map(BotOp::MeleeDamage),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_catalog_setup_keeps_one_vocabulary_and_rejects_invalid_ranges() {
        let first = crate::rules::workshop_catalog(&crate::testing::catalog()).unwrap();
        let again = crate::rules::workshop_catalog(&first).unwrap();
        assert_eq!(first.inputs, again.inputs);
        assert_eq!(first.outputs, again.outputs);
        let mut row = Row {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onActivate".into(),
            delay_ms: 0,
            target: Target::Slot(Slot::SelfBrick),
            output: "setBotSightRadius".into(),
            params: vec![Value::Float(0.0)],
        };
        let bindings = Bindings::default();
        assert!(first.validate_row(&row, &bindings).is_ok());
        for radius in [-1.0, 401.0, f32::NAN, f32::INFINITY] {
            row.params = vec![Value::Float(radius)];
            assert!(first.validate_row(&row, &bindings).is_err(), "{radius}");
        }
    }
}
