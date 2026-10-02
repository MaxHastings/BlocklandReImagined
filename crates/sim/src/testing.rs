//! Made-up brick definitions for tests and tools that run without converted
//! v20 content: a plate, a brick, a tall column, a baseplate, water bricks,
//! an indestructible stone, a vehicle spawn brick, the special bricks
//! (checkpoint, teledoor, treasure chest, player spawn), a steep ramp and a
//! portal doorway. Each but the ramp and the portal is a plain box with
//! studs on top; every size is invented, none comes from Blockland's own
//! bricks.
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
/// A swimmable four by four brick of water: the stand-in carries the id of
/// a stock water brick, whose script makes the zone.
pub const WATER: &str = "v20/brick/brick8xwaterdata";
/// A two by two brick explosions cannot break.
pub const STONE: &str = "test/brick/stone";
/// A plate that holds a vehicle (the wrench's Vehicle list): twelve studs
/// square, wider and longer than any synthetic vehicle parked on it, and
/// indestructible, as spawn bricks keep explosions off.
pub const VEHICLE_SPAWN: &str = "test/brick/vehicle-spawn";
/// A four by four plate that sets the respawn point of whoever touches it.
pub const CHECKPOINT: &str = "test/brick/checkpoint";
/// A door-sized slab (four studs by one, twelve plates) that carries a
/// player who walks into it to its paired door.
pub const TELEDOOR: &str = "test/brick/teledoor";
/// A water brick taller than a player (eight studs square, thirty plates).
pub const DEEP_WATER: &str = "v20/brick/brick32xwaterdata";
/// The treasure chest, closed and open, and the player spawn brick: the
/// host names these by their v20 ids, so the stand-ins carry them.
pub const TREASURE_CHEST: &str = "v20/brick/bricktreasurechestdata";
pub const TREASURE_CHEST_OPEN: &str = "v20/brick/bricktreasurechestopendata";
pub const SPAWN_POINT: &str = "v20/brick/brickspawnpointdata";
/// A ramp too steep to stand on: two studs square and ten plates high, its
/// front half a slope from the bottom front edge up to the top's middle
/// (about 76 degrees), its back half solid. See [`ramp`].
pub const STEEP_RAMP: &str = "test/brick/steep-ramp";
/// A 1x4x5 doorway (2 wide, 3 tall, half a unit deep) opening north and
/// south through its middle, linked to another of one name: a portal like
/// the Portal Add-On's smallest. See [`portal`].
pub const PORTAL: &str = "test/brick/portal";

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

/// A ramp brick `studs` wide (x) and deep (z) and `plates` high whose slope
/// faces -z: the front half rises from the bottom front edge to the top at
/// the brick's middle, the back half is a solid block. One convex piece.
pub fn ramp(id: &str, studs: [u8; 2], plates: u16) -> Definition {
    let mut d = definition(id, studs, plates, Special::None, false);
    let [hx, hy, hz] = [
        f32::from(studs[0]) * 0.25,
        f32::from(plates) * 0.1,
        f32::from(studs[1]) * 0.25,
    ];
    let vertices = vec![
        [-hx, -hy, -hz],
        [hx, -hy, -hz],
        [hx, -hy, hz],
        [-hx, -hy, hz],
        [-hx, hy, 0.0],
        [hx, hy, 0.0],
        [hx, hy, hz],
        [-hx, hy, hz],
    ];
    // Faces as corner loops; each fans into triangles turned outward.
    let faces: [&[u32]; 6] = [
        &[0, 1, 2, 3],
        &[4, 5, 6, 7],
        &[3, 2, 6, 7],
        &[0, 1, 5, 4],
        &[0, 3, 7, 4],
        &[1, 2, 6, 5],
    ];
    let point = |i: u32| glam::Vec3::from(vertices[i as usize]);
    let centre = vertices
        .iter()
        .map(|v| glam::Vec3::from(*v))
        .sum::<glam::Vec3>()
        / vertices.len() as f32;
    let triangles = faces
        .iter()
        .flat_map(|f| (1..f.len() - 1).map(move |i| [f[0], f[i], f[i + 1]]))
        .map(|[a, b, c]| {
            let normal = (point(b) - point(a)).cross(point(c) - point(a));
            if normal.dot(point(a) - centre) < 0.0 {
                [a, c, b]
            } else {
                [a, b, c]
            }
        })
        .collect();
    d.collision = CollisionBody {
        id: id.into(),
        parts: vec![Part::Convex {
            label: "ramp".into(),
            vertices,
            triangles,
        }],
    };
    d.shape = bri_physics::content::collider(&d.collision)
        .expect("a ramp collides")
        .build()
        .shared_shape()
        .clone();
    d
}

