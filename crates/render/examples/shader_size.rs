//! The HLSL naga makes of the scene shaders, per entry point (what DX12 hands the shader compiler): bytes, lines and the largest
//! functions. `cargo run -p bri-render --example shader_size --release`; `SHADER_SIZE_WRITE=dir` also writes the HLSL.
use naga::ShaderStage;
use naga::back::{hlsl, pipeline_constants};
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn report(name: &str, wgsl: &str, modern: bool) {
    let module = naga::front::wgsl::parse_str(wgsl).expect("parse");
    let validator = || Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator().validate(&module).expect("validate");
    println!(
        "{name}: WGSL {} bytes, {} functions, {} global variables",
        wgsl.len(),
        module.functions.len(),
        module.global_variables.len()
    );
    let mut stages = vec![(ShaderStage::Vertex, "vs_main", 0u32)];
    stages.extend((0..6).map(|group| (ShaderStage::Fragment, "fs_main", group)));
    for (stage, entry, group) in stages {
        let mut list = vec![
            ("OUTPUT_ENCODED".to_string(), 0.0),
            ("COLOR_VISION".to_string(), 0.0),
            ("RAMP_BRIGHTNESS".to_string(), 0.5),
            ("RAMP_CONTRAST".to_string(), 0.5),
            ("RAMP_GAMMA".to_string(), 1.2),
        ];
        list.push(("SURFACE_GROUP".to_string(), f64::from(group)));
        if modern {
            list.push(("MODERN_SKY_SHADING".to_string(), 1.0));
        }
        let constants = list.into_iter().collect();
        let (module, info) =
            pipeline_constants::process_overrides(&module, &info, Some((stage, entry)), &constants)
                .expect("overrides");
        let mut text = String::new();
        let options = hlsl::Options::default();
        let pipeline = hlsl::PipelineOptions {
            entry_point: Some((stage, entry.to_string())),
        };
        hlsl::Writer::new(&mut text, &options, &pipeline)
            .write(&module, &info, None)
            .expect("hlsl");
        if let Ok(dir) = std::env::var("SHADER_SIZE_WRITE") {
            std::fs::create_dir_all(&dir).unwrap();
            let file = name.split_whitespace().next().unwrap().replace('.', "_");
            std::fs::write(format!("{dir}/{file}_{entry}{group}.hlsl"), &text).unwrap();
        }
        println!(
            "  {entry} group {group}: HLSL {} bytes, {} lines, {} functions kept",
            text.len(),
            text.lines().count(),
            module.functions.len()
        );
        let mut sizes: Vec<(usize, String)> = Vec::new();
        let mut current: Option<(String, usize)> = None;
        for line in text.lines() {
            let top = !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty();
            if top && line.contains('(') && !line.starts_with("//") && !line.ends_with(';') {
                if let Some((n, s)) = current.take() {
                    sizes.push((s, n));
                }
                current = Some((line.chars().take(80).collect(), 0));
            }
            if let Some((_, s)) = current.as_mut() {
                *s += line.len() + 1;
            }
        }
        if let Some((n, s)) = current.take() {
            sizes.push((s, n));
        }
        sizes.sort_by(|a, b| b.0.cmp(&a.0));
        for (size, n) in sizes.iter().take(6) {
            println!("      {size:>8} bytes  {n}");
        }
    }
}

fn main() {
    use bri_render::color::shader_source;
    report(
        "scene_original.wgsl (classic)",
        &shader_source(include_str!("../src/scene_original.wgsl")),
        false,
    );
    report(
        "scene.wgsl (modern)",
        &shader_source(
            &std::env::var("SHADER_SIZE_MODERN")
                .map(|p| std::fs::read_to_string(p).unwrap())
                .unwrap_or_else(|_| include_str!("../src/scene.wgsl").to_string()),
        ),
        true,
    );
}
