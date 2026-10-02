//! Avatar animation inputs.
use super::*;

/// Avatar bodies, their actions and gestures, and the avatar screen's preview.
pub(super) struct Avatars {
    pub(super) avatar_assets: Arc<crate::avatar::AvatarAssets>,
    pub(super) avatars: BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
    /// Horses spawned at vehicle bricks: `HorseArmor` bots, animated like
    /// horse players.
    pub(super) mount_meshes: BTreeMap<u64, crate::avatar::AvatarMesh>,
    pub(super) avatar_actions: BTreeMap<u64, crate::avatar::ActionAnimation>,
    /// The script threads not tied to a mounted image, by player and thread
    /// number: 0 and 1 a package's body animations, 3 the builder and chat
    /// gestures. Thread 2 is `avatar_actions`.
    pub(super) avatar_threads: BTreeMap<u64, AvatarThreads>,
    pub(super) avatar_action_images: BTreeMap<u64, String>,
    pub(super) animation_time: f64,
    pub(super) avatar_preview: Option<crate::gpu_build::Building<crate::avatar::Preview>>,
    pub(super) preview_request: Option<(bri_content::avatar::Appearance, [f32; 3], f32)>,
    pub(super) preview_dirty: bool,
}

impl App {
    pub(super) fn update_avatar_animation_inputs(
        avatar_actions: &mut BTreeMap<u64, crate::avatar::ActionAnimation>,
        avatar_threads: &mut BTreeMap<u64, AvatarThreads>,
        avatar_action_images: &mut BTreeMap<u64, String>,
        weapon_animation_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
        weapon_animation_drops: &mut u64,
        view: &network::View,
        elapsed: f32,
    ) {
        avatar_actions.retain(|owner, _| view.poses.contains_key(owner));
        avatar_threads.retain(|owner, _| view.poses.contains_key(owner));
        avatar_action_images.retain(|owner, _| view.poses.contains_key(owner));
        // The images in a player's hands; empty when they hold nothing.
        let identity = |owner: &u64| -> String {
            let mut parts = Vec::new();
            if let Some(images) = view.weapons.images.get(owner) {
                let mut images: Vec<_> = images.iter().collect();
                images.sort_by_key(|image| image.hand);
                parts.extend(
                    images
                        .iter()
                        .map(|image| format!("{}:{}", image.hand, image.image)),
                );
            }
            parts.join("|")
        };
        // An action belongs to the hands it started with: a tool's swing
        // ends when the tool changes or is put away. One a rule started
        // with empty hands (`playThread(2, armReadyBoth)`, `death1`) plays
        // on, as v20's thread 2 does, until a tool is taken out.
        for owner in view.poses.keys() {
            let current = identity(owner);
            if avatar_action_images
                .get(owner)
                .is_some_and(|old| current != *old)
            {
                avatar_actions.remove(owner);
                avatar_action_images.remove(owner);
            }
        }
        for (_, age, _) in weapon_animation_cues.iter_mut() {
            *age += elapsed.min(0.25);
        }
        while let Some((cue, age, started_at)) = weapon_animation_cues.pop_front() {
            let bri_sim::presentation::CueKind::WeaponAnimation {
                actor,
                thread,
                sequence,
                image_hand,
            } = &cue.kind
            else {
                continue;
            };
            if play_free_thread(avatar_threads, *actor, *thread, sequence, *image_hand, started_at) {
                continue;
            }
            if *thread != 2 || sequence.eq_ignore_ascii_case("root") {
                if *thread == 2 {
                    avatar_actions.remove(actor);
                    avatar_action_images.remove(actor);
                }
                continue;
            }
            let current = identity(actor);
            // An image's own animation waits for that image to arrive; a
            // rule's (`image_hand: None`) plays with whatever is in hand.
            let hand_matches = image_hand.is_none_or(|hand| {
                view.weapons
                    .images
                    .get(actor)
                    .is_some_and(|images| images.iter().any(|image| image.hand == hand))
            });
            if !hand_matches {
                if age >= 0.5 {
                    *weapon_animation_drops = weapon_animation_drops.saturating_add(1);
                    continue;
                }
                weapon_animation_cues.push_front((cue, age, started_at));
                break;
            }
            let action = crate::avatar::ActionAnimation {
                sequence: sequence.clone(),
                started_at,
            };
            avatar_actions.insert(*actor, action);
            avatar_action_images.insert(*actor, current);
        }
    }
}
