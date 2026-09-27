use bri_net::{codec, protocol::*, replica::Replica};
use bri_sim::player::PlayerState;
use bri_world::{Brick, ContentRef};
use std::collections::BTreeMap;
fn checkpoint() -> Checkpoint {
    Checkpoint {
        weapons: Default::default(),
        tools: [(
            1,
            bri_sim::session::ToolInventory {
                slots: vec![None; 5],
                selected: None,
            },
        )]
        .into(),
        cue_cursor: 0,
        dropped_cues: 0,
        cursor: 0,
        tick: 10,
        world: PublicWorld {
            name: "Test".into(),
            map_id: "Fixture".into(),
            palette: vec![[1.0; 4]],
            bricks: BTreeMap::new(),
        },
        names: [(1, "Player".into())].into(),
        avatars: Default::default(),
        chat: vec![],
        poses: vec![],
        vitals: Default::default(),
        minigames: vec![],
        vehicles: vec![],
        vehicle_poses: vec![],
    }
}
fn pose(tick: u64, x: f32, yaw: f32) -> Pose {
    Pose {
        tick,
        acknowledged_input: tick,
        player: PlayerState {
            owner: 1,
            feet: [x, 0.0, 0.0],
            velocity: [0.0; 3],
            yaw,
            pitch: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jet_boost: 0.0,
            jump_held: false,
        },
    }
}

#[test]
fn malformed_inventory_delta_cannot_partially_mutate_replica() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    let mut tools = replica.tools.clone();
    tools.get_mut(&1).unwrap().selected = Some(5);
    let delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: Some(tools),
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: [(
            99,
            Some(Brick::new(
                ContentRef::Resolved("plate".into()),
                [0.5, 0.1, 0.25],
                1,
            )),
        )]
        .into(),
        names: None,
        avatars: None,
        palette: None,
        chat: vec![],
        cues: vec![],
        dropped_cues: 0,
    };
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.cursor, 0);
    assert!(replica.world.bricks.is_empty());
    let mut delta = delta;
    delta.tools = Some(BTreeMap::new());
    assert!(replica.update(delta.clone()).is_err());
    delta.tools = Some(replica.tools.clone());
    replica.update(delta).unwrap();
    assert_eq!(replica.cursor, 1);
    assert!(replica.world.bricks.contains_key(&99));
}
#[test]
fn malformed_weapon_state_or_presentation_rejects_before_mutation() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    let projectile = bri_weapons::Projectile {
        id: 1,
        definition: "v20.projectile.gunprojectile".into(),
        source: bri_weapons::ActorId(1),
        position: glam::Vec3::ZERO,
        velocity: glam::Vec3::NEG_Z,
        scale: 1.,
        age: 1,
        bounced: false,
        stuck: false,
        origin: glam::Vec3::ZERO,
        was_thrown: false,
    };
    let weapons = bri_sim::session::WeaponView {
        projectiles: vec![projectile.clone(), projectile],
        ..Default::default()
    };
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: Some(weapons),
        tools: None,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: Default::default(),
        names: None,
        avatars: None,
        palette: None,
        chat: vec![],
        cues: vec![],
        dropped_cues: 0,
    };
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.cursor, 0);
    delta.weapons.as_mut().unwrap().projectiles.pop();
    delta.cues.push(bri_sim::presentation::Cue {
        id: 1,
        tick: 11,
        position: [0.; 3],
        kind: bri_sim::presentation::CueKind::WeaponSound {
            profile: "x".repeat(129),
        },
    });
    assert!(replica.update(delta.clone()).is_err());
    assert!(replica.weapons.projectiles.is_empty());
    delta.cues.clear();
    replica.update(delta).unwrap();
    assert_eq!(replica.weapons.projectiles.len(), 1);
}

