//! How a brick bot looks and is called, so a crowd of one kind does not
//! look or read as copies: a seeded look drawn from what the server's avatar
//! pack offers (its faces and decals) and its paint palette, and a first
//! name from its kind's `first_names` no other player has.
use super::*;

/// One step of a seeded stream (SplitMix64).
fn mix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Slots left their default colour: skin.
const SKIN: [&str; 3] = ["head", "lhand", "rhand"];

/// `avatar` with a face, a decal and clothing colours drawn by `seed` from
/// the `pack`'s faces and decals and the opaque colours of `palette`, as
/// the avatar screen's randomize draws them. Skin keeps its colour; a slot
/// the defaults do not colour stays uncoloured.
pub(super) fn seeded_look(
    seed: u64,
    avatar: &mut bri_content::avatar::Appearance,
    pack: &bri_content::avatar::Package,
    palette: &[[f32; 4]],
) {
    let mut state = seed;
    let mut pick = |n: usize| (mix(&mut state) % n.max(1) as u64) as usize;
    if !pack.faces.is_empty() {
        avatar.face = pack.faces[pick(pack.faces.len())].clone();
    }
    if !pack.decals.is_empty() {
        avatar.decal = pack.decals[pick(pack.decals.len())].clone();
    }
    let paints: Vec<[f32; 4]> = palette.iter().copied().filter(|c| c[3] >= 1.0).collect();
    if paints.is_empty() {
        return;
    }
    // Arms, legs and the pair of each match, as a player dresses.
    let mut chosen: BTreeMap<&str, [f32; 4]> = BTreeMap::new();
    for (slot, colour) in avatar.colors.iter_mut() {
        if SKIN.contains(&slot.as_str()) {
            continue;
        }
        let pair = slot
            .strip_prefix(['l', 'r'])
            .filter(|rest| ["arm", "leg"].contains(rest) && slot.len() == rest.len() + 1);
        let paint = match pair.and_then(|p| chosen.get(p)) {
            Some(c) => *c,
            None => {
                let c = paints[pick(paints.len())];
                if let Some(p) = pair {
                    chosen.insert(p, c);
                }
                c
            }
        };
        // Keep a see-through part (an accent) see-through.
        *colour = [paint[0], paint[1], paint[2], colour[3]];
    }
}

impl Session {
    /// A first name of `kind` that no player here goes by (its own bot
    /// excepted), starting from one `brick` seeds, or `None` when the kind
    /// has none or every one is taken.
    pub(super) fn bot_first_name(
        &self,
        kind: &BotKind,
        brick: BrickId,
        bot: Option<OwnerId>,
    ) -> Option<String> {
        let names = &kind.first_names;
        if names.is_empty() {
            return None;
        }
        let taken = |name: &str| {
            self.peers
                .iter()
                .any(|(o, p)| Some(*o) != bot && p.name.eq_ignore_ascii_case(name))
                || self
                    .bots
                    .brains
                    .iter()
                    .any(|(o, b)| Some(*o) != bot && b.named.eq_ignore_ascii_case(name))
        };
        let mut seed = brick;
        let start = (mix(&mut seed) % names.len() as u64) as usize;
        (0..names.len())
            .map(|i| &names[(start + i) % names.len()])
            .find(|n| !taken(n))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> bri_content::avatar::Package {
        let mut defaults = bri_content::avatar::Appearance {
            parts: Default::default(),
            colors: Default::default(),
            face: "face0".into(),
            decal: "decal0".into(),
        };
        for slot in [
            "head", "torso", "larm", "rarm", "lleg", "rleg", "hip", "accent",
        ] {
            defaults.colors.insert(slot.into(), [0.5, 0.5, 0.5, 1.0]);
        }
        defaults
            .colors
            .insert("accent".into(), [0.5, 0.5, 0.5, 0.7]);
        bri_content::avatar::Package {
            schema_version: 1,
            id: "test".into(),
            rig: String::new(),
            rig_sha256: String::new(),
            parts: Default::default(),
            accents_allowed: Default::default(),
            faces: (0..4).map(|i| format!("face{i}")).collect(),
            decals: (0..4).map(|i| format!("decal{i}")).collect(),
            surfaces: Default::default(),
            textures: Default::default(),
            defaults,
        }
    }

    #[test]
    fn seeded_looks_differ_keep_skin_and_pair_limbs() {
        let pack = pack();
        let palette: Vec<[f32; 4]> = (0..16).map(|i| [i as f32 / 16.0, 0.2, 0.7, 1.0]).collect();
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..12u64 {
            let mut a = pack.defaults.clone();
            seeded_look(seed, &mut a, &pack, &palette);
            assert_eq!(a.colors["head"], [0.5, 0.5, 0.5, 1.0], "skin stays");
            assert_eq!(a.colors["larm"], a.colors["rarm"], "arms match");
            assert_eq!(a.colors["lleg"], a.colors["rleg"], "legs match");
            assert_eq!(a.colors["accent"][3], 0.7, "see-through stays so");
            assert!(pack.faces.contains(&a.face) && pack.decals.contains(&a.decal));
            seen.insert(format!("{a:?}"));
        }
        assert!(seen.len() >= 6, "{} distinct of 12", seen.len());
        let (mut a, mut b) = (pack.defaults.clone(), pack.defaults.clone());
        seeded_look(3, &mut a, &pack, &palette);
        seeded_look(3, &mut b, &pack, &palette);
        assert_eq!(a, b, "one seed, one look");
    }
}
