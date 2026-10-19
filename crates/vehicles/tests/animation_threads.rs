//! A vehicle may play many speed-switched animation threads (a dashboard
//! of gauges, one clip for each state of each gauge): up to 64.
use bri_vehicles::schema::{AnimationThread, SCHEMA_VERSION};
use bri_vehicles::*;

fn threads(n: usize) -> Vec<AnimationThread> {
    (0..n)
        .map(|i| AnimationThread {
            slot: (i % 4) as u8,
            sequence: format!("gauge_{i}"),
            rate: 1.0,
            min_speed: Some(i as f32),
            max_speed: Some(i as f32 + 1.0),
        })
        .collect()
}

#[test]
fn up_to_sixty_four_threads_validate_and_more_do_not() {
    let valid = testing::definitions().into_iter().next().unwrap();
    for (count, ok) in [(16, true), (64, true), (65, false)] {
        let mut d = valid.clone();
        d.threads = threads(count);
        let pack = Pack {
            schema_version: SCHEMA_VERSION,
            definitions: vec![d],
            assets: vec![],
            evidence: vec![],
            unresolved: vec![],
            animation_aliases: Default::default(),
        };
        assert_eq!(pack.validate().is_ok(), ok, "{count} threads");
    }
}