/// A portal doorway ([`PORTAL`]) `size` studs wide, deep and plates high
/// (4x1x15 when `None`), stretched as the Portal Add-On's bigger ones are:
/// thin sides and top, a sill to step over, openings north and south.
pub fn portal(id: &str, size: Option<[u32; 3]>) -> Definition {
    use bri_content::brick::{Face, Frame, Link, Quad, Surface, Vertex};
    let door = Mesh {
        schema_version: 1,
        id: id.into(),
        footprint_studs: [4, 1],
        height_plates: 15,
        attachment_rows: vec!["bbbb".into(); 15],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        // Its top, for a shape that is whole.
        quads: vec![Quad {
            face: Face::Top,
            surface: Surface::Top,
            vertices: [[-1.0, 0.25], [1.0, 0.25], [1.0, -0.25], [-1.0, -0.25]].map(|[x, z]| {
                Vertex {
                    position: [x, 1.5, z],
                    normal: [0.0, 1.0, 0.0],
                    uv: [x + 1.0, z + 0.25],
                }
            }),
            colors: None,
        }],
    };
    let mesh = match size {
        Some(size) => door.stretched(id, size).expect("a doorway stretches"),
        None => door,
    };
    let link = Link {
        faces: vec![Face::North, Face::South],
        depth: 0.5,
        inset: 0.0,
        tint: [1.0; 3],
        idle: [0.5; 3],
        pass: true,
        frame: Frame {
            sides: 0.05,
            top: 0.05,
            bottom: 0.2,
        },
        name: "Portal".into(),
    };
    let collision = CollisionBody {
        id: id.into(),
        parts: link
            .frame_boxes(&mesh)
            .into_iter()
            .map(|b| Part::Box {
                center: b.center,
                size: b.size,
            })
            .collect(),
    };
    let shape = bri_physics::content::collider(&collision)
        .expect("a frame collides")
        .build()
        .shared_shape()
        .clone();
    Definition {
        mesh,
        collision,
        shape,
        indestructible: false,
        special: Special::None,
        reflection: None,
        link: Some(link),
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
        definition(CHECKPOINT, [4, 4], 1, Special::Checkpoint, false),
        definition(TELEDOOR, [4, 1], 12, Special::Teledoor, false),
        definition(DEEP_WATER, [8, 8], 30, Special::Water, false),
        definition(TREASURE_CHEST, [2, 2], 3, Special::TreasureChest, false),
        definition(
            TREASURE_CHEST_OPEN,
            [2, 2],
            3,
            Special::TreasureChestOpen,
            false,
        ),
        definition(SPAWN_POINT, [2, 2], 1, Special::SpawnPoint, false),
        ramp(STEEP_RAMP, [2, 2], 10),
        portal(PORTAL, None),
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
        assert_eq!(definitions.entries.len(), 15);
        for (id, d) in &definitions.entries {
            assert_eq!(&d.mesh.id, id);
            let rows = d.mesh.footprint_studs[1] as usize * d.mesh.height_plates as usize;
            assert_eq!(d.mesh.attachment_rows.len(), rows, "{id}");
        }
        assert!(definitions.entries[super::VEHICLE_SPAWN].indestructible);
    }
}
