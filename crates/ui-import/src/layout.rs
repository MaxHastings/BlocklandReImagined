//! Convert parsed GUI objects into native `Control` trees and flattened
//! `Style`s.

use crate::torque::{Object, Value};
use bri_ui::schema::{Control, HSizing, Justify, Rgba, Style, VSizing};
use std::collections::BTreeMap;

/// Normalise a script bitmap/file reference to an image id relative to the
/// directory of the referring script (`./` = `base/client/ui` for GUI files).
pub fn image_id(reference: &str, dot_dir: &str) -> Option<String> {
    let r = reference.trim().replace('\\', "/");
    if r.is_empty() {
        return None;
    }
    let path = if let Some(rest) = r.strip_prefix("./") {
        format!("{dot_dir}/{rest}")
    } else if let Some(rest) = r.strip_prefix("~/") {
        format!("base/{rest}")
    } else {
        r
    };
    let low = path.to_ascii_lowercase();
    let low = [".png", ".jpg", ".jpeg"]
        .iter()
        .find_map(|e| low.strip_suffix(e).map(str::to_string))
        .unwrap_or(low);
    Some(low)
}

/// Parse "r g b [a]" in 0..255 ints or 0..1 floats.
pub fn color(v: &str) -> Option<Rgba> {
    let parts: Vec<f32> = v
        .split_whitespace()
        .map(|w| w.parse().ok())
        .collect::<Option<_>>()?;
    if parts.len() < 3 {
        return None;
    }
    let float = v.contains('.') && parts.iter().all(|x| *x <= 1.0);
    let conv = |x: f32| {
        let y = if float { x * 255.0 } else { x };
        y.round().clamp(0.0, 255.0) as u8
    };
    Some([
        conv(parts[0]),
        conv(parts[1]),
        conv(parts[2]),
        parts.get(3).map_or(255, |a| conv(*a)),
    ])
}

fn pair(v: Option<&Value>, default: [i32; 2]) -> [i32; 2] {
    let Some(v) = v else { return default };
    let n: Vec<i32> = v
        .as_text()
        .split_whitespace()
        .filter_map(|w| w.parse::<f32>().ok().map(|f| f as i32))
        .collect();
    if n.len() >= 2 { [n[0], n[1]] } else { default }
}

fn truthy(v: Option<&Value>) -> Option<bool> {
    v.map(|v| {
        matches!(
            v.as_text().trim().to_ascii_lowercase().as_str(),
            "1" | "true"
        )
    })
}

fn justify(v: &str) -> Justify {
    match v.trim().to_ascii_lowercase().as_str() {
        "center" => Justify::Center,
        "right" => Justify::Right,
        _ => Justify::Left,
    }
}

