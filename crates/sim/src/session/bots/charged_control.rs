//! Safe continuity of native release-only charged attacks.
//! Waiting preserves the authored image state; only a validated release fires.
use bri_weapons::{Image, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FireAdmission {
    Allow,
    HoldCharge,
    Abort,
}

/// Every onFire entry must require trigger-up. Initial/mixed/fused images
/// cannot use an indefinite held charge as a harmless aiming wait: a fused
/// one held to aim would burn its fuse in the hand. A fused image that
/// throws on the press holds nothing.
pub(super) fn release_only(image: &Image) -> bool {
    !image.charges()
        || (image.cook.is_none()
            && image
                .states
                .first()
                .is_none_or(|s| !s.script.eq_ignore_ascii_case("onfire"))
            && image.states.iter().all(|s| {
                [s.timeout, s.down, s.ammo, s.no_ammo, s.loaded, s.not_loaded]
                    .into_iter()
                    .flatten()
                    .all(|to| {
                        image
                            .states
                            .get(to)
                            .is_none_or(|next| !next.script.eq_ignore_ascii_case("onfire"))
                    })
            }))
}

/// Conservative finite reachability with the button up. A recovery state
/// which can only reach Ready must keep its native cooldown, even if the
/// next charge's press arrived early. A path into onFire requires cancellation.
pub(super) fn release_may_fire(image: &Image, current: &State) -> bool {
    let mut visited = [false; 128];
    let mut pending = Vec::with_capacity(image.states.len().min(128));
    let mut enqueue = |state: &State, pending: &mut Vec<usize>| {
        for at in [
            state.up,
            state.timeout,
            state.ammo,
            state.no_ammo,
            state.loaded,
            state.not_loaded,
        ]
        .into_iter()
        .flatten()
        {
            let Some(seen) = visited.get_mut(at) else {
                return false;
            };
            if !*seen {
                *seen = true;
                pending.push(at);
            }
        }
        true
    };
    if !enqueue(current, &mut pending) {
        return true;
    }
    while let Some(at) = pending.pop() {
        let Some(state) = image.states.get(at) else {
            return true;
        };
        if state.script.eq_ignore_ascii_case("onfire") || !enqueue(state, &mut pending) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_distinguishes_native_windup_from_held_cooldown() {
        let image = bri_weapons::testing::pack().images[bri_weapons::testing::SPEAR_IMAGE].clone();
        let windup = image
            .states
            .iter()
            .find(|s| s.script.eq_ignore_ascii_case("oncharge"))
            .unwrap();
        let armed = image
            .states
            .iter()
            .find(|s| image.fires_on_release(s))
            .unwrap();
        let cooldown = image
            .states
            .iter()
            .find(|s| s.script.eq_ignore_ascii_case("onfire"))
            .unwrap();
        assert!(release_may_fire(&image, windup));
        assert!(release_may_fire(&image, armed));
        assert!(!release_may_fire(&image, cooldown));
        let mut mixed = image.clone();
        let fire = mixed
            .states
            .iter()
            .position(|s| s.script.eq_ignore_ascii_case("onfire"))
            .unwrap();
        mixed.states[fire].timeout = Some(fire);
        assert!(release_may_fire(&mixed, &mixed.states[fire]));
    }

    #[test]
    fn indirect_trigger_up_requires_release_validation() {
        let mut image =
            bri_weapons::testing::pack().images[bri_weapons::testing::SPEAR_IMAGE].clone();
        let armed = image
            .states
            .iter()
            .position(|s| image.fires_on_release(s))
            .unwrap();
        let relay = image.states.len();
        let mut state = image.states[armed].clone();
        state.name = "ReleaseRelay".into();
        state.ticks = 0;
        state.wait = false;
        image.states.push(state);
        image.states[armed].up = Some(relay);
        assert!(release_only(&image));
        assert!(!image.fires_on_release(&image.states[armed]));
        assert!(release_may_fire(&image, &image.states[armed]));
    }
}
