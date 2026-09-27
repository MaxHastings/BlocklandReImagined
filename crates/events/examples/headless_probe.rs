use anyhow::{Result, ensure};
use bri_events::*;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};
#[derive(Default)]
struct MeasuredHost {
    calls: u64,
    per_origin: BTreeMap<u64, u64>,
}
impl Host for MeasuredHost {
    fn alive(&self, _: Entity) -> bool {
        true
    }
    fn permitted(&self, _: &Trigger, _: Entity, _: &str) -> bool {
        true
    }
    fn relay_neighbors(&mut self, _: Id, _: Direction, _: usize) -> Result<Vec<Id>, String> {
        Ok(vec![])
    }
    fn apply(&mut self, d: &Dispatch) -> Apply {
        self.calls += 1;
        *self.per_origin.entry(d.origin).or_default() += 1;
        Apply::Applied
    }
}
fn id(index: u64) -> Id {
    Id {
        index,
        generation: 1,
    }
}
fn run(
    catalog: &Catalog,
    rows: usize,
    phase_budget: usize,
    frames: u64,
) -> Result<serde_json::Value> {
    let limits = Limits {
        steps_per_phase: phase_budget,
        steps_per_origin: phase_budget / 8,
        ..Default::default()
    };
    let bindings = Bindings {
        palette_len: 64,
        ..Default::default()
    };
    let mut w = EventWorld::new(catalog.clone(), bindings.clone(), limits)?;
    for area in 1..=8 {
        let mut program = Vec::new();
        for n in 0..rows - 1 {
            program.push(Row {
                preserved: None,
                enabled: true,
                input: "onRelay".into(),
                delay_ms: 0,
                target: Target::Slot(Slot::SelfBrick),
                output: "setColor".into(),
                params: vec![Value::Color((n % 64) as u8)],
            });
        }
        program.push(Row {
            preserved: None,
            enabled: true,
            input: "onRelay".into(),
            delay_ms: 0,
            target: Target::Slot(Slot::SelfBrick),
            output: "fireRelay".into(),
            params: vec![],
        });
        w.install_brick(BrickProgram {
            id: id(area),
            owner_scope: area,
            name: Some(format!("area-{area}")),
            rows: program,
            print_count: 0,
            implicit_cancel_relays: false,
        })?;
        w.trigger(Trigger::new(id(area), "onRelay", area))?;
    }
    let mut samples = Vec::new();
    let mut h = MeasuredHost::default();
    let mut oldest = 0;
    let mut deferred = 0;
    let mut report = RunReport::default();
    for frame in 0..frames {
        let start = Instant::now();
        report = w.advance(frame * 1_000_000 / 120, &mut h)?;
        samples.push(start.elapsed().as_secs_f64() * 1000.);
        oldest = oldest.max(report.oldest_due_age_us);
        deferred += report.due_pending as u64;
    }
    samples.sort_by(f64::total_cmp);
    ensure!(h.per_origin.len() == 8, "Origin starvation");
    let low = *h.per_origin.values().min().unwrap();
    let high = *h.per_origin.values().max().unwrap();
    ensure!(high - low <= rows as u64, "Unfair origin execution");
    let calls_before_save = h.calls;
    let per_origin_before_save = h.per_origin.clone();
    let start = Instant::now();
    let save = w.save()?;
    let save_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let mut restored = EventWorld::restore(catalog.clone(), bindings, &save)?;
    let restore_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut replay = MeasuredHost::default();
    let next = frames * 1_000_000 / 120;
    let a = w.advance(next, &mut h)?;
    let b = restored.advance(next, &mut replay)?;
    ensure!(
        a.steps == b.steps && a.pending == b.pending,
        "Checkpoint schedule divergence"
    );
    Ok(
        json!({"rows_per_area":rows,"independent_origins":8,"frames":frames,"phase_budget":phase_budget,"median_ms":samples[samples.len()/2],"p95_ms":samples[samples.len()*95/100],"max_ms":samples.last(),"oldest_due_age_us":oldest,"accumulated_deferred_counts":deferred,"host_calls_before_save":calls_before_save,"per_origin_before_save":per_origin_before_save,"per_origin_after_replay":h.per_origin,"final_phase":report,"checkpoint_bytes":save.len(),"save_ms":save_ms,"restore_ms":restore_ms,"scope":"real native event catalog and scheduler; host records typed brick mutations. AI, physics, network and rendering are not measured."}),
    )
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        a.len() == 2,
        "Usage: headless_probe <native-catalog.json> <report.json>"
    );
    let catalog = Catalog::load(&a[0])?;
    let report = json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"processor":std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default(),"inputs":catalog.inputs.len(),"outputs":catalog.outputs.len(),"sustained":run(&catalog,64,512,240)?,"overloaded":run(&catalog,4096,4096,120)?});
    std::fs::write(&a[1], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "sustained p95 {}ms, overloaded p95 {}ms",
        report["sustained"]["p95_ms"], report["overloaded"]["p95_ms"]
    );
    Ok(())
}
