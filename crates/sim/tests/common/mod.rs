//! Shared session test helpers for the v20 tool images.
#![allow(dead_code)]
use bri_sim::{
    player::MoveInput,
    session::{Command, InspectMode, Notice, Session},
};
use bri_world::Brick;

/// The generated native weapon pack, which includes the core tool images.
pub fn weapon_pack() -> bri_weapons::Pack {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    bri_weapons::Pack::from_json(&std::fs::read(path).expect("Run the documented importer first"))
        .unwrap()
}

/// A movement sequence newer than any the tests sent before.
pub fn move_sequence(s: &Session) -> u64 {
    1_000_000 + s.simulation().state().tick
}

/// Keep the player's current look and refresh the input lease.
pub fn hold_still(s: &mut Session, owner: u64) {
    let player = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let sequence = move_sequence(s);
    s.movement(
        owner,
        sequence,
        MoveInput {
            yaw: player.yaw,
            pitch: player.pitch,
            ..Default::default()
        },
    )
    .unwrap();
}

/// Equip a tool slot and click: press until the swing lands, then let go
/// and wait for the image to be ready again. The trigger is the player's
/// held button, as in v20: held, the hammer would keep swinging.
pub fn swing(s: &mut Session, owner: u64, seq: u64, slot: usize) -> anyhow::Result<()> {
    s.equip_tool(owner, Some(slot))?;
    hold_still(s, owner);
    s.command(owner, seq, Command::WeaponTrigger { down: true })?;
    for _ in 0..8 {
        s.step()?;
    }
    s.release_trigger(owner)?;
    // The wrench's Fire alone lasts half a second.
    for _ in 0..240 {
        let ready = s.weapon_view().images.get(&owner).is_none_or(|images| {
            images
                .iter()
                .all(|image| image.hand != 0 || image.state == "Ready")
        });
        if ready {
            break;
        }
        s.step()?;
    }
    Ok(())
}

/// The dialog a wrench or printer hit opened for this player, if any.
pub fn opened(s: &mut Session, owner: u64) -> Option<(u64, Brick, InspectMode)> {
    s.take_private_notices()
        .into_iter()
        .rev()
        .find_map(|(to, notice)| match notice {
            Notice::Inspected {
                brick_id,
                brick,
                mode,
            } if to == owner => Some((brick_id, *brick, mode)),
            _ => None,
        })
}

/// Centre prints sent to this player.
pub fn center_prints(s: &mut Session, owner: u64) -> Vec<String> {
    s.take_private_notices()
        .into_iter()
        .filter_map(|(to, notice)| match notice {
            Notice::Center { text, .. } if to == owner => Some(text),
            _ => None,
        })
        .collect()
}