#[test]
fn reliable_cues_do_not_replay_before_join_or_duplicate_and_reject_unreported_loss() {
    use bri_sim::presentation::{Cue, CueKind};
    let mut checkpoint = checkpoint();
    checkpoint.cue_cursor = 40;
    let mut replica = Replica::new(checkpoint).unwrap();
    let cue = |id| Cue {
        id,
        tick: 11,
        kind: CueKind::Jump,
        position: [1., 2., 3.],
    };
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: None,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: Default::default(),
        names: None,
        avatars: None,
        palette: None,
        chat: vec![],
        cues: vec![cue(39), cue(40), cue(41)],
        dropped_cues: 0,
    };
    replica.update(delta.clone()).unwrap();
    assert_eq!(replica.take_cues(), vec![cue(41)]);
    delta.base = 1;
    delta.cursor = 2;
    replica.update(delta.clone()).unwrap();
    assert!(replica.take_cues().is_empty());
    delta.base = 2;
    delta.cursor = 3;
    delta.cues = vec![cue(43)];
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.cursor, 2);
    delta.dropped_cues = 1;
    replica.update(delta.clone()).unwrap();
    assert_eq!(replica.take_cues(), vec![cue(43)]);
    delta.base = 3;
    delta.cursor = 4;
    delta.cues = vec![cue(45), cue(44)];
    assert!(replica.update(delta).is_err());
    assert_eq!(replica.cue_cursor, 43);
}
#[test]
fn gaps_and_invalid_changes_are_rejected_before_mutation() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    let brick = Brick::new(ContentRef::Resolved("brick".into()), [0.0; 3], 1);
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: None,
        cues: vec![],
        dropped_cues: 0,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: [(1, Some(brick.clone()))].into(),
        names: None,
        avatars: None,
        palette: None,
        chat: vec![],
    };
    let before = replica.world.clone();
    delta.base = 7;
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.world, before);
    delta.base = 0;
    let mut invalid = brick;
    invalid.color = 3;
    delta.bricks.insert(2, Some(invalid));
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.world, before);
    delta.bricks.remove(&2);
    replica.update(delta.clone()).unwrap();
    assert!(replica.update(delta).is_err());
    assert_eq!(replica.world.bricks.len(), 1);
    replica
        .update(Delta {
            vitals: None,
            minigames: None,
        vehicles: None,
            weapons: None,
            tools: None,
            cues: vec![],
            dropped_cues: 0,
            base: 1,
            cursor: 2,
            tick: 12,
            bricks: [(1, None)].into(),
            names: None,
            avatars: None,
            palette: None,
            chat: vec![],
        })
        .unwrap();
    assert!(replica.world.bricks.is_empty());
}

#[test]
fn invalid_avatar_delta_cannot_partially_change_world_or_peers() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    let avatar = bri_content::avatar::Appearance {
        parts: Default::default(),
        colors: Default::default(),
        face: "face".into(),
        decal: "decal".into(),
    };
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: None,
        cues: vec![],
        dropped_cues: 0,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: [(
            1,
            Some(Brick::new(
                ContentRef::Resolved("brick".into()),
                [0.0; 3],
                1,
            )),
        )]
        .into(),
        names: None,
        avatars: Some([(99, avatar.clone())].into()),
        palette: None,
        chat: vec![],
    };
    assert!(replica.update(delta.clone()).is_err());
    assert!(replica.world.bricks.is_empty());
    assert_eq!(replica.cursor, 0);
    let mut invalid = avatar;
    invalid.face = "x".repeat(257);
    delta.avatars = Some([(1, invalid)].into());
    assert!(replica.update(delta).is_err());
    assert!(replica.world.bricks.is_empty());
    assert!(replica.avatars.is_empty());
}