/// Resolve profile inheritance and convert to `Style`.
pub fn styles(
    profiles: &BTreeMap<String, Object>,
    font_ids: &dyn Fn(&str, u32) -> Option<String>,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, Style> {
    let mut out = BTreeMap::new();
    for name in profiles.keys() {
        // Walk parent chain: fields from nearest definition win.
        let mut chain = Vec::new();
        let mut cur = Some(name.as_str());
        while let Some(n) = cur {
            let Some(p) = profiles.get(n) else {
                warnings.push(format!("profile {name}: parent {n} missing"));
                break;
            };
            if chain.len() > 16 {
                warnings.push(format!("profile {name}: inheritance cycle"));
                break;
            }
            chain.push(p);
            cur = p.parent.as_deref();
        }
        let get = |k: &str| chain.iter().find_map(|p| p.fields.get(k));
        let text = |k: &str| get(k).map(|v| v.as_text().to_string());
        let col = |k: &str| text(k).and_then(|v| color(&v));
        let font = match (
            text("fontType"),
            text("fontSize").and_then(|s| s.parse::<f32>().ok()),
        ) {
            (Some(face), Some(size)) => font_ids(&face, size as u32),
            (Some(face), None) => font_ids(&face, 14),
            (None, Some(size)) => font_ids("Arial", size as u32),
            (None, None) => None,
        };
        let raw_justify = text("justify").unwrap_or_default();
        let (just, just_mac) = if raw_justify.contains("$platform") {
            // `$platform $= "macos" ? "center" : "left"`
            (Justify::Left, Some(Justify::Center))
        } else {
            (justify(&raw_justify), None)
        };
        let style = Style {
            font,
            font_color: col("fontColor"),
            font_color_hl: col("fontColorHL"),
            font_color_na: col("fontColorNA"),
            font_color_sel: col("fontColorSEL"),
            font_colors: (0..10).map(|i| col(&format!("fontColors[{i}]"))).collect(),
            fill_color: col("fillColor"),
            fill_color_hl: col("fillColorHL"),
            fill_color_na: col("fillColorNA"),
            border_color: col("borderColor"),
            border_color_hl: col("borderColorHL"),
            opaque: truthy(get("opaque")).unwrap_or(false),
            border: text("border")
                .and_then(|b| b.parse::<f32>().ok())
                .unwrap_or(0.0) as i32,
            bitmap: text("bitmap").and_then(|b| image_id(&b, "base/client/ui")),
            has_bitmap_array: truthy(get("hasBitmapArray")).unwrap_or(false),
            justify: just,
            justify_macos: just_mac,
            text_offset: pair(get("textOffset"), [0, 0]),
            font_outline: if truthy(get("doFontOutline")).unwrap_or(false) {
                col("fontOutlineColor").or(Some([0, 0, 0, 255]))
            } else {
                None
            },
            modal: truthy(get("modal")).unwrap_or(false),
            can_key_focus: truthy(get("canKeyFocus")).unwrap_or(false),
            tab: truthy(get("tab")).unwrap_or(false),
            source_line: chain.first().map_or(0, |p| p.line),
        };
        out.insert(name.clone(), style);
    }
    out
}

const PROMOTED: &[&str] = &[
    "position",
    "extent",
    "minExtent",
    "horizSizing",
    "vertSizing",
    "profile",
    "visible",
    "text",
    "bitmap",
    "command",
    "altCommand",
    "closeCommand",
    "accelerator",
    "variable",
    "mColor",
    "color",
    "groupNum",
    "buttonType",
];

pub fn control(o: &Object, images: &dyn Fn(&str) -> bool, warnings: &mut Vec<String>) -> Control {
    let f = &o.fields;
    let t = |k: &str| f.get(k).map(|v| v.as_text().to_string());
    let h_sizing = match t("horizSizing")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "left" => HSizing::Left,
        "center" => HSizing::Center,
        "width" => HSizing::Width,
        "relative" => HSizing::Relative,
        _ => HSizing::Right,
    };
    let v_sizing = match t("vertSizing")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "top" => VSizing::Top,
        "center" => VSizing::Center,
        "height" => VSizing::Height,
        "relative" => VSizing::Relative,
        _ => VSizing::Bottom,
    };
    let bitmap = t("bitmap").and_then(|b| image_id(&b, "base/client/ui"));
    if let Some(b) = &bitmap {
        let states = o.class == "GuiBitmapButtonCtrl" || o.class == "GuiAnimatedBitmapCtrl";
        let found =
            images(b) || (states && (images(&format!("{b}_n")) || images(&format!("{b}_00"))));
        if !found {
            warnings.push(format!(
                "layout line {}: {} bitmap {b:?} not in pack",
                o.line, o.class
            ));
        }
    }
    let color_field = if o.class == "GuiBitmapButtonCtrl" {
        "mColor"
    } else {
        "color"
    };
    Control {
        class: o.class.clone(),
        name: o.name.clone(),
        source_line: o.line,
        position: pair(f.get("position"), [0, 0]),
        extent: pair(f.get("extent"), [8, 2]),
        min_extent: pair(f.get("minExtent"), [8, 2]),
        h_sizing,
        v_sizing,
        style: t("profile").unwrap_or_else(|| "GuiDefaultProfile".into()),
        visible: truthy(f.get("visible")).unwrap_or(true),
        text: t("text"),
        bitmap,
        command: t("command").filter(|s| !s.is_empty()),
        alt_command: t("altCommand").filter(|s| !s.is_empty()),
        close_command: t("closeCommand").filter(|s| !s.is_empty()),
        accelerator: t("accelerator").filter(|s| !s.is_empty()),
        variable: t("variable").filter(|s| !s.is_empty()),
        color: t(color_field).and_then(|c| color(&c)),
        group: t("groupNum").and_then(|g| g.parse().ok()),
        button_type: t("buttonType"),
        fields: f
            .iter()
            .filter(|(k, _)| !PROMOTED.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.as_text().to_string()))
            .collect(),
        children: o
            .children
            .iter()
            .filter(|c| c.class != "GuiControlProfile")
            .map(|c| control(c, images, warnings))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_colors() {
        assert_eq!(
            image_id("./button1", "base/client/ui").unwrap(),
            "base/client/ui/button1"
        );
        assert_eq!(
            image_id("~/data/missions/default.jpg", "x").unwrap(),
            "base/data/missions/default"
        );
        assert_eq!(
            image_id("Add-Ons/Face_Jirue/Knight.png", "x").unwrap(),
            "add-ons/face_jirue/knight"
        );
        assert_eq!(color("255 150 0 255"), Some([255, 150, 0, 255]));
        assert_eq!(color("0.2 0.5 1 1"), Some([51, 128, 255, 255]));
        assert_eq!(color("50 130 255"), Some([50, 130, 255, 255]));
    }
}
