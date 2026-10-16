//! Exclusive presentation claims. Conflicts leave the affected stock controls
//! or camera active; package load order never chooses an owner.
use bri_client_sandbox::{
    Captured,
    host::{ALL_CONTROLS, CONTROLS, LOOK_CONTROL},
};
use std::collections::BTreeMap;

type Conflicts = Vec<(String, Vec<String>)>;

#[derive(Default)]
pub(super) struct Input {
    owners: BTreeMap<u32, String>,
}

pub(super) struct Resolution {
    pub granted: Vec<u32>,
    pub conflicts: Conflicts,
}

impl Input {
    pub fn resolve(&mut self, requests: &[(&str, u32)], captured: &mut Captured) -> Resolution {
        let mut owners = BTreeMap::new();
        let mut conflicts = Vec::new();
        let mut granted = vec![0; requests.len()];
        let mut retained = 0;
        let mut taken = 0;
        for (index, name) in CONTROLS.iter().enumerate() {
            let bit = 1 << index;
            let asking: Vec<_> = requests
                .iter()
                .enumerate()
                .filter(|(_, (_, mask))| mask & bit & ALL_CONTROLS != 0)
                .collect();
            if let [(slot, (id, _))] = asking.as_slice() {
                granted[*slot] |= bit;
                taken |= bit;
                if self.owners.get(&bit).is_some_and(|previous| previous == id) {
                    retained |= bit;
                }
                owners.insert(bit, (*id).to_owned());
            } else if asking.len() > 1 {
                let mut ids: Vec<_> = asking.iter().map(|(_, (id, _))| (*id).to_owned()).collect();
                ids.sort();
                conflicts.push(((*name).to_owned(), ids));
            }
        }
        captured.granted = taken;
        captured.held &= retained;
        if retained & LOOK_CONTROL == 0 {
            captured.look = [0.0; 2];
        }
        self.owners = owners;
        Resolution { granted, conflicts }
    }
}

const MAX_NOTICES: usize = 32;
const MAX_CLAIMANTS: usize = 4;
const MAX_ID_CHARS: usize = 80;

#[derive(Default)]
pub(super) struct Diagnostics {
    input: Conflicts,
    camera: Vec<String>,
    notices: usize,
}

impl Diagnostics {
    pub fn input(&mut self, conflicts: Conflicts) -> Option<String> {
        if self.input == conflicts {
            return None;
        }
        self.input = conflicts;
        if self.input.is_empty() || self.notices >= MAX_NOTICES {
            return None;
        }
        let details = self
            .input
            .iter()
            .map(|(control, ids)| format!("{control}: {}", names(ids)))
            .collect::<Vec<_>>()
            .join("; ");
        self.notice(format!(
            "Add-On input claim conflict ({details}); contested controls stay with the game"
        ))
    }

    pub fn camera(&mut self, mut claimants: Vec<String>) -> Option<String> {
        claimants.sort();
        if claimants.len() < 2 {
            claimants.clear();
        }
        if self.camera == claimants {
            return None;
        }
        self.camera = claimants;
        if self.camera.is_empty() || self.notices >= MAX_NOTICES {
            return None;
        }
        self.notice(format!(
            "Add-On camera claim conflict ({}); the game camera stays active",
            names(&self.camera)
        ))
    }

    fn notice(&mut self, mut message: String) -> Option<String> {
        self.notices += 1;
        if self.notices == MAX_NOTICES {
            message.push_str("; further claim diagnostics suppressed until leaving this game");
        }
        Some(message)
    }
}