#[test]
fn palette_extension_and_new_bricks_commit_together_or_reject_together() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    let before = replica.world.clone();
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.0; 3], 1);
    brick.color = 1;
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: None,
        cues: vec![],
        dropped_cues: 0,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: [(1, Some(brick))].into(),
        names: None,
        avatars: None,
        palette: Some(vec![[0.0; 4], [0.2, 0.4, 0.6, 1.0]]),
        chat: vec![],
    };
    assert!(
        replica.update(delta.clone()).is_err(),
        "Existing palette cannot be recolored"
    );
    assert_eq!(replica.world, before);
    delta.palette.as_mut().unwrap()[0] = [1.0; 4];
    delta.bricks.get_mut(&1).unwrap().as_mut().unwrap().color = 2;
    assert!(
        replica.update(delta.clone()).is_err(),
        "No partial palette extension on invalid brick"
    );
    assert_eq!(replica.world, before);
    delta.bricks.get_mut(&1).unwrap().as_mut().unwrap().color = 1;
    replica.update(delta).unwrap();
    assert_eq!(replica.world.palette[1], [0.2, 0.4, 0.6, 1.0]);
    assert_eq!(replica.world.bricks[&1].color, 1);
}
#[test]
fn reordered_poses_do_not_rewind_and_interpolation_crosses_yaw_seam() {
    let mut replica = Replica::new(checkpoint()).unwrap();
    replica.pose(pose(12, 0.0, 3.1)).unwrap();
    replica.pose(pose(24, 12.0, -3.1)).unwrap();
    replica.pose(pose(18, 999.0, 0.0)).unwrap();
    assert_eq!(replica.poses[&1].player.feet[0], 12.0);
    let midway = replica.interpolated(1, 18.0).unwrap();
    assert_eq!(midway.feet[0], 6.0);
    assert!((midway.yaw.abs() - std::f32::consts::PI).abs() < 0.001);
    assert_eq!(replica.interpolated(1, 10000.0).unwrap().feet[0], 12.0);
    let mut invalid = pose(25, 0.0, 0.0);
    invalid.player.feet[0] = f32::INFINITY;
    assert!(replica.pose(invalid).is_err());
}
#[test]
fn compressed_checkpoint_roundtrips_and_truncation_fails() {
    let data = codec::encode(&Message::Welcome {
        administrator: false,
        owner: 1,
        resume: ResumeToken([1; 32]),
        checkpoint: checkpoint(),
    })
    .unwrap();
    let decoded: Message = codec::decode(&data).unwrap();
    assert!(matches!(
        decoded,
        Message::Welcome {
            administrator: false,
            owner: 1,
            ..
        }
    ));
    for end in [0, 1, 4, data.len() / 2, data.len() - 1] {
        assert!(codec::decode::<Message>(&data[..end]).is_err());
    }
    assert!(codec::decode::<Message>(&vec![0; codec::MAX_FRAME + 1]).is_err());
}

#[test]
fn invalid_weapon_pose_cue_cannot_partially_commit_world() {
    use bri_sim::presentation::{Cue, CueKind};
    let mut replica = Replica::new(checkpoint()).unwrap();
    let cue = Cue {
        id: 1,
        tick: 11,
        position: [0.; 3],
        kind: CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(1)),
            definition: "gunFlashEmitter".into(),
            node: "muzzleNode".into(),
            seconds: 0.05,
            image: Some("v20.image.gunimage".into()),
            hand: Some(0),
            direction: Some([0., 0., -1.]),
            scale: 1.,
        },
    };
    let mut delta = Delta {
        vitals: None,
        minigames: None,
        vehicles: None,
        weapons: None,
        tools: None,
        cues: vec![cue],
        dropped_cues: 0,
        base: 0,
        cursor: 1,
        tick: 11,
        bricks: [(
            1,
            Some(Brick::new(ContentRef::Resolved("brick".into()), [0.; 3], 1)),
        )]
        .into(),
        names: None,
        avatars: None,
        palette: None,
        chat: vec![],
    };
    if let CueKind::WeaponEffect { hand, .. } = &mut delta.cues[0].kind {
        *hand = Some(2);
    }
    assert!(replica.update(delta.clone()).is_err());
    assert_eq!(replica.cursor, 0);
    assert_eq!(replica.cue_cursor, 0);
    assert!(replica.world.bricks.is_empty());
    if let CueKind::WeaponEffect { hand, .. } = &mut delta.cues[0].kind {
        *hand = Some(1);
    }
    let encoded = codec::encode(&Message::Update(delta)).unwrap();
    let Message::Update(delta) = codec::decode(&encoded).unwrap() else {
        panic!("Wrong message");
    };
    replica.update(delta).unwrap();
    assert_eq!(replica.world.bricks.len(), 1);
    assert!(matches!(
        &replica.take_cues()[0].kind,
        CueKind::WeaponEffect {
            hand: Some(1),
            direction: Some([0., 0., -1.]),
            ..
        }
    ));
}
