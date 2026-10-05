//! Recovered event math in native Y-up. Host adapters apply permissions and current state.
use bri_console::Clamp;
use glam::Vec3;
/// Source direct event AddHealth heals damage or routes negative health through damage policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HealthChange {
    Unchanged,
    SetDamage(f32),
    Damage(f32),
}
pub fn add_health(max_health: f32, damage: f32, amount: i32) -> HealthChange {
    if !max_health.is_finite() || !damage.is_finite() || max_health <= 0. || damage >= max_health {
        return HealthChange::Unchanged;
    }
    if amount > 0 {
        HealthChange::SetDamage((damage - amount as f32).max(0.))
    } else {
        HealthChange::Damage(-(amount as f32))
    }
}
pub fn set_health(max_health: f32, damage: f32, health: u32) -> HealthChange {
    if !max_health.is_finite() || !damage.is_finite() || max_health <= 0. || damage >= max_health {
        return HealthChange::Unchanged;
    }
    if health == 0 {
        HealthChange::Damage(max_health)
    } else {
        HealthChange::SetDamage((max_health - health as f32).max(0.))
    }
}
pub fn bounce(incident: Vec3, normal: Vec3, factor: f32) -> Vec3 {
    (incident - normal * incident.dot(normal) * 2.)
        .mul_add(Vec3::splat(factor), Vec3::ZERO)
        .clamp_length_max(200.)
}
pub fn redirect(incident: Vec3, vector: Vec3, normalized: bool) -> Vec3 {
    (if normalized {
        vector.normalize_or_zero() * incident.length()
    } else {
        vector
    })
    .clamp_length_max(200.)
}
/// Source radius impulse has squared-distance falloff, separate radial + vertical impulses.
pub fn radius_impulse(center: Vec3, target: Vec3, radius: f32, force: f32, vertical: f32) -> Vec3 {
    if radius <= 0. {
        return Vec3::ZERO;
    }
    let delta = target - center;
    let factor = (1. - delta.length_squared() / (radius * radius)).clamped(0., 1.);
    (delta.normalize_or_zero() * force + Vec3::Y * vertical) * factor
}
/// Thin source axes collapse to box center before the three independent uniform draws.
pub fn brick_projectile_position(min: Vec3, max: Vec3, random: [f32; 3]) -> Vec3 {
    let size = max - min;
    let threshold = Vec3::new(1.05, 0.65, 1.05);
    let mut result = min;
    for i in 0..3 {
        result[i] += if size[i] < threshold[i] {
            size[i] * 0.5
        } else {
            size[i] * random[i].clamped(0., 1.)
        };
    }
    result
}
pub fn projectile_velocity(base: Vec3, variance: Vec3, random: [f32; 3]) -> Vec3 {
    base + variance * (Vec3::from_array(random).clamp(Vec3::ZERO, Vec3::ONE) - Vec3::splat(0.5))
}
/// Directional relay query slabs are 0.1 thick, centered exactly on the selected face.
pub fn relay_box(min: Vec3, max: Vec3, direction: crate::Direction) -> (Vec3, Vec3) {
    let center = (min + max) * 0.5;
    let mut size = (max - min - Vec3::splat(0.1)).max(Vec3::ZERO);
    let axis = match direction {
        crate::Direction::Up | crate::Direction::Down => 1,
        crate::Direction::East | crate::Direction::West => 0,
        _ => 2,
    };
    let position = center + direction.vector() * (size[axis] * 0.5 + 0.05);
    size[axis] = 0.1;
    (position, size)
}
/// Recovery never evicts a human occupant or a nested mounted passenger.
pub fn may_recover_vehicle(passengers: &[(bool, bool)]) -> bool {
    passengers
        .iter()
        .all(|(has_client, has_mounts)| !*has_client && !*has_mounts)
}
pub fn sound_allowed(looping: bool, spatial: bool) -> bool {
    !looping && spatial
}
pub fn brick_spawn_allowed(fake_dead_ms: u32, rendering: bool, ray_casting: bool) -> bool {
    fake_dead_ms <= 120 && (rendering || ray_casting)
}
pub fn client_message(text: &str, name: &str, score: i64, chat: bool) -> String {
    let s = text.replace("%1", name);
    if chat {
        s.replace("%2", &score.to_string())
    } else {
        s
    }
}

/// Original input adapters expose MiniGame only for matching scopes, except explicit Legacy LAN.
pub fn minigame_target(
    legacy_lan: bool,
    brick_game: Option<crate::Entity>,
    client_game: Option<crate::Entity>,
) -> Option<crate::Entity> {
    if legacy_lan {
        client_game
    } else if brick_game == client_game {
        brick_game
    } else {
        None
    }
}
pub fn touch_input_allowed(age_ms: u64, immune_ms: u64, holding_admin_wand: bool) -> bool {
    age_ms >= immune_ms && !holding_admin_wand
}
pub fn fake_kill_origin(center: Vec3, velocity: Vec3) -> (Vec3, f32) {
    (
        center - velocity.normalize_or_zero(),
        velocity.length() * 2.,
    )
}
pub fn item_spawn_position(
    brick_center: Vec3,
    brick_half: Vec3,
    item_position: Vec3,
    item_box_center: Vec3,
    item_half: Vec3,
    velocity: Vec3,
) -> Vec3 {
    let offset = item_position - item_box_center;
    let clearance = (item_half.y - brick_half.y + 0.1).max(0.);
    brick_center
        + offset
        + Vec3::Y
            * clearance
            * if velocity.y > 0. {
                1.
            } else if velocity.y < 0. {
                -1.
            } else {
                0.
            }
}
pub fn item_yaw(direction: crate::Direction) -> f32 {
    match direction {
        crate::Direction::East => std::f32::consts::FRAC_PI_2,
        crate::Direction::South => -std::f32::consts::PI,
        crate::Direction::West => -std::f32::consts::FRAC_PI_2,
        _ => 0.,
    }
}
