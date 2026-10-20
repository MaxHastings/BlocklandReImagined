use bri_emitters::fog::*;

fn zone(color: Rgb, start: f32, end: f32, blend: f32) -> ZoneFog {
    let set = FogSet { color, start, end };
    ZoneFog {
        distance_fog: true,
        normal: set,
        night_vision: set,
        blend_time: blend,
        real_end: end,
        ..Default::default()
    }
}

fn input<'a>(zone_number: i32, zone: &'a ZoneFog) -> FogInput<'a> {
    FogInput {
        zone_number,
        zone,
        volume: None,
        night_vision: false,
        glow_pass: false,
        show_fog: true,
    }
}

#[test]
fn the_first_frame_in_a_zone_takes_its_fog_at_once() {
    let z = zone([100, 110, 120], 0.0, 6000.0, 10.0);
    let mut v = FogViewer::default();
    let f = v.main_view(&input(3, &z));
    assert!(f.fog.enabled);
    assert_eq!(
        (f.fog.start, f.fog.end, f.fog.color),
        (0.0, 6000.0, [100, 110, 120])
    );
    assert_eq!(f.far_plane, 6000.0);
}

#[test]
fn a_zone_change_blends_over_the_new_zones_blend_time() {
    // Zone 1 has a blend time, so its first frame snaps and becomes the blend's start. (With
    // a zero blend time the snapshot is never taken, and the next zone fades in from no fog.)
    let a = zone([200, 0, 0], 0.0, 4000.0, 1.0);
    let b = zone([0, 100, 0], 1000.0, 8000.0, 10.0);
    let mut v = FogViewer::default();
    v.main_view(&input(1, &a));
    v.tick(0.016);
    // Cross into zone 2: nothing has elapsed, so the fog is still zone 1's.
    let f = v.main_view(&input(2, &b));
    assert!(f.fog.enabled);
    assert!((f.fog.end - 4000.0).abs() < 1.0);
    assert_eq!(f.fog.color, [200, 0, 0]);
    // Five seconds in: halfway.
    v.tick(5.0);
    let f = v.main_view(&input(2, &b));
    assert!((f.fog.start - 500.0).abs() < 1.0, "{}", f.fog.start);
    assert!((f.fog.end - 6000.0).abs() < 2.0, "{}", f.fog.end);
    assert_eq!(f.fog.color, [100, 50, 0]);
    // Past the blend time it lands on the zone's values and stays.
    v.tick(6.0);
    let f = v.main_view(&input(2, &b));
    assert_eq!(
        (f.fog.start, f.fog.end, f.fog.color),
        (1000.0, 8000.0, [0, 100, 0])
    );
    assert_eq!(f.far_plane, 8000.0);
}

#[test]
fn leaving_fog_for_a_clear_zone_fades_out_before_turning_off() {
    let a = zone([90, 90, 90], 0.0, 5000.0, 4.0);
    let clear = ZoneFog::default();
    let mut v = FogViewer::default();
    v.main_view(&input(1, &a));
    v.tick(0.1);
    let mid = v.main_view(&input(2, &clear));
    // The clear zone has no blend time, so it snaps to "no fog" (start and end at FAR).
    assert!(!mid.fog.enabled || mid.fog.end >= 5000.0);
    v.tick(1.0);
    let after = v.main_view(&input(2, &clear));
    assert!(!after.fog.enabled);
    assert_eq!(after.far_plane, FAR);
}

#[test]
fn volume_fog_wins_ignores_the_show_flag_and_skips_blending() {
    let z = zone([10, 10, 10], 0.0, 1000.0, 5.0);
    let set = FogSet {
        color: [0, 40, 80],
        start: 0.0,
        end: 300.0,
    };
    let vol = VolumeFog {
        distance_fog: true,
        normal: set,
        night_vision: set,
    };
    let mut inp = input(1, &z);
    inp.volume = Some(&vol);
    inp.show_fog = false;
    let f = FogViewer::default().main_view(&inp);
    assert!(f.fog.enabled);
    assert_eq!((f.fog.end, f.fog.color), (300.0, [0, 40, 80]));
}

#[test]
fn the_show_flag_gates_zone_fog_and_the_glow_pass_is_black() {
    let z = zone([90, 90, 90], 0.0, 5000.0, 0.0);
    let mut inp = input(1, &z);
    inp.show_fog = false;
    assert!(!FogViewer::default().main_view(&inp).fog.enabled);
    assert!(!static_fog(&inp).enabled);
    inp.show_fog = true;
    inp.glow_pass = true;
    assert_eq!(FogViewer::default().main_view(&inp).fog.color, [0; 3]);
    assert_eq!(static_fog(&inp).color, [0; 3]);
}

#[test]
fn clear_colour_follows_the_zone_and_night_vision_uses_its_own_set() {
    let mut z = zone([30, 60, 90], 0.0, 5000.0, 0.0);
    z.clear_to_fog_color = true;
    z.night_vision = FogSet {
        color: [0, 255, 0],
        start: 0.0,
        end: 2000.0,
    };
    let f = FogViewer::default().main_view(&input(1, &z));
    assert_eq!(f.clear_color, Some([30, 60, 90]));
    let mut nv = input(1, &z);
    nv.night_vision = true;
    let f = FogViewer::default().main_view(&nv);
    assert_eq!((f.fog.end, f.clear_color), (2000.0, Some([0, 255, 0])));
}

#[test]
fn the_driver_widens_a_degenerate_range_and_fades_linearly() {
    let f = Fog {
        enabled: true,
        start: 100.0,
        end: 100.4,
        color: [0; 3],
    };
    assert_eq!(f.driver_range(), (99.4, 100.4));
    let g = Fog {
        enabled: true,
        start: 1000.0,
        end: 3000.0,
        color: [0; 3],
    };
    assert_eq!(g.factor(500.0), 1.0);
    assert!((g.factor(2000.0) - 0.5).abs() < 1e-6);
    assert_eq!(g.factor(4000.0), 0.0);
}

#[test]
fn the_fog_distance_option_pulls_the_end_toward_its_minimum() {
    let mut zones = [zone([1; 3], 0.0, 8000.0, 0.0), {
        let mut z = zone([1; 3], 0.0, 6000.0, 0.0);
        z.end_min = 2000.0;
        z
    }];
    init_lod(&mut zones);
    update_lod(&mut zones, 0.5);
    // Without a minimum the end stays; with one it halves the way to the minimum.
    assert_eq!(zones[0].normal.end, 8000.0);
    assert_eq!(zones[1].normal.end, 4000.0);
    update_lod(&mut zones, 1.0);
    assert_eq!(zones[1].normal.end, 6000.0);
}

#[test]
fn a_zero_blend_zone_never_snapshots_so_the_next_fog_fades_in_from_nothing() {
    let a = zone([200, 0, 0], 0.0, 4000.0, 0.0);
    let b = zone([0, 100, 0], 1000.0, 8000.0, 10.0);
    let mut v = FogViewer::default();
    v.main_view(&input(1, &a));
    v.tick(0.016);
    let f = v.main_view(&input(2, &b));
    // Last start was still FAR, so the fog is "not there yet" in zone 2's own colour.
    assert!(f.fog.enabled);
    assert!(f.fog.end > 60000.0, "{}", f.fog.end);
    assert_eq!(f.fog.color, [0, 100, 0]);
}
