//! Bounded 20 ms packet playout. Reorder before decoding: Opus state follows
//! playout order, including FEC from the following packet and bounded PLC.
use super::{EncodedFrame, FRAME_SAMPLES, MAX_BUFFERED_FRAMES, SAMPLE_RATE, newer};

pub(super) struct Jitter {
    decoder: Option<opus::Decoder>,
    packets: [Option<(u16, EncodedFrame)>; MAX_BUFFERED_FRAMES],
    expected: Option<u16>,
    played: bool,
    losses: usize,
    pub recovered: u64,
    pub concealed: u64,
}

impl Jitter {
    pub fn new() -> Self {
        Self {
            decoder: opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono).ok(),
            packets: [None; MAX_BUFFERED_FRAMES],
            expected: None,
            played: false,
            losses: 0,
            recovered: 0,
            concealed: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.packets.iter().flatten().count()
    }

    pub fn push(&mut self, sequence: u16, packet: &[u8]) {
        if let Some(expected) = self.expected {
            if newer(sequence, expected) {
                // Before playout starts, permit an earlier reordered first
                // packet inside the window. Never replay an already played one.
                if self.played || expected.wrapping_sub(sequence) >= MAX_BUFFERED_FRAMES as u16 {
                    return;
                }
                self.expected = Some(sequence);
            } else if sequence.wrapping_sub(expected) >= MAX_BUFFERED_FRAMES as u16 {
                // Catch up after a pause or overflow; stale speech must never
                // grow into a seconds-long delay. Reset only codec history.
                self.packets.fill(None);
                if let Some(decoder) = &mut self.decoder {
                    let _ = decoder.reset_state();
                }
                self.expected = Some(sequence);
                self.losses = 0;
            }
        } else {
            self.expected = Some(sequence);
        }
        if self
            .packets
            .iter()
            .flatten()
            .any(|(held, _)| *held == sequence)
        {
            return;
        }
        if let Some(slot) = self.packets.iter_mut().find(|slot| slot.is_none()) {
            *slot = EncodedFrame::from_bytes(packet).map(|packet| (sequence, packet));
        }
    }

    pub fn pop(&mut self) -> Option<[f32; FRAME_SAMPLES]> {
        let sequence = self.expected?;
        let decoder = self.decoder.as_mut()?;
        let mut frame = [0.0; FRAME_SAMPLES];
        let index = self
            .packets
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|(held, _)| *held == sequence));
        if let Some(index) = index {
            let (_, packet) = self.packets[index].take()?;
            let packet = packet.as_bytes();
            if packet[0] == 1 {
                decoder.reset_state().ok()?;
            }
            decoder.decode_float(&packet[1..], &mut frame, false).ok()?;
            self.losses = 0;
        } else {
            // Two missing frames (40 ms) is the concealment limit when a
            // burst ends. Do not synthesize speech or keep an idle actor alive.
            if self.losses >= 2 && self.packets.iter().all(Option::is_none) {
                return None;
            }
            let following = self
                .packets
                .iter()
                .flatten()
                .find(|(held, packet)| {
                    *held == sequence.wrapping_add(1) && packet.as_bytes()[0] == 0
                })
                .map(|(_, packet)| &packet.as_bytes()[1..]);
            decoder
                .decode_float(following.unwrap_or(&[]), &mut frame, following.is_some())
                .ok()?;
            self.losses += 1;
            if following.is_some() {
                self.recovered += 1;
            } else {
                self.concealed += 1;
            }
        }
        self.played = true;
        self.expected = Some(sequence.wrapping_add(1));
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::Codec;

    #[test]
    fn reorders_wraps_deduplicates_and_conceals_without_unbounded_audio() {
        let mut packet = Codec::encode(&[1000; FRAME_SAMPLES]);
        packet[0] = 0;
        let mut jitter = Jitter::new();
        jitter.push(0, &packet);
        jitter.push(u16::MAX, &packet);
        jitter.push(0, &packet);
        assert_eq!(jitter.len(), 2);
        assert!(jitter.pop().is_some()); // 65535
        assert!(jitter.pop().is_some()); // 0
        jitter.push(0, &packet); // too late
        assert_eq!(jitter.len(), 0);
        jitter.push(2, &packet); // frame 1 lost, recover from 2
        assert!(jitter.pop().is_some());
        assert_eq!(jitter.recovered, 1);
        assert!(jitter.pop().is_some());
        assert!(jitter.pop().is_some());
        assert!(jitter.pop().is_some());
        assert!(jitter.pop().is_none());
        assert_eq!(jitter.concealed, 2);
        for sequence in 3..1000 {
            jitter.push(sequence, &packet);
            assert!(jitter.len() <= MAX_BUFFERED_FRAMES);
        }
        jitter.push(1000, &packet);
        assert!(jitter.pop().is_some(), "recovers after a long gap");
    }
}