fn names(ids: &[String]) -> String {
    let mut out = ids
        .iter()
        .take(MAX_CLAIMANTS)
        .map(|id| {
            let mut short: String = id.chars().take(MAX_ID_CHARS).collect();
            if id.chars().count() > MAX_ID_CHARS {
                short.push('…');
            }
            short
        })
        .collect::<Vec<_>>()
        .join(", ");
    if ids.len() > MAX_CLAIMANTS {
        out.push_str(&format!(", +{} more", ids.len() - MAX_CLAIMANTS));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_code::{ClientCode, Running};
    use bri_client_sandbox::{AddOnCode, Budgets, Capability, Sandbox, TrustLevel};

    const JUMP: u32 = 1 << 4;

    #[test]
    fn overlapping_bits_fail_closed_independently_of_order() {
        for requests in [
            vec![("a", JUMP | 1), ("b", JUMP | 2)],
            vec![("b", JUMP | 2), ("a", JUMP | 1)],
        ] {
            let mut input = Input::default();
            let mut captured = Captured::default();
            let result = input.resolve(&requests, &mut captured);
            assert_eq!(captured.granted, 3);
            for ((_, request), grant) in requests.iter().zip(result.granted) {
                assert_eq!(grant, request & !JUMP);
            }
            assert_eq!(
                result.conflicts,
                [("jump".into(), vec!["a".into(), "b".into()])]
            );
        }
    }

    #[test]
    fn only_the_same_owner_keeps_held_input_and_pending_look() {
        let mut input = Input::default();
        let mut captured = Captured::default();
        let mask = JUMP | LOOK_CONTROL;
        input.resolve(&[("a", mask)], &mut captured);
        captured.held = JUMP;
        captured.look = [0.2, -0.1];
        input.resolve(&[("a", mask)], &mut captured);
        assert_eq!(captured.held, JUMP);
        assert_eq!(captured.look, [0.2, -0.1]);
        input.resolve(&[("b", mask)], &mut captured);
        assert_eq!(
            captured,
            Captured {
                granted: mask,
                ..Default::default()
            }
        );
        captured.held = JUMP;
        captured.look = [1.0; 2];
        input.resolve(&[("a", mask), ("b", mask)], &mut captured);
        assert_eq!(captured, Captured::default());
        input.resolve(&[("a", mask)], &mut captured);
        assert_eq!(
            captured,
            Captured {
                granted: mask,
                ..Default::default()
            }
        );
        input.resolve(&[], &mut captured);
        assert_eq!(captured, Captured::default());
    }

    #[test]
    fn diagnostics_are_sorted_deduplicated_and_bounded_across_both_channels() {
        let mut diagnostics = Diagnostics::default();
        let ids = vec!["b".into(), "a".into()];
        let first = diagnostics.camera(ids.clone()).unwrap();
        assert!(first.contains("a, b"));
        assert!(diagnostics.camera(vec!["a".into(), "b".into()]).is_none());
        let conflict = vec![("jump".into(), vec!["a".into(), "b".into()])];
        assert!(diagnostics.input(conflict.clone()).is_some());
        assert!(diagnostics.input(conflict).is_none());
        for _ in 2..MAX_NOTICES {
            assert!(diagnostics.camera(Vec::new()).is_none());
            assert!(diagnostics.camera(ids.clone()).is_some());
        }
        assert_eq!(diagnostics.notices, MAX_NOTICES);
        diagnostics.camera(Vec::new());
        assert!(diagnostics.camera(ids).is_none());
        let many = vec!["x".repeat(200); 1000];
        assert!(names(&many).len() < 1100);
        assert!(names(&many).contains("+996 more"));
    }

    // Small synthetic Wasm modules encoded here keep these real client/sandbox
    // boundary tests independent of a WAT compiler or new manifest dependencies.
    // The frame reports action_down(jump), input_claim's grant, look_delta and
    // the number of camera callbacks through its ordinary net.message outbox.
    fn uint(mut value: u32, bytes: &mut Vec<u8>) {
        loop {
            let byte = (value & 127) as u8;
            value >>= 7;
            bytes.push(byte | if value == 0 { 0 } else { 128 });
            if value == 0 {
                break;
            }
        }
    }

    fn int(mut value: i32, bytes: &mut Vec<u8>) {
        loop {
            let byte = (value & 127) as u8;
            value >>= 7;
            let done = (value == 0 && byte & 64 == 0) || (value == -1 && byte & 64 != 0);
            bytes.push(byte | if done { 0 } else { 128 });
            if done {
                break;
            }
        }
    }

    fn text(value: &str, bytes: &mut Vec<u8>) {
        uint(value.len() as u32, bytes);
        bytes.extend(value.as_bytes());
    }

    fn constant(value: i32, bytes: &mut Vec<u8>) {
        bytes.push(0x41);
        int(value, bytes);
    }
    fn call(index: u8, bytes: &mut Vec<u8>) {
        bytes.extend([0x10, index]);
    }

    // camera: 0 inactive, 1 places, 2 traps, 3 places then traps, 4 releases input.
    fn module(mask: u32, camera: u8, release: bool, frame_traps: bool) -> Vec<u8> {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        let mut section = |id: u8, data: Vec<u8>| {
            wasm.push(id);
            uint(data.len() as u32, &mut wasm);
            wasm.extend(data);
        };
        section(
            1,
            vec![
                5, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 1, 0x7f, 0, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0x60, 0,
                0, 0x60, 2, 0x7d, 0x7d, 0,
            ],
        );
        let mut imports = vec![5];
        for (name, ty) in [
            ("input_claim", 0),
            ("action_down", 0),
            ("look_delta", 1),
            ("send", 2),
            ("camera_set", 0),
        ] {
            text("bri", &mut imports);
            text(name, &mut imports);
            imports.extend([0, ty]);
        }
        section(2, imports);
        section(3, vec![3, 3, 4, 4]);
        section(5, vec![1, 0, 1]);
        let mut exports = vec![4];
        for (name, kind, index) in [
            ("memory", 2, 0),
            ("init", 0, 5),
            ("frame", 0, 6),
            ("camera_frame", 0, 7),
        ] {
            text(name, &mut exports);
            exports.extend([kind, index]);
        }
        section(7, exports);
        let mut init = vec![0];
        constant(mask as i32, &mut init);
        call(0, &mut init);
        init.extend([0x1a, 0x0b]);
        let mut frame = vec![0];
        if frame_traps {
            frame.push(0);
        }
        constant(0, &mut frame);
        constant(4, &mut frame);
        call(1, &mut frame);
        frame.extend([0x36, 2, 0]); // i32.store action_down at 0
        constant(4, &mut frame);
        constant(if release { 0 } else { mask as i32 }, &mut frame);
        call(0, &mut frame);
        frame.extend([0x36, 2, 0]);
        constant(8, &mut frame);
        call(2, &mut frame);
        constant(16, &mut frame);
        constant(32, &mut frame);
        frame.extend([0x28, 2, 0, 0x36, 2, 0]); // copy callback counter into report
        constant(0, &mut frame);
        constant(20, &mut frame);
        call(3, &mut frame);
        frame.extend([0x1a, 0x0b]);
        let mut callback = vec![0];
        constant(32, &mut callback);
        constant(32, &mut callback);
        callback.extend([0x28, 2, 0]);
        constant(1, &mut callback);
        callback.extend([0x6a, 0x36, 2, 0]);
        if camera == 1 || camera == 3 {
            constant(64, &mut callback);
            call(4, &mut callback);
            callback.push(0x1a);
        }
        if camera == 4 {
            constant(0, &mut callback);
            call(0, &mut callback);
            callback.push(0x1a);
        }
        if camera == 2 || camera == 3 {
            callback.push(0);
        }
        callback.push(0x0b);
        let mut bodies = vec![3];
        for body in [init, frame, callback] {
            uint(body.len() as u32, &mut bodies);
            bodies.extend(body);
        }
        section(10, bodies);
        let mut data = vec![1, 0, 0x41, 0xc0, 0, 0x0b, 32]; // active segment at 64
        for value in [0f32, 5., 0., 0., 0., -1., 0., 60.] {
            data.extend(value.to_le_bytes());
        }
        section(11, data);
        wasm
    }

    fn client(specs: &[(&str, u32, u8, bool, bool)]) -> ClientCode {
        let sandbox = Sandbox::new().unwrap();
        let mut code = ClientCode::default();
        for (slot, &(id, mask, camera, release, trap)) in specs.iter().enumerate() {
            let addon = AddOnCode {
                id: id.into(),
                name: id.into(),
                version: "1.0.0".into(),
                module: module(mask, camera, release, trap),
                capabilities: [
                    Capability::InputCapture,
                    Capability::CameraControl,
                    Capability::NetMessage,
                ]
                .into(),
                shaders: vec![],
                sounds: vec![],
                sound_files: Default::default(),
                code_hash: "0".repeat(64),
            };
            code.running.push(Running {
                addon: sandbox
                    .start_in(
                        &addon,
                        Budgets::untimed(),
                        TrustLevel::Sandboxed,
                        slot as u32,
                    )
                    .unwrap(),
                renderer: None,
                frame: Default::default(),
                physics: Default::default(),
                slot: slot as u32,
                sounds: Default::default(),
            });
        }
        code
    }

    fn frame(code: &mut ClientCode) {
        code.run_frame(
            0.,
            glam::Vec3::ZERO,
            glam::Vec3::NEG_Z,
            Default::default(),
            Default::default(),
        );
    }

    fn report(code: &ClientCode, at: usize) -> [u32; 5] {
        let bytes = &code.running[at].frame.outbox[0];
        std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
    }

    #[test]
    fn real_frames_reject_overlap_at_delivery_and_at_event_capture() {
        for ids in [["a", "b"], ["b", "a"]] {
            let mut code = client(&[
                (ids[0], JUMP | LOOK_CONTROL | 1, 0, false, false),
                (ids[1], JUMP | LOOK_CONTROL | 2, 0, false, false),
            ]);
            frame(&mut code);
            assert_eq!(code.captured_controls(), 3);
            assert!(!code.capture_held(bri_ui::api::HeldControl::Jump, true));
            assert!(!code.capture_look(0.2, 0.1));
            frame(&mut code);
            for i in 0..2 {
                let result = report(&code, i);
                assert_eq!(result[0], 0);
                assert_eq!(result[1], 1 << i);
                assert_eq!(&result[2..4], &[0, 0]);
            }
        }
    }

    #[test]
    fn a_single_owner_receives_input_and_release_requires_a_fresh_press() {
        let mut code = client(&[("a", JUMP | LOOK_CONTROL, 0, false, false)]);
        frame(&mut code);
        assert!(code.capture_held(bri_ui::api::HeldControl::Jump, true));
        assert!(code.capture_look(0.2, -0.1));
        frame(&mut code);
        assert_eq!(
            report(&code, 0),
            [
                1,
                JUMP | LOOK_CONTROL,
                0.2f32.to_bits(),
                (-0.1f32).to_bits(),
                0
            ]
        );
        // Remove the owner, as a trap or unload would, and let another take it.
        code.running.clear();
        code.resolve_input_claims();
        assert!(!code.capture_held(bri_ui::api::HeldControl::Jump, false));
        let replacement = client(&[("b", JUMP | LOOK_CONTROL, 0, false, false)]);
        code.running = replacement.running;
        frame(&mut code);
        assert_eq!(report(&code, 0)[0], 0);
        assert!(code.capture_held(bri_ui::api::HeldControl::Jump, true));
        frame(&mut code);
        assert_eq!(report(&code, 0)[0], 1);
    }

    #[test]
    fn post_frame_releases_and_traps_do_not_keep_input_captured() {
        for (release, trap) in [(true, false), (false, true)] {
            let mut code = client(&[("a", JUMP, 0, release, trap)]);
            frame(&mut code);
            assert_eq!(code.captured_controls(), 0);
            assert!(!code.capture_held(bri_ui::api::HeldControl::Jump, true));
        }
    }

    #[test]
    fn camera_conflicts_run_all_callbacks_once_and_keep_the_stock_camera() {
        for ids in [["a", "b"], ["b", "a"]] {
            let mut code = client(&[(ids[0], 0, 1, false, false), (ids[1], 0, 1, false, false)]);
            frame(&mut code);
            assert!(
                code.camera_shot(glam::Vec3::ZERO, glam::Vec3::NEG_Z)
                    .is_none()
            );
            frame(&mut code);
            assert_eq!(report(&code, 0)[4], 1);
            assert_eq!(report(&code, 1)[4], 1);
        }
    }

    #[test]
    fn inactive_and_trapping_cameras_do_not_hide_a_valid_camera_or_hold_input() {
        for camera in [0, 2, 3, 4] {
            for reverse in [false, true] {
                let mut specs = [("a", JUMP, camera, false, false), ("b", 0, 1, false, false)];
                if reverse {
                    specs.reverse();
                }
                let mut code = client(&specs);
                frame(&mut code);
                assert_eq!(code.captured_controls(), JUMP);
                let shot = code
                    .camera_shot(glam::Vec3::ZERO, glam::Vec3::NEG_Z)
                    .unwrap();
                assert_eq!(shot.eye, [0., 5., 0.]);
                assert_eq!(code.captured_controls(), if camera == 0 { JUMP } else { 0 });
                frame(&mut code);
                for (i, _) in code.running.iter().enumerate() {
                    assert_eq!(report(&code, i)[4], 1);
                }
            }
        }
    }

    #[test]
    fn stopping_or_resetting_a_session_releases_claims_and_diagnostic_budget() {
        let mut code = client(&[("a", JUMP, 1, false, false), ("b", JUMP, 1, false, false)]);
        frame(&mut code);
        code.camera_shot(glam::Vec3::ZERO, glam::Vec3::NEG_Z);
        assert_eq!(code.claim_diagnostics.notices, 2);
        code.stop();
        assert_eq!(code.captured_controls(), 0);
        assert_eq!(code.claim_diagnostics.notices, 0);
        assert!(
            code.camera_shot(glam::Vec3::ZERO, glam::Vec3::NEG_Z)
                .is_none()
        );
        let mut code = client(&[("a", JUMP, 0, false, false)]);
        frame(&mut code);
        code.device_lost();
        assert_eq!(code.captured_controls(), 0);
    }
}
