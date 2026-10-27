//! Proximity voice on the wire.
//!
//! Voice travels as QUIC datagrams beside the movement and state datagrams.
//! Every voice datagram starts with [`MARKER`], the one byte MessagePack never
//! uses, so it can never be mistaken for a [`crate::protocol::Movement`] or a
//! state datagram (both MessagePack) and the readers tell them apart without
//! decoding. The payload is opaque here: the audio crate's frames.
//!
//! Upstream (client to host): marker, [`UPSTREAM`], sequence (u16), payload.
//! Downstream (host to client): marker, [`DOWNSTREAM`], sequence (u16), the
//! speaker's owner (u64), the speaker's position (three f32), payload. All
//! little endian. The host sends a listener only the voices of players within
//! [`RANGE`] of them, so a voice never reaches a player who could not hear it,
//! and the speaker's place on the map is stamped by the host, never believed
//! from the client.
use anyhow::{Result, ensure};
use bri_world::OwnerId;
use std::sync::{Arc, Mutex, mpsc};

/// The first byte of every voice datagram.
pub const MARKER: u8 = 0xC1;
/// How far a voice carries, in world units. Beyond it the host sends nothing.
pub const RANGE: f32 = 48.0;
/// Largest voice payload the host relays. An encoded frame is 163 bytes.
pub const MAX_PAYLOAD: usize = 256;
/// Voice datagrams a client may send per second, sustained (50 a second is
/// 20 ms frames), with room for a burst after a stall.
pub const UPSTREAM_RATE: f64 = 75.0;
pub const UPSTREAM_BURST: f64 = 25.0;
const UPSTREAM: u8 = 1;
const DOWNSTREAM: u8 = 2;
const UPSTREAM_ACTIVITY: u8 = 3;
const DOWNSTREAM_ACTIVITY: u8 = 4;
const ACTIVITY_UPSTREAM_LEN: usize = 3;
const ACTIVITY_DOWNSTREAM_LEN: usize = 11;
const UPSTREAM_HEADER: usize = 4;
const DOWNSTREAM_HEADER: usize = 4 + 8 + 12;
/// Received voice frames waiting for the game; the newest are dropped when
/// it falls behind.
const INBOX: usize = 256;

/// Whether `bytes` is a voice datagram.
pub fn is_voice(bytes: &[u8]) -> bool {
    bytes.first() == Some(&MARKER)
}

/// A voice frame as a client sends it.
pub fn encode_upstream(sequence: u16, payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !payload.is_empty() && payload.len() <= MAX_PAYLOAD,
        "A voice frame is 1 to {MAX_PAYLOAD} bytes"
    );
    let mut out = Vec::with_capacity(UPSTREAM_HEADER + payload.len());
    out.extend_from_slice(&[MARKER, UPSTREAM]);
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// The sequence and payload of a client's voice datagram, or None when it
/// is not a well-formed one.
pub fn decode_upstream(bytes: &[u8]) -> Option<(u16, &[u8])> {
    if bytes.len() <= UPSTREAM_HEADER
        || bytes.len() > UPSTREAM_HEADER + MAX_PAYLOAD
        || bytes[0] != MARKER
        || bytes[1] != UPSTREAM
    {
        return None;
    }
    Some((
        u16::from_le_bytes([bytes[2], bytes[3]]),
        &bytes[UPSTREAM_HEADER..],
    ))
}

/// A voice frame as the host relays it to a listener.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub speaker: OwnerId,
    pub position: [f32; 3],
    pub sequence: u16,
    pub payload: Vec<u8>,
}

pub fn encode_downstream(
    speaker: OwnerId,
    position: [f32; 3],
    sequence: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(DOWNSTREAM_HEADER + payload.len());
    out.extend_from_slice(&[MARKER, DOWNSTREAM]);
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&speaker.to_le_bytes());
    for axis in position {
        out.extend_from_slice(&axis.to_le_bytes());
    }
    out.extend_from_slice(payload);
    out
}

pub fn decode_downstream(bytes: &[u8]) -> Option<Heard> {
    if bytes.len() <= DOWNSTREAM_HEADER
        || bytes.len() > DOWNSTREAM_HEADER + MAX_PAYLOAD
        || bytes[0] != MARKER
        || bytes[1] != DOWNSTREAM
    {
        return None;
    }
    let sequence = u16::from_le_bytes([bytes[2], bytes[3]]);
    let speaker = u64::from_le_bytes(bytes[4..12].try_into().ok()?);
    let mut position = [0.0f32; 3];
    for (i, axis) in position.iter_mut().enumerate() {
        let at = 12 + 4 * i;
        *axis = f32::from_le_bytes(bytes[at..at + 4].try_into().ok()?);
    }
    if !position.iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(Heard {
        speaker,
        position,
        sequence,
        payload: bytes[DOWNSTREAM_HEADER..].to_vec(),
    })
}

