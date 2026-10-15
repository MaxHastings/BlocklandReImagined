use bri_audio::decode::{StreamDecoder, decode_all};
use std::sync::Arc;

fn wave(extra: usize, short_riff: bool) -> Vec<u8> {
    let original = bri_audio::wav::encode_pcm16(22050, 1, &[0.25, -0.5, 0.75, 0.]);
    let mut bytes = original[..36].to_vec();
    bytes[16..20].copy_from_slice(&(16u32 + extra as u32).to_le_bytes());
    bytes.resize(bytes.len() + extra, 0);
    // Preserve an unknown odd-length metadata chunk, including its padding.
    bytes.extend_from_slice(b"JUNK\x03\0\0\0abc\0");
    bytes.extend_from_slice(&original[36..]);
    let size = bytes.len() as u32 - 8 - if short_riff { extra as u32 } else { 0 };
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

#[test]
fn zero_pcm_extensions_preserve_samples_in_preload_stream_and_restart() {
    let expected = decode_all(&wave(0, false), Some("wav")).unwrap().0;
    for extra in [2, 4] {
        for short in [false, true] {
            let bytes = wave(extra, short);
            let unchanged = bytes.clone();
            let (clip, report) = decode_all(&bytes, Some("wav")).unwrap();
            assert_eq!(clip, expected);
            assert_eq!(report.corrupt_packets, 0);
            let mut stream =
                StreamDecoder::open(Arc::from(bytes.clone()), Some("wav"), 22050, 1).unwrap();
            for pass in 0..2 {
                if pass > 0 {
                    stream.restart().unwrap();
                }
                let mut samples = Vec::new();
                while stream.next_chunk(&mut samples).unwrap() {}
                assert_eq!(&*expected.samples, samples);
            }
            assert_eq!(bytes, unchanged);
        }
    }
}

#[test]
fn meaningful_or_truncated_extensions_are_not_reinterpreted_as_plain_pcm() {
    let mut meaningful = wave(4, false);
    meaningful[36] = 1;
    assert!(decode_all(&meaningful, Some("wav")).is_err());
    let mut oversized = wave(4, false);
    oversized[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_all(&oversized, Some("wav")).is_err());
    for length in 0..40 {
        assert!(decode_all(&wave(4, false)[..length], Some("wav")).is_err());
    }
}
