//! Native JSON/UI migration only. Original BLS/Torque parsing remains offline.
use crate::*;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;
pub fn row_selection(text: &str) -> Result<RowSelection> {
    if text.trim().eq_ignore_ascii_case("ALL") {
        return Ok(RowSelection::All);
    }
    let values = text
        .split_whitespace()
        .map(str::parse::<u16>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        values.len() <= 4096 && values.iter().all(|i| *i < 4096),
        "Event row index exceeds 4095"
    );
    Ok(RowSelection::Indices(values))
}
pub fn legacy_event(
    e: &Json,
    mut resolve: impl FnMut(&str, &str) -> Option<String>,
) -> Result<Row> {
    let input = match e["input"].as_str() {
        Some("activate") => "onActivate",
        Some("touch") => "onPlayerTouch",
        _ => bail!("Unsupported legacy native input"),
    };
    let target = match e["target"]["kind"].as_str() {
        Some("this_brick") => Target::Slot(Slot::SelfBrick),
        Some("named") => Target::Named(
            e["target"]["value"]
                .as_str()
                .context("Invalid named target")?
                .into(),
        ),
        _ => bail!("Unsupported legacy native target"),
    };
    let a = &e["action"];
    let kind = a["kind"].as_str().context("Missing native action kind")?;
    let v = &a["value"];
    let (output, value) = match kind {
        "color" => (
            "setColor",
            Value::Color(u8::try_from(v.as_u64().context("Invalid color")?)?),
        ),
        "color_effect" => (
            "setColorFX",
            Value::Int(v.as_i64().context("Invalid color effect")?),
        ),
        "visible" => (
            "setRendering",
            Value::Bool(v.as_bool().context("Invalid visible")?),
        ),
        "colliding" => (
            "setColliding",
            Value::Bool(v.as_bool().context("Invalid collision")?),
        ),
        "raycast" => (
            "setRayCasting",
            Value::Bool(v.as_bool().context("Invalid raycast")?),
        ),
        "light" | "emitter" => {
            let id = if v.is_null() {
                None
            } else {
                match v["kind"].as_str() {
                    Some("resolved") => {
                        Some(v["value"].as_str().context("Invalid resolved ID")?.into())
                    }
                    Some("unresolved") => Some(
                        resolve(
                            v["value"]["namespace"]
                                .as_str()
                                .context("Missing namespace")?,
                            v["value"]["name"].as_str().context("Missing source name")?,
                        )
                        .context("Unresolved event datablock; preserve original row")?,
                    ),
                    _ => bail!("Invalid native content reference"),
                }
            };
            (
                if kind == "light" {
                    "setLight"
                } else {
                    "setEmitter"
                },
                Value::Datablock(id),
            )
        }
        _ => bail!("Unsupported legacy native action"),
    };
    Ok(Row {
        preserved: None,
        enabled: e["enabled"].as_bool().context("Invalid enabled")?,
        input: input.into(),
        delay_ms: u32::try_from(e["delay_ms"].as_u64().context("Invalid delay")?)?,
        target,
        output: output.into(),
        params: vec![value],
    })
}
/// Familiar event editor vectors use original X/Y/Z labels. Convert (x,z,-y) once at this boundary.
pub fn ui_event(e: &Json) -> Result<Row> {
    let target = if e["target"] == "<NAMED BRICK>" {
        Target::Named(
            e["named_target"]
                .as_str()
                .context("Missing named target")?
                .into(),
        )
    } else {
        let target = e["target"].as_str().context("Missing target")?;
        match Slot::parse(target) {
            Some(slot) => Target::Slot(slot),
            // An Add-On's target, checked against the catalog later.
            None => {
                ensure!(!target.is_empty() && target.len() <= 64, "Unknown target");
                Target::Derived(target.into())
            }
        }
    };
    let params = e["params"]
        .as_array()
        .context("Missing UI parameters")?
        .iter()
        .map(|p| -> Result<Value> {
            let obj = p.as_object().context("Invalid UI value")?;
            ensure!(obj.len() == 1, "Ambiguous UI value");
            let (k, v) = obj.iter().next().unwrap();
            Ok(match k.as_str() {
                "Int" | "List" => Value::Int(v.as_i64().context("Invalid int/list")?),
                "Float" => Value::Float(v.as_f64().context("Invalid float")? as f32),
                "Bool" => Value::Bool(v.as_bool().context("Invalid bool")?),
                "Text" => Value::Text(v.as_str().context("Invalid text")?.into()),
                "Datablock" => Value::Datablock(if v.is_null() {
                    None
                } else {
                    Some(v.as_str().context("Invalid datablock")?.into())
                }),
                "Vector" => Value::Vector({
                    let a: [f32; 3] = serde_json::from_value(v.clone())?;
                    glam::Vec3::new(a[0], a[2], -a[1])
                }),
                "PaintColor" => {
                    Value::Color(u8::try_from(v.as_u64().context("Invalid paint color")?)?)
                }
                _ => bail!("Unknown UI parameter {k}"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Row {
        preserved: None,
        enabled: e["enabled"].as_bool().context("Missing enabled")?,
        input: e["input"].as_str().context("Missing input")?.into(),
        delay_ms: u32::try_from(e["delay_ms"].as_u64().context("Missing delay")?)?,
        target,
        output: e["output"].as_str().context("Missing output")?.into(),
        params,
    })
}
/// Convert the UI's IntList text only when the chosen output declares that parameter type.
pub fn normalize_ui_row(catalog: &Catalog, mut row: Row) -> Result<Row> {
    let (_, output) = catalog
        .row_output(&row.input, &row.target, &row.output)
        .context("Unknown output")?;
    for (s, p) in output.params.iter().zip(&mut row.params) {
        if let Param::Float { min, max, step, .. } = s
            && let Value::Float(v) = p
        {
            ensure!(v.is_finite(), "Nonfinite event float");
            *v = *min + (((v.clamp(*min, *max) - *min) / *step) + 1e-5).floor() * *step;
        }
        if matches!(s, Param::IntList { .. })
            && let Value::Text(text) = p
        {
            *p = Value::Rows(row_selection(text)?);
        }
    }
    Ok(row)
}
pub fn ui_catalog(
    c: &Catalog,
    bound_inputs: &BTreeSet<String>,
    bound_outputs: &BTreeSet<(Class, String)>,
) -> Json {
    let inputs:Vec<_>=c.inputs.iter().map(|i|json!({"name":i.name,"targets":i.targets,"supported":bound_inputs.iter().any(|s|s.eq_ignore_ascii_case(&i.name))})).collect();
    let outputs:Vec<_>=c.outputs.iter().map(|o|{let params:Vec<_>=o.params.iter().map(|p|{let mut v=serde_json::to_value(p).unwrap();match p {Param::Datablock{class_name}=>{v.as_object_mut().unwrap().remove("class_name");v["class"]=json!(class_name);},Param::Vector{max_length}=>{v.as_object_mut().unwrap().remove("max_length");v["max"]=json!(max_length);},_=>{}}v}).collect();json!({"class":o.class_name,"name":o.name,"params":params,"supported":bound_outputs.iter().any(|(class,name)|Some(*class)==Class::parse(&o.class_name)&&name.eq_ignore_ascii_case(&o.name))})}).collect();
    json!({"inputs":inputs,"outputs":outputs})
}
pub fn world_tick_to_us(tick: u64) -> Result<u64> {
    u64::try_from(u128::from(tick) * 1_000_000 / 120).context("Native world clock overflow")
}