/// A small voice-state packet uses the same datagram marker as audio, but is
/// deliberately separate from audio frames. A true state is periodically
/// refreshed; receivers expire it if refreshes stop, so disconnects cannot
/// leave a speaker stuck in a talking pose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceActivity {
    pub speaker: OwnerId,
    pub active: bool,
}

/// Client-to-host speech activity (marker, opcode, active byte).
pub fn encode_upstream_activity(active: bool) -> Vec<u8> {
    vec![MARKER, UPSTREAM_ACTIVITY, u8::from(active)]
}

pub fn decode_upstream_activity(bytes: &[u8]) -> Option<bool> {
    if bytes.len() != ACTIVITY_UPSTREAM_LEN || bytes[0] != MARKER || bytes[1] != UPSTREAM_ACTIVITY {
        return None;
    }
    match bytes[2] {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Host-to-client speech activity (marker, opcode, owner u64, active byte).
pub fn encode_downstream_activity(speaker: OwnerId, active: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(ACTIVITY_DOWNSTREAM_LEN);
    out.extend_from_slice(&[MARKER, DOWNSTREAM_ACTIVITY]);
    out.extend_from_slice(&speaker.to_le_bytes());
    out.push(u8::from(active));
    out
}

pub fn decode_downstream_activity(bytes: &[u8]) -> Option<VoiceActivity> {
    if bytes.len() != ACTIVITY_DOWNSTREAM_LEN
        || bytes[0] != MARKER
        || bytes[1] != DOWNSTREAM_ACTIVITY
    {
        return None;
    }
    let speaker = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
    let active = match bytes[10] {
        0 => false,
        1 => true,
        _ => return None,
    };
    Some(VoiceActivity { speaker, active })
}

/// Whether a voice from `speaker` carries to `listener`.
pub fn audible(speaker: [f32; 3], listener: [f32; 3], range: f32) -> bool {
    let d: f32 = (0..3).map(|i| (speaker[i] - listener[i]).powi(2)).sum();
    d.is_finite() && d <= range * range
}

/// The sending end the transport task keeps for the datagram reader.
pub(crate) type Inbox = mpsc::SyncSender<Heard>;
pub(crate) type ActivityInbox = mpsc::SyncSender<VoiceActivity>;

pub(crate) fn inbox() -> (Inbox, mpsc::Receiver<Heard>) {
    mpsc::sync_channel(INBOX)
}

pub(crate) fn activity_inbox() -> (ActivityInbox, mpsc::Receiver<VoiceActivity>) {
    mpsc::sync_channel(64)
}

/// A client's voice connection: send frames to the host, take the frames
/// the host relayed. Cheap to clone, usable from any thread, and it never
/// waits on the network.
#[derive(Clone)]
pub struct VoiceLink {
    connection: quinn::Connection,
    heard: Arc<Mutex<mpsc::Receiver<Heard>>>,
    activity: Arc<Mutex<mpsc::Receiver<VoiceActivity>>>,
}

impl VoiceLink {
    pub(crate) fn new(
        connection: quinn::Connection,
        heard: mpsc::Receiver<Heard>,
        activity: mpsc::Receiver<VoiceActivity>,
    ) -> Self {
        Self {
            connection,
            heard: Arc::new(Mutex::new(heard)),
            activity: Arc::new(Mutex::new(activity)),
        }
    }
    /// Send one voice frame. Voice is never retransmitted: a frame the
    /// network drops is a 20 ms gap.
    pub fn send(&self, sequence: u16, payload: &[u8]) -> Result<()> {
        let bytes = encode_upstream(sequence, payload)?;
        self.connection.send_datagram(bytes.into())?;
        Ok(())
    }
    /// Send the local speech-activity edge/keepalive. This is control state,
    /// not a claim that the microphone is enabled; callers must gate it with
    /// voice activity detection first.
    pub fn send_activity(&self, active: bool) -> Result<()> {
        self.connection
            .send_datagram(encode_upstream_activity(active).into())?;
        Ok(())
    }
    /// Move the frames received so far into `out`, oldest first.
    pub fn take_heard(&self, out: &mut Vec<Heard>) {
        let receiver = self.heard.lock().unwrap_or_else(|e| e.into_inner());
        out.extend(receiver.try_iter());
    }
    /// Move remote activity states received so far into `out`, oldest first.
    pub fn take_activity(&self, out: &mut Vec<VoiceActivity>) {
        let receiver = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        out.extend(receiver.try_iter());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_is_never_valid_messagepack() {
        // 0xC1 is the one byte MessagePack reserves as never used.
        assert!(rmp_serde::from_slice::<serde_json::Value>(&[MARKER]).is_err());
        assert!(rmp_serde::from_slice::<serde_json::Value>(&[MARKER, 1, 0, 0, 7]).is_err());
        assert!(is_voice(&[MARKER, 1]));
        assert!(!is_voice(&[0x95]));
        assert!(!is_voice(&[]));
    }

    #[test]
    fn upstream_frames_round_trip_and_bad_ones_are_refused() {
        let bytes = encode_upstream(513, &[9; 163]).unwrap();
        assert_eq!(decode_upstream(&bytes), Some((513, &[9u8; 163][..])));
        assert!(encode_upstream(0, &[]).is_err());
        assert!(encode_upstream(0, &[0; MAX_PAYLOAD + 1]).is_err());
        assert_eq!(decode_upstream(&[MARKER, UPSTREAM, 0, 0]), None);
        assert_eq!(decode_upstream(&[MARKER, DOWNSTREAM, 0, 0, 1]), None);
        assert_eq!(decode_upstream(&[0x95, UPSTREAM, 0, 0, 1]), None);
        let mut long = bytes.clone();
        long.resize(UPSTREAM_HEADER + MAX_PAYLOAD + 1, 0);
        assert_eq!(decode_upstream(&long), None);
    }

    #[test]
    fn downstream_frames_carry_who_and_where() {
        let bytes = encode_downstream(u64::MAX - 3, [1.5, -2.0, 300.25], 65_535, &[7; 163]);
        let heard = decode_downstream(&bytes).unwrap();
        assert_eq!(heard.speaker, u64::MAX - 3);
        assert_eq!(heard.position, [1.5, -2.0, 300.25]);
        assert_eq!(heard.sequence, 65_535);
        assert_eq!(heard.payload, vec![7; 163]);
        assert!(bytes.len() < crate::protocol::MAX_DATAGRAM);
        // A position that is not a number is never played.
        let bad = encode_downstream(1, [f32::NAN, 0.0, 0.0], 0, &[1]);
        assert_eq!(decode_downstream(&bad), None);
        assert_eq!(decode_downstream(&bytes[..DOWNSTREAM_HEADER]), None);
        assert_eq!(decode_downstream(&encode_upstream(0, &[1]).unwrap()), None);
    }

    #[test]
    fn activity_packets_round_trip_and_reject_bad_shapes() {
        assert_eq!(
            decode_upstream_activity(&encode_upstream_activity(true)),
            Some(true)
        );
        assert_eq!(
            decode_upstream_activity(&encode_upstream_activity(false)),
            Some(false)
        );
        assert_eq!(
            decode_upstream_activity(&[MARKER, UPSTREAM_ACTIVITY, 2]),
            None
        );
        assert_eq!(decode_upstream_activity(&[MARKER, UPSTREAM_ACTIVITY]), None);
        assert_eq!(decode_upstream_activity(&[MARKER, UPSTREAM, 0, 0]), None);

        for active in [false, true] {
            let bytes = encode_downstream_activity(u64::MAX - 9, active);
            assert_eq!(bytes.len(), ACTIVITY_DOWNSTREAM_LEN);
            assert_eq!(
                decode_downstream_activity(&bytes),
                Some(VoiceActivity {
                    speaker: u64::MAX - 9,
                    active,
                })
            );
        }
        let mut bad = encode_downstream_activity(4, true);
        bad[10] = 2;
        assert_eq!(decode_downstream_activity(&bad), None);
        assert_eq!(
            decode_downstream_activity(&[MARKER, DOWNSTREAM_ACTIVITY]),
            None
        );
    }

    #[test]
    fn a_voice_carries_exactly_to_its_range() {
        let at = [0.0, 0.0, 0.0];
        assert!(audible(at, [RANGE, 0.0, 0.0], RANGE));
        assert!(!audible(at, [RANGE + 0.1, 0.0, 0.0], RANGE));
        assert!(audible(at, [20.0, 20.0, 20.0], RANGE));
        assert!(!audible(at, [30.0, 30.0, 30.0], RANGE));
        assert!(!audible(at, [f32::NAN, 0.0, 0.0], RANGE));
    }
}
