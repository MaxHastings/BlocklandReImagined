use crate::World;
use anyhow::{Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
pub const MAX_SAVE_BYTES: u64 = 512 * 1024 * 1024;
pub fn decode(bytes: &[u8]) -> Result<World> {
    ensure!(
        bytes.len() as u64 <= MAX_SAVE_BYTES,
        "Oversized native world"
    );
    let world: World = serde_json::from_slice(bytes)?;
    world.validate()?;
    Ok(world)
}
pub fn load(path: &Path) -> Result<World> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_SAVE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    decode(&bytes)
}
/// Dedicated startup accepts both running checkpoints and client build saves.
/// A build save never resumes an event queue from its originating session.
pub fn load_startup(path: &Path) -> Result<World> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_SAVE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    match decode(&bytes) {
        Ok(world) => Ok(world),
        Err(_) => Ok(crate::build::decode(&bytes)?.world),
    }
}
/// Flush a staging file, then publish a new revision with an atomic no-clobber
/// hard link. The destination is never visible with partial contents. Requires
/// a filesystem with hard-link support; failure leaves existing saves intact.
pub fn save_new(path: &Path, world: &World) -> Result<()> {
    world.validate()?;
    let bytes = serde_json::to_vec(world)?;
    ensure!(
        bytes.len() as u64 <= MAX_SAVE_BYTES,
        "Oversized native world"
    );
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let (staging, mut file) = loop {
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let staging = parent.join(format!(".bri-save-{}-{serial}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => break (staging, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    };
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    let published = result.and_then(|_| std::fs::hard_link(&staging, path).map_err(Into::into));
    let _ = std::fs::remove_file(&staging);
    published
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Brick, ContentRef, ItemSpawn, SourceRecord};
    #[test]
    fn item_spawn_roundtrip_and_old_schema_defaults_are_compatible() {
        let mut world = World::new("items".into(), "map/test".into(), vec![[1.0; 4]]);
        let mut brick = Brick::new(ContentRef::Resolved("brick/test".into()), [0.0; 3], 1);
        brick.item_spawn = ItemSpawn {
            item: Some(ContentRef::Resolved("v20.weapon.gunitem".into())),
            position: 5,
            direction: 3,
            respawn_ms: 12001,
        };
        brick.source_records.push(SourceRecord {
            line: 1,
            text: "+-CUSTOM preserved".into(),
            diagnostic: Some("Unsupported".into()),
        });
        world.bricks.insert(1, brick);
        world.next_brick_id = 2;
        let bytes = serde_json::to_vec(&world).unwrap();
        assert_eq!(decode(&bytes).unwrap(), world);
        assert_eq!(world.bricks[&1].item_spawn.respawn_ticks(), 1441);
        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        legacy["bricks"]["1"]
            .as_object_mut()
            .unwrap()
            .remove("item_spawn");
        let old = decode(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(old.bricks[&1].item_spawn, ItemSpawn::default());
        assert_eq!(
            old.bricks[&1].source_records,
            world.bricks[&1].source_records
        );
        for invalid in [
            serde_json::json!({"item":null,"position":6,"direction":2,"respawn_ms":4000}),
            serde_json::json!({"item":null,"position":0,"direction":0,"respawn_ms":4000}),
            serde_json::json!({"item":null,"position":0,"direction":2,"respawn_ms":999}),
            serde_json::json!({"item":null,"position":0,"direction":2,"respawn_ms":300001}),
        ] {
            legacy["bricks"]["1"]["item_spawn"] = invalid;
            assert!(decode(&serde_json::to_vec(&legacy).unwrap()).is_err());
        }
    }
    #[test]
    fn save_publish_never_overwrites_an_existing_revision() {
        let directory = std::env::temp_dir().join(format!(
            "bri-save-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("revision.world.json");
        let mut world = World::new("original".into(), "map/test".into(), vec![[1.0; 4]]);
        save_new(&path, &world).unwrap();
        let original = load(&path).unwrap();
        world.name = "replacement".into();
        assert!(save_new(&path, &world).is_err());
        assert_eq!(load(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}
