//! Whatever arrives off the network: random bytes, and real messages with
//! bytes flipped, cut short or extended. Decoding may refuse; it must never
//! panic, and whatever it accepts must go into a client's replica without a
//! panic and without NaN reaching what the client draws.
use bri_chaos::{fixture, scan};
use bri_net::{
    codec,
    protocol::{Checkpoint, Datagram, Hello, Message, Movement, Pose, Request, poses},
    replica::Replica,
};
use bri_sim::player::{MoveInput, PlayerState};
use proptest::prelude::*;

/// A real Welcome checkpoint, a pose and a vehicle-free update from a live
/// synthetic session with two players.
fn samples() -> (Checkpoint, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut fx = fixture::synthetic().unwrap();
    let a = fx
        .session
        .join("Alpha".into(), fx.spawn_points[0], true)
        .unwrap();
    fx.session
        .join("Bravo".into(), fx.spawn_points[1], false)
        .unwrap();
    for _ in 0..10 {
        fx.session.step().unwrap();
    }
    let (checkpoint, _) = Checkpoint::from_session(&fx.session, 1);
    let frames = vec![
        codec::encode(&Message::Welcome {
            owner: a,
            administrator: true,
            resume: bri_net::protocol::ResumeToken([7; 32]),
            checkpoint: checkpoint.clone(),
        })
        .unwrap(),
        codec::encode(&Message::Rejected("no".into())).unwrap(),
    ];
    let datagrams = poses(&fx.session)
        .into_iter()
        .map(|p| codec::encode_datagram(&Datagram::Pose(p)).unwrap())
        .chain([codec::encode_datagram(&Movement {
            version: bri_net::protocol::VERSION,
            newest: 5,
            inputs: vec![MoveInput::default(); 3],
            camera: None,
        })
        .unwrap()])
        .collect();
    (checkpoint, frames, datagrams)
}

thread_local! {
    static SAMPLES: (Checkpoint, Vec<Vec<u8>>, Vec<Vec<u8>>) = samples();
}

#[derive(Clone, Debug)]
enum Damage {
    Flip { at: usize, bits: u8 },
    Cut(usize),
    Extend(Vec<u8>),
    Splice { at: usize, bytes: Vec<u8> },
}

fn damage() -> impl Strategy<Value = Vec<Damage>> {
    proptest::collection::vec(
        prop_oneof![
            4 => (any::<usize>(), 1u8..=255).prop_map(|(at, bits)| Damage::Flip { at, bits }),
            1 => any::<usize>().prop_map(Damage::Cut),
            1 => proptest::collection::vec(any::<u8>(), 1..16).prop_map(Damage::Extend),
            2 => (any::<usize>(), proptest::collection::vec(prop_oneof![Just(0xcau8), Just(0x7f), Just(0xc0), Just(0xff), Just(0xdd), any::<u8>()], 1..9))
                .prop_map(|(at, bytes)| Damage::Splice { at, bytes }),
        ],
        1..6,
    )
}

fn apply(mut bytes: Vec<u8>, damage: &[Damage]) -> Vec<u8> {
    for d in damage {
        match d {
            Damage::Flip { at, bits } if !bytes.is_empty() => {
                let i = at % bytes.len();
                bytes[i] ^= bits;
            }
            Damage::Cut(at) if !bytes.is_empty() => bytes.truncate(at % bytes.len()),
            Damage::Extend(more) => bytes.extend_from_slice(more),
            Damage::Splice { at, bytes: new } if !bytes.is_empty() => {
                let i = at % bytes.len();
                for (j, b) in new.iter().enumerate() {
                    if let Some(slot) = bytes.get_mut(i + j) {
                        *slot = *b;
                    }
                }
            }
            _ => {}
        }
    }
    bytes
}

