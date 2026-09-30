//! Lower literal legacy fields into typed native effects. Unused fields remain
//! explicit conversion diagnostics, never executable runtime expressions.
use crate::effect_script::Declaration;
use anyhow::{Context, Result, ensure};
use bri_content::effects::*;
use std::collections::BTreeMap;

pub struct Fields {
    values: BTreeMap<String, String>,
    pub notes: Vec<String>,
}
impl Fields {
    pub fn new(d: &Declaration) -> Self {
        Self {
            values: d.fields.clone(),
            notes: vec![],
        }
    }
    pub fn text(&mut self, key: &str, default: &str) -> String {
        self.values.remove(key).unwrap_or_else(|| default.into())
    }
    pub fn number(&mut self, key: &str, default: f32) -> Result<f32> {
        let n = self
            .values
            .remove(key)
            .map(|s| s.parse::<f32>())
            .transpose()?
            .unwrap_or(default);
        ensure!(n.is_finite(), "Nonfinite field {key}");
        Ok(n)
    }
    pub fn boolean(&mut self, key: &str, default: bool) -> Result<bool> {
        match self.values.remove(key).map(|v| v.to_lowercase()).as_deref() {
            None => Ok(default),
            Some("true" | "1") => Ok(true),
            Some("false" | "0") => Ok(false),
            _ => anyhow::bail!("Invalid boolean {key}"),
        }
    }
    pub fn vector<const N: usize>(&mut self, key: &str, default: [f32; N]) -> Result<[f32; N]> {
        let Some(text) = self.values.remove(key) else {
            return Ok(default);
        };
        let v: Vec<f32> = text
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        ensure!(
            v.len() == N && v.iter().all(|n| n.is_finite()),
            "Invalid {N}-component field {key}: {text}"
        );
        Ok(v.try_into().unwrap())
    }
    pub fn color(&mut self, key: &str, default: [f32; 4]) -> Result<[f32; 4]> {
        let Some(text) = self.values.remove(key) else {
            return Ok(default);
        };
        let mut v: Vec<f32> = text
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        ensure!(
            v.len() >= 3 && v.iter().all(|n| n.is_finite()),
            "Invalid color {key}"
        );
        if v.len() == 3 {
            v.push(1.0);
        }
        if v.len() > 4 {
            self.notes.push(format!(
                "{key}: ignored surplus components as the original color field reader does: {text}"
            ));
            v.truncate(4);
        }
        Ok(v.try_into().unwrap())
    }
    pub fn finish(mut self) -> Vec<String> {
        self.notes.extend(
            self.values
                .into_iter()
                .map(|(k, v)| format!("Unadapted field {k}={v}")),
        );
        self.notes
    }
}
pub fn id(kind: &str, name: &str) -> String {
    format!("v20/{kind}/{}", name.to_lowercase())
}
pub fn texture_path(value: String, origin: &str) -> Result<String> {
    let value = value.replace('\\', "/");
    let value = if let Some(v) = value.strip_prefix("~/") {
        format!("base/{v}")
    } else if let Some(v) = value.strip_prefix("./") {
        format!(
            "{}/{v}",
            origin
                .rsplit_once('/')
                .context("Missing source directory")?
                .0
        )
    } else {
        value
    };
    ensure!(
        !value.is_empty()
            && !value.starts_with('/')
            && !value.contains(':')
            && value
                .split('/')
                .all(|p| !p.is_empty() && p != ".." && p != "."),
        "Invalid effect texture path {value}"
    );
    Ok(value.to_lowercase())
}
pub fn particle(d: &Declaration) -> Result<(Particle, Vec<String>)> {
    let mut f = Fields::new(d);
    ensure!(
        !f.boolean("animatetexture", false)?,
        "Animated texture needs adaptation"
    );
    let mut keys = Vec::new();
    let mut previous = 0.0;
    for i in 0..4 {
        let authored = f.number(&format!("times[{i}]"), [0.0, 1.0, 2.0, 2.0][i])?;
        let time = if i == 0 { 0.0 } else { authored.max(previous) };
        if time != authored {
            f.notes.push(format!(
                "times[{i}] normalized from {authored} to {time}, matching engine onAdd ordering"
            ));
        }
        keys.push(ParticleKey {
            time,
            color: f.color(&format!("colors[{i}]"), [1.0; 4])?,
            size: f.number(&format!("sizes[{i}]"), 1.0)?,
        });
        previous = time;
    }
    let end = keys
        .iter()
        .position(|k| k.time >= 1.0)
        .context("Particle keys never reach end of lifetime")?;
    keys.truncate(end + 1);
    let lifetime = f.number("lifetimems", 1000.0)?.max(1.0);
    let variance = f.number("lifetimevariancems", 0.0)?;
    if variance >= lifetime {
        f.notes
            .push("Lifetime variance clamped below lifetime, matching engine onAdd".into());
    }
    let p = Particle {
        id: id("particle", &d.name),
        texture: texture_path(f.text("texturename", ""), &d.source)?,
        alpha_blend: f.boolean("useinvalpha", false)?,
        lifetime: lifetime / 1000.0,
        lifetime_variance: variance.min(lifetime - 1.0) / 1000.0,
        drag: f.number("dragcoefficient", 0.0)?,
        wind: f.number("windcoefficient", 1.0)?,
        gravity: f.number("gravitycoefficient", 0.0)?,
        inherited_velocity: f.number("inheritedvelfactor", 0.0)?,
        acceleration: f.number("constantacceleration", 0.0)?,
        spin_degrees: f.number("spinspeed", 0.0)?,
        random_spin: [
            f.number("spinrandommin", 0.0)?,
            f.number("spinrandommax", 0.0)?,
        ],
        keys,
    };
    Ok((p, f.finish()))
}
pub fn emitter(d: &Declaration, nodes: &BTreeMap<String, f32>) -> Result<(Emitter, Vec<String>)> {
    let mut f = Fields::new(d);
    let mut node = |key: &str| -> Result<f32> {
        let name = f.text(key, "");
        if name.is_empty() {
            Ok(1.0)
        } else {
            nodes
                .get(&name.to_lowercase())
                .copied()
                .with_context(|| format!("Unknown emitter node {name}"))
        }
    };
    let node_time_scale = node("emitternode")?;
    let point_node_time_scale = node("pointemitternode")?;
    // `ParticleEmitterData::onAdd` corrects what it cannot run: a period
    // under 1 ms, a period variance not below the period, theta outside
    // 0..180 or with its minimum above its maximum.
    let authored_period = f.number("ejectionperiodms", 100.0)?;
    let period = authored_period.max(1.0);
    let authored_variance = f.number("periodvariancems", 0.0)?;
    let variance = if authored_variance >= period {
        period - 1.0
    } else {
        authored_variance
    };
    let authored_theta = [f.number("thetamin", 0.0)?, f.number("thetamax", 90.0)?];
    let theta_max = authored_theta[1].clamp(0.0, 180.0);
    let theta_min = authored_theta[0].max(0.0).min(theta_max);
    let mut corrected = Vec::new();
    if [theta_min, theta_max] != authored_theta {
        corrected.push(format!(
            "theta {}..{} clamped to {theta_min}..{theta_max}",
            authored_theta[0], authored_theta[1]
        ));
    }
    if period != authored_period {
        corrected.push(format!("ejectionPeriodMS {authored_period} raised to {period}"));
    }
    if variance != authored_variance {
        corrected.push(format!(
            "periodVarianceMS {authored_variance} lowered to {variance}"
        ));
    }
    let e = Emitter {
        id: id("emitter", &d.name),
        name: f.text("uiname", ""),
        particles: f
            .text("particles", "")
            .split_whitespace()
            .map(|n| id("particle", n))
            .collect(),
        period: period / 1000.0,
        period_variance: variance.max(0.0) / 1000.0,
        speed: f.number("ejectionvelocity", 2.0)?,
        speed_variance: f.number("velocityvariance", 1.0)?,
        offset: f.number("ejectionoffset", 0.0)?,
        offset_variance: f.number("ejectionoffsetvariance", 0.0)?,
        theta_degrees: [theta_min, theta_max],
        phi_rate_degrees: f.number("phireferencevel", 0.0)?,
        phi_variance_degrees: f.number("phivariance", 360.0)?,
        lifetime: f.number("lifetimems", 0.0)? / 1000.0,
        lifetime_variance: f.number("lifetimevariancems", 0.0)? / 1000.0,
        orient: f.boolean("orientparticles", false)?,
        orient_on_velocity: f.boolean("orientonvelocity", true)?,
        override_advance: f.boolean("overrideadvance", false)?,
        use_emitter_colors: f.boolean("useemittercolors", false)?,
        use_emitter_sizes: f.boolean("useemittersizes", false)?,
        use_placement_velocity: f.boolean("useplacementforvelocity", false)?,
        node_time_scale,
        point_node_time_scale,
    };
    let mut notes = f.finish();
    notes.extend(
        corrected
            .into_iter()
            .map(|c| format!("{c}, matching engine onAdd")),
    );
    Ok((e, notes))
}

