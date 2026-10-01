//! Each wire change is one file in `protocol-changes/`; the version counts them.
use std::path::Path;

#[test]
fn every_protocol_change_file_is_named_and_written() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("protocol-changes");
    for name in bri_net::protocol::PROTOCOL_CHANGES {
        let stem = name.strip_suffix(".md").unwrap();
        assert!(
            !stem.is_empty()
                && stem
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{name}: name protocol change files with lowercase words joined by '-'"
        );
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        assert!(
            !text.trim().is_empty(),
            "{name}: say which messages or fields changed"
        );
    }
}

#[test]
fn the_version_counts_the_change_files() {
    assert_eq!(
        bri_net::protocol::VERSION,
        69 + bri_net::protocol::PROTOCOL_CHANGES.len() as u32
    );
}
