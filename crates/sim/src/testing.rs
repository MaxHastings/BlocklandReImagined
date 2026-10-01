//! Made-up brick definitions for tests and tools that run without converted
//! v20 content: a plate, a brick, a tall column, a baseplate, a water brick,
//! an indestructible stone and a vehicle spawn brick. Each is a plain box
//! with studs on top; every size is invented, none comes from Blockland's
//! own bricks.
//!
//! Tests read a brick's size back from its [`Definition`] rather than
//! repeating it.
use crate::definitions::{Definition, Definitions, Special};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};

/// One stud by one, a plate high.
pub const PLATE: &str = "test/brick/plate";
/// Four studs by two, a brick (three plates) high.
pub const BRICK: &str = "test/brick/brick2x4";
/// One stud by one, five bricks high.
pub const TALL: &str = "test/brick/tall";
/// Sixteen studs square, a plate high.
pub const BASEPLATE: &str = "test/brick/baseplate";
/// A swimmable four by four brick of water.
pub const WATER: &str = "test/brick/water";
/// A two by two brick explosions cannot break.
pub const STONE: &str = "test/brick/stone";
/// A plate that holds a vehicle (the wrench's Vehicle list): twelve studs
/// square, wider and longer than any synthetic vehicle parked on it, and
/// indestructible, as spawn bricks keep explosions off.
pub const VEHICLE_SPAWN: &str = "test/brick/vehicle-spawn";

/// A box brick `studs` wide and long and `plates` high, studded on top and
/// socketed below.
pub fn definition(
    id: &str,
    studs: [u8; 2],
    plates: u16,
    special: Special,
    indestructible: bool,
) -> Definition {
    let size = [
        f32::from(studs[0]) * 0.5,
        f32::from(plates) * 0.2,
        f32::from(studs[1]) * 0.5,
    ];
    let collision = CollisionBody {
        id: id.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size,
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .expect("a box collides")
        .build()
        .shared_shape()
        .clone();
    Definition {
        mesh: Mesh {
            schema_version: 1,
            id: id.into(),
            footprint_studs: [studs[0].into(), studs[1].into()],
            height_plates: plates.into(),
            // Per stud row, top plate first: studs on top, sockets below.
            attachment_rows: (0..studs[1])
                .flat_map(|_| {
                    (0..plates).map(move |y| {
                        let cell = match (y, plates) {
                            (_, 1) => "b",
                            (0, _) => "u",
                            (y, h) if y == h - 1 => "d",
                            _ => "x",
                        };
                        cell.repeat(studs[0].into())
                    })
                })
                .collect(),
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        },
        collision,
        shape,
        indestructible,
        special,
        reflection: None,
        link: None,
        glass: [0.0; 4],
    }
}

/// Every brick here, keyed by id.
pub fn definitions() -> Definitions {
    let entries = [
        definition(PLATE, [1, 1], 1, Special::None, false),
        definition(BRICK, [4, 2], 3, Special::None, false),
        definition(TALL, [1, 1], 15, Special::None, false),
        definition(BASEPLATE, [16, 16], 1, Special::None, false),
        definition(WATER, [4, 4], 3, Special::Water, false),
        definition(STONE, [2, 2], 3, Special::None, true),
        definition(VEHICLE_SPAWN, [12, 12], 1, Special::None, true),
    ];
    Definitions {
        entries: entries
            .into_iter()
            .map(|d| (d.mesh.id.clone(), d))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_synthetic_brick_is_a_box_of_its_footprint() {
        let definitions = super::definitions();
        assert_eq!(definitions.entries.len(), 7);
        for (id, d) in &definitions.entries {
            assert_eq!(&d.mesh.id, id);
            let rows = d.mesh.footprint_studs[1] as usize * d.mesh.height_plates as usize;
            assert_eq!(d.mesh.attachment_rows.len(), rows, "{id}");
        }
        assert!(definitions.entries[super::VEHICLE_SPAWN].indestructible);
    }
}