fn curve(f: &mut Fields, key: &str, time: &str, lerp: &str, from: f32, to: f32) -> Result<Curve> {
    let keys = f.text(key, "AZA");
    ensure!(
        !keys.is_empty() && keys.len() <= 256 && keys.bytes().all(|b| b.is_ascii_alphabetic()),
        "Invalid animation keys {key}"
    );
    let c = Curve {
        period: f.number(time, 5.0)?,
        linear: f.boolean(lerp, true)?,
        values: keys
            .bytes()
            .map(|b| from + (to - from) * f32::from(b.to_ascii_uppercase() - b'A') / 25.0)
            .collect(),
    };
    c.validate()?;
    Ok(c)
}
pub fn light(d: &Declaration) -> Result<(Light, Vec<String>)> {
    let mut f = Fields::new(d);
    ensure!(
        !f.boolean("animoffsets", false)? && !f.boolean("animrotation", false)?,
        "Animated light transform needs adaptation"
    );
    let color = f.color("color", [1.0; 4])?;
    let color_curves = if f.boolean("animcolor", false)? {
        let min = f.color("mincolor", [0.0, 0.0, 0.0, 1.0])?;
        let max = f.color("maxcolor", [1.0; 4])?;
        let single = f.boolean("singlecolorkeys", true)?;
        let template = f.values.clone();
        let mut curves = Vec::new();
        for i in 0..3 {
            // Each channel shares time/interpolation parameters; consuming fields
            // must not cause later channels to fall back to engine defaults.
            for key in ["colortime", "lerpcolor", "redkeys"] {
                if let Some(v) = template.get(key) {
                    f.values.insert(key.into(), v.clone());
                }
            }
            curves.push(curve(
                &mut f,
                if single {
                    "redkeys"
                } else {
                    ["redkeys", "greenkeys", "bluekeys"][i]
                },
                "colortime",
                "lerpcolor",
                min[i],
                max[i],
            )?);
        }
        Some(curves.try_into().unwrap())
    } else {
        None
    };
    let brightness_curve = if f.boolean("animbrightness", false)? {
        let min = f.number("minbrightness", 0.0)?;
        let max = f.number("maxbrightness", 1.0)?;
        Some(curve(
            &mut f,
            "brightnesskeys",
            "brightnesstime",
            "lerpbrightness",
            min,
            max,
        )?)
    } else {
        None
    };
    let radius_curve = if f.boolean("animradius", false)? {
        let min = f.number("minradius", 0.1)?;
        let max = f.number("maxradius", 20.0)?;
        Some(curve(
            &mut f,
            "radiuskeys",
            "radiustime",
            "lerpradius",
            min,
            max,
        )?)
    } else {
        None
    };
    let flare = if f.boolean("flareon", false)? {
        let color = f.color("flarecolor", [1.0; 4])?;
        let constant = f.boolean("constantsizeon", false)?;
        let size = f.number("constantsize", 1.0)?;
        Some(Flare {
            texture: texture_path(f.text("flarebitmap", ""), &d.source)?,
            color: [color[0], color[1], color[2]],
            third_person: f.boolean("flaretp", true)?,
            constant_size: constant.then_some(size),
            near_size: f.number("nearsize", 3.0)?,
            far_size: f.number("farsize", 0.5)?,
            near_distance: f.number("neardistance", 10.0)?,
            far_distance: f.number("fardistance", 30.0)?,
            fade_seconds: f.number("fadetime", 0.1)?,
            blend_mode: f.text("blendmode", "0").parse()?,
            link_color: f.boolean("linkflare", true)?,
            link_size: f.boolean("linkflaresize", false)?,
        })
    } else {
        None
    };
    let light = Light {
        id: id("light", &d.name),
        name: f.text("uiname", ""),
        enabled: f.boolean("lighton", true)?,
        color: [color[0], color[1], color[2]],
        brightness: f.number("brightness", 1.0)?,
        radius: f.number("radius", 10.0)?,
        color_curves,
        brightness_curve,
        radius_curve,
        flare,
    };
    // Disabled animation settings are inert authoring data, retained in provenance.
    for key in [
        "mincolor",
        "maxcolor",
        "minbrightness",
        "maxbrightness",
        "minradius",
        "maxradius",
        "minrotation",
        "maxrotation",
        "startoffset",
        "endoffset",
        "singlecolorkeys",
        "redkeys",
        "greenkeys",
        "bluekeys",
        "brightnesskeys",
        "radiuskeys",
        "offsetkeys",
        "rotationkeys",
        "colortime",
        "brightnesstime",
        "radiustime",
        "offsettime",
        "rotationtime",
        "lerpcolor",
        "lerpbrightness",
        "lerpradius",
        "lerpoffset",
        "lerprotation",
    ] {
        f.values.remove(key);
    }
    Ok((light, f.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect_script::Declarations;
    #[test]
    fn source_light_and_particle_curves_lower_to_native_units() {
        let mut d = Declarations::default();
        d.read(r#"datablock fxLightData(Blink){color="1 0 0";animBrightness=true;minBrightness=0;maxBrightness=9;brightnessKeys="AZA";brightnessTime=2;lerpBrightness=true;};
        datablock ParticleData(Jet){textureName="~/data/particles/cloud";lifetimeMS=130;colors[0]="0 0 1 1";colors[1]="1 0.5 0 1";colors[2]="1 0 0 0";times[0]=0;times[1]=0.1;times[2]=1;};"#,"base/core.cs").unwrap();
        let (l, notes) = light(&d.entries[0]).unwrap();
        assert!(notes.is_empty());
        assert_eq!(l.sample(1.0).0, [9.0, 0.0, 0.0]);
        let (p, notes) = particle(&d.entries[1]).unwrap();
        assert!(notes.is_empty());
        assert_eq!(p.lifetime, 0.13);
        assert_eq!(p.keys.len(), 3);
        assert_eq!(p.sample(0.1).0, [1.0, 0.5, 0.0, 1.0]);
        assert_eq!(p.texture, "base/data/particles/cloud");
        let mut library = Library {
            schema_version: 1,
            lights: vec![],
            particles: vec![p],
            emitters: vec![],
            textures: BTreeMap::from([("base/data/particles/cloud".into(), "cloud.png".into())]),
        };
        library.particles[0].keys[0].color[3] = 2.0;
        library.validate().unwrap();
        assert_eq!(library.particles[0].sample(0.0).0[3], 2.0);
        library.particles[0].keys[0].time = f32::NAN;
        assert!(library.validate().is_err());
    }

    #[test]
    fn emitters_get_the_engines_on_add_corrections() {
        let mut d = Declarations::default();
        d.read(r#"datablock ParticleData(Spark){textureName="~/data/particles/cloud";lifetimeMS=200;};
        datablock ParticleEmitterData(Flash){ejectionPeriodMS=0;periodVarianceMS=4;thetaMin=-10;thetaMax=200;particles="Spark";};
        datablock ParticleEmitterData(Burst){ejectionPeriodMS=10;periodVarianceMS=10;thetaMin=120;thetaMax=90;particles="Spark";};
        datablock ParticleEmitterData(Fine){ejectionPeriodMS=10;periodVarianceMS=2;thetaMin=0;thetaMax=45;particles="Spark";};"#,"add-ons/test/fx.cs").unwrap();
        let (p, _) = particle(&d.entries[0]).unwrap();
        let mut emitters = vec![];
        for (entry, period, variance, theta) in [
            (1, 0.001, 0.0, [0.0, 180.0]),
            (2, 0.010, 0.009, [90.0, 90.0]),
            (3, 0.010, 0.002, [0.0, 45.0]),
        ] {
            let (e, notes) = emitter(&d.entries[entry], &BTreeMap::new()).unwrap();
            assert_eq!(e.period, period);
            assert_eq!(e.period_variance, variance);
            assert_eq!(e.theta_degrees, theta);
            assert_eq!(notes.iter().any(|n| n.contains("onAdd")), entry != 3, "{notes:?}");
            emitters.push(e);
        }
        // Every corrected emitter is one the effects library accepts.
        Library {
            schema_version: 1,
            lights: vec![],
            textures: BTreeMap::from([(p.texture.clone(), "cloud.png".into())]),
            particles: vec![p],
            emitters,
        }
        .validate()
        .unwrap();
    }
}