/// A decoded server message goes into a replica the way the client applies it.
fn receive(checkpoint: &Checkpoint, message: Message) -> Result<(), TestCaseError> {
    match message {
        Message::Welcome { checkpoint, .. } | Message::MapChanged(checkpoint) => {
            if let Ok(replica) = Replica::new(checkpoint) {
                drawn(&replica)?;
            }
        }
        Message::Update(delta) => {
            let mut replica = Replica::new(checkpoint.clone()).unwrap();
            if replica.update(delta).is_ok() {
                drawn(&replica)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// What a client draws from its replica holds no NaN.
fn drawn(replica: &Replica) -> Result<(), TestCaseError> {
    let found = scan::non_finite(
        &(
            &replica.poses,
            &replica.vehicle_poses,
            &replica.vitals,
            &replica.weapons,
            &replica.world,
        ),
        4,
    )
    .map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert!(
        found.is_empty(),
        "the replica accepted non-finite state: {found:?}"
    );
    for owner in replica.poses.keys() {
        if let Some(state) = replica.interpolated(*owner, replica.tick as f64 - 3.5) {
            let found = scan::non_finite(&state, 4).unwrap();
            prop_assert!(found.is_empty(), "interpolated pose: {found:?}");
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(1000, 0x7e7))]

    #[test]
    fn random_bytes_never_panic_a_decoder(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = codec::decode::<Message>(&bytes);
        let _ = codec::decode_datagram::<Datagram>(&bytes);
        let _ = codec::decode_datagram::<Movement>(&bytes);
        let _ = codec::decode_datagram::<Request>(&bytes);
        let _ = codec::decode_datagram::<Hello>(&bytes);
        let _ = codec::decode_datagram::<bri_sim::session::Command>(&bytes);
    }

    #[test]
    fn damaged_server_frames_decode_or_refuse(which in 0usize..2, damage in damage()) {
        SAMPLES.with(|(checkpoint, frames, _)| -> Result<(), TestCaseError> {
            // Damage the MessagePack inside the zstd frame, where it can
            // still decode into something.
            let raw = zstd::stream::decode_all(frames[which].as_slice()).unwrap();
            let raw = apply(raw, &damage);
            let frame = zstd::stream::encode_all(raw.as_slice(), 1).unwrap();
            if let Ok(message) = codec::decode::<Message>(&frame) {
                receive(checkpoint, message)?;
            }
            let _ = codec::decode::<Message>(&apply(frames[which].clone(), &damage));
            Ok(())
        })?;
    }

    #[test]
    fn damaged_datagrams_decode_or_refuse(which in 0usize..3, damage in damage()) {
        SAMPLES.with(|(checkpoint, _, datagrams)| -> Result<(), TestCaseError> {
            let bytes = apply(datagrams[which % datagrams.len()].clone(), &damage);
            if let Ok(Datagram::Pose(pose)) = codec::decode_datagram::<Datagram>(&bytes) {
                let mut replica = Replica::new(checkpoint.clone()).unwrap();
                if replica.pose(pose).is_ok() {
                    drawn(&replica)?;
                }
            }
            if let Ok(movement) = codec::decode_datagram::<Movement>(&bytes)
                && movement.validate().is_ok()
            {
                for (_, input) in movement.sequenced() {
                    let _ = input.validate();
                }
            }
            Ok(())
        })?;
    }

    /// Any pose floats the host could send: the replica takes finite ones
    /// and refuses the rest, whichever field holds the NaN.
    #[test]
    fn replicated_poses_are_finite_or_refused(tick in 1u64..1000, floats in proptest::collection::vec(prop_oneof![
        8 => -100.0f32..100.0, 1 => Just(f32::NAN), 1 => Just(f32::INFINITY)], 11)) {
        SAMPLES.with(|(checkpoint, _, _)| -> Result<(), TestCaseError> {
            let mut replica = Replica::new(checkpoint.clone()).unwrap();
            let owner = *replica.names.keys().next().unwrap();
            let mut player = PlayerState {
                owner,
                ..replica.poses[&owner].player.clone()
            };
            player.feet = [floats[0], floats[1], floats[2]];
            player.velocity = [floats[3], floats[4], floats[5]];
            player.yaw = floats[6];
            player.pitch = floats[7];
            player.head_yaw = floats[8];
            player.scale = floats[9];
            player.energy = floats[10];
            let base = replica.poses[&owner].tick;
            let _ = replica.pose(Pose { tick: base + tick, acknowledged_input: 0, player });
            drawn(&replica)
        })?;
    }
}
