//! A content-free weapons pack for tests that have no converted v20
//! content: one stand-in for every item role the runtime and the host
//! exercise, with made-up timings and numbers.
//!
//! Ids are `test:` ids, except where the engine itself names a definition
//! by its v20 id (the core tools, the spray cans and their paint, the akimbo
//! gun's left hand, the sports balls, the host's own projectiles): those use
//! the id the engine looks up, and are listed in [`ENGINE_IDS`]. Some
//! behaviours hang on a definition's `name` instead (`skiWeaponImage`,
//! `horseRayProjectile`, an image name containing `spear` or `keyimage`);
//! their stand-ins carry such a name. Nothing here is read from, or copied
//! out of, the original game's datablocks.
//!
//! The Bubble Blaster sample Add-On (`packages/samples/sample-bubble-blaster`)
//! is merged in as it ships.
use crate::*;
use std::collections::BTreeMap;

// --- Core tools: the host's building authority performs their `onFire`. ---
pub use crate::{HAMMER, PRINTER, WAND, WRENCH};
pub const HAMMER_IMAGE: &str = "v20.image.hammerimage";
pub const WRENCH_IMAGE: &str = "v20.image.wrenchimage";
pub const PRINTER_IMAGE: &str = "v20.image.printgunimage";
pub const WAND_IMAGE: &str = "v20.image.wandimage";
/// `serverCmdMagicWand`'s image; no inventory item holds it.
pub const ADMIN_WAND_IMAGE: &str = "v20.image.adminwandimage";
/// The ghost brick's images, which the tutorial checks are in hand. A
/// click swings them ([`BRICK_FIRE_SEQUENCE`], trailing
/// [`BRICK_TRAIL_EMITTER`]) and throws [`BRICK_DEPLOY_PROJECTILE`].
pub const BRICK_IMAGE: &str = "v20.image.brickimage";
pub const HORSE_BRICK_IMAGE: &str = "v20.image.horsebrickimage";
/// The brick images' Fire state: its image sequence and its emitter.
pub const BRICK_FIRE_SEQUENCE: &str = "testBrickThrow";
pub const BRICK_TRAIL_EMITTER: &str = "testBrickTrailEmitter";

// --- Guns. ---
/// Semi-automatic: one shot per click, ejecting a casing. Its Ready state
/// goes to `NoAmmo` when the host empties it.
pub const GUN_ITEM: &str = "v20.weapon.gunitem";
pub const GUN_IMAGE: &str = "v20.image.gunimage";
pub const GUN_PROJECTILE: &str = "test:projectile/gun";
/// The gun image's `casing`, a `DebrisData` in [`Pack::definitions`].
pub const GUN_CASING: &str = "gunShellDebris";
/// Two guns: the right fires on press, the left on release (`onFireAkimbo`).
pub const AKIMBO_ITEM: &str = "test:weapon/akimbo";
pub const AKIMBO_IMAGE: &str = "v20.image.akimbogunimage";
pub const LEFT_GUN_IMAGE: &str = "v20.image.lefthandedgunimage";
/// Several spread pellets per shot, with recoil.
pub const SHOTGUN_ITEM: &str = "test:weapon/shotgun";
pub const SHOTGUN_IMAGE: &str = "test:image/shotgun";
pub const PELLET_PROJECTILE: &str = "test:projectile/pellet";
/// Fully automatic while held; its arrows stick in what they hit head-on.
pub const BOW_ITEM: &str = "test:weapon/bow";
pub const BOW_IMAGE: &str = "test:image/bow";
pub const ARROW_PROJECTILE: &str = "test:projectile/arrow";
/// Explodes on everything, with radius falloff, burn and brick knockouts;
/// `min_shot_ticks` stops re-equipping to fire faster.
pub const ROCKET_ITEM: &str = "test:weapon/rocket";
pub const ROCKET_IMAGE: &str = "test:image/rocket";
pub const ROCKET_PROJECTILE: &str = "test:projectile/rocket";
/// The rocket's explosion: its effect name and `ExplosionData` definition,
/// which throws [`ROCKET_DEBRIS`].
pub const ROCKET_EXPLOSION: &str = "testRocketExplosion";
pub const ROCKET_DEBRIS: &str = "testRocketDebris";
/// Bounces until it times out (`arm_ticks` equals its lifetime).
pub const BOUNCER_ITEM: &str = "test:weapon/bouncer";
pub const BOUNCER_IMAGE: &str = "test:image/bouncer";
pub const BOUNCER_PROJECTILE: &str = "test:projectile/bouncer";
/// Sticks wherever it lands, then explodes when its lifetime ends.
pub const STICKY_ITEM: &str = "test:weapon/sticky";
pub const STICKY_IMAGE: &str = "test:image/sticky";
pub const STICKY_PROJECTILE: &str = "test:projectile/sticky";
/// Pushes hard and knocks bricks loose without exploding.
pub const SHOVE_ITEM: &str = "test:weapon/shove";
pub const SHOVE_IMAGE: &str = "test:image/shove";
pub const SHOVE_PROJECTILE: &str = "test:projectile/shove";
/// Hold to charge, release to throw; a short charge aborts.
pub const SPEAR_ITEM: &str = "test:weapon/spear";
pub const SPEAR_IMAGE: &str = "test:image/spear";
pub const SPEAR_PROJECTILE: &str = "test:projectile/spear";
/// Turns the player it hits into a horse.
pub const HORSE_RAY_ITEM: &str = "test:weapon/horse_ray";
pub const HORSE_RAY_IMAGE: &str = "test:image/horse_ray";
pub const HORSE_RAY_PROJECTILE: &str = "test:projectile/horse_ray";

// --- Melee. ---
/// Short reach, damage and no push. Its projectile is the one the host's
/// pumpkin bricks react to.
pub const SWORD_ITEM: &str = "test:weapon/sword";
pub const SWORD_IMAGE: &str = "test:image/sword";
pub const SWORD_PROJECTILE: &str = "v20.projectile.swordprojectile";
/// Short reach, a push and no damage.
pub const BROOM_ITEM: &str = "test:weapon/push_broom";
pub const BROOM_IMAGE: &str = "test:image/push_broom";
pub const BROOM_PROJECTILE: &str = "test:projectile/push_broom";

// --- Host commands. ---
/// A red key: its `onFire` tests the brick in view against its colour.
pub const KEY_ITEM: &str = "test:weapon/red_key";
pub const KEY_IMAGE: &str = "test:image/red_key";
/// The key image's colour, which bricks of a matching hue open for.
pub const KEY_COLOR: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
/// Its `onFire` boards the holder onto a ski vehicle the host spawns.
pub const SKIS_ITEM: &str = "test:weapon/skis";
pub const SKIS_IMAGE: &str = "test:image/skis";

// --- Spray cans (mounted by `serverCmdUseSprayCan`/`UseFXCan`). ---
/// The colour can; each palette colour is this image with a paint index.
pub const SPRAY_CAN_IMAGE: &str = "v20.image.bluespraycanimage";
pub const PAINT_PROJECTILE: &str = "v20.projectile.bluepaintprojectile";
/// The FX cans in `serverCmdUseFXCan` order, each with its paint.
pub const FX_CANS: [(&str, &str); 9] = [
    (
        "v20.image.flatspraycanimage",
        "v20.projectile.flatpaintprojectile",
    ),
    (
        "v20.image.pearlspraycanimage",
        "v20.projectile.pearlpaintprojectile",
    ),
    (
        "v20.image.chromespraycanimage",
        "v20.projectile.chromepaintprojectile",
    ),
    (
        "v20.image.glowspraycanimage",
        "v20.projectile.glowpaintprojectile",
    ),
    (
        "v20.image.blinkspraycanimage",
        "v20.projectile.blinkpaintprojectile",
    ),
    (
        "v20.image.swirlspraycanimage",
        "v20.projectile.swirlpaintprojectile",
    ),
    (
        "v20.image.rainbowspraycanimage",
        "v20.projectile.rainbowpaintprojectile",
    ),
    (
        "v20.image.stablespraycanimage",
        "v20.projectile.stablepaintprojectile",
    ),
    (
        "v20.image.jellospraycanimage",
        "v20.projectile.jellopaintprojectile",
    ),
];

// --- Sports balls: charged throws, catches, tackles and steals. ---
pub const BASKETBALL_ITEM: &str = "test:weapon/basketball";
pub const BASKETBALL_IMAGE: &str = "v20.image.basketballimage";
/// What a basketball's `onFire` switches to: the charged shot.
pub const BASKETBALL_SHOOT_IMAGE: &str = "v20.image.basketballshootimage";
pub const BASKETBALL_PROJECTILE: &str = "v20.projectile.basketballprojectile";
/// A dodgeball hitting a player before it bounces knocks them out.
pub const DODGEBALL_ITEM: &str = "test:weapon/dodgeball";
pub const DODGEBALL_IMAGE: &str = "v20.image.dodgeballimage";
pub const HORSE_DODGEBALL_IMAGE: &str = "v20.image.horsedodgeballimage";
pub const DODGEBALL_PROJECTILE: &str = "test:projectile/dodgeball";
/// A football or soccer ball coming to rest becomes this item again.
pub const FOOTBALL_ITEM: &str = "v20.weapon.footballitem";
pub const FOOTBALL_IMAGE: &str = "v20.image.footballimage";
pub const HORSE_FOOTBALL_IMAGE: &str = "v20.image.horsefootballimage";
pub const FOOTBALL_PROJECTILE: &str = "v20.projectile.footballprojectile";
pub const SOCCER_ITEM: &str = "v20.weapon.soccerballitem";
pub const SOCCER_IMAGE: &str = "v20.image.soccerballimage";
pub const SOCCER_PROJECTILE: &str = "v20.projectile.soccerballprojectile";
/// Every sports ball item.
pub const SPORT_ITEMS: [&str; 4] = [BASKETBALL_ITEM, DODGEBALL_ITEM, FOOTBALL_ITEM, SOCCER_ITEM];

// --- Projectiles the host spawns itself. ---
pub const SPAWN_PROJECTILE: &str = "v20.projectile.spawnprojectile";
pub const DEATH_PROJECTILE: &str = "v20.projectile.deathprojectile";
pub const ALARM_PROJECTILE: &str = "v20.projectile.alarmprojectile";
pub const BRICK_DEPLOY_PROJECTILE: &str = "v20.projectile.brickdeployprojectile";

// --- The Bubble Blaster sample Add-On. ---
pub const BUBBLE_BLASTER_ITEM: &str = "sample-bubble-blaster:weapon/bubble_blaster";
pub const BUBBLE_PROJECTILE: &str = "sample-bubble-blaster:projectile/bubble";

/// Every `v20.*` id here: definitions the engine (this crate or the host)
/// refers to by id, so the stand-ins must carry it.
pub const ENGINE_IDS: &[&str] = &[
    HAMMER,
    WRENCH,
    PRINTER,
    WAND,
    HAMMER_IMAGE,
    WRENCH_IMAGE,
    PRINTER_IMAGE,
    WAND_IMAGE,
    ADMIN_WAND_IMAGE,
    BRICK_IMAGE,
    HORSE_BRICK_IMAGE,
    GUN_ITEM,
    GUN_IMAGE,
    AKIMBO_IMAGE,
    LEFT_GUN_IMAGE,
    SWORD_PROJECTILE,
    SPRAY_CAN_IMAGE,
    PAINT_PROJECTILE,
    BASKETBALL_IMAGE,
    BASKETBALL_SHOOT_IMAGE,
    BASKETBALL_PROJECTILE,
    DODGEBALL_IMAGE,
    HORSE_DODGEBALL_IMAGE,
    FOOTBALL_ITEM,
    FOOTBALL_IMAGE,
    HORSE_FOOTBALL_IMAGE,
    FOOTBALL_PROJECTILE,
    SOCCER_ITEM,
    SOCCER_IMAGE,
    SOCCER_PROJECTILE,
    SPAWN_PROJECTILE,
    DEATH_PROJECTILE,
    ALARM_PROJECTILE,
    BRICK_DEPLOY_PROJECTILE,
];

/// Items whose image fires a projectile from the hand, the ones a random
/// player in a stress run picks from.
pub const PROJECTILE_WEAPONS: &[&str] = &[
    BUBBLE_BLASTER_ITEM,
    GUN_ITEM,
    AKIMBO_ITEM,
    SHOTGUN_ITEM,
    BOW_ITEM,
    ROCKET_ITEM,
    BOUNCER_ITEM,
    STICKY_ITEM,
    SHOVE_ITEM,
    SPEAR_ITEM,
    HORSE_RAY_ITEM,
    SWORD_ITEM,
    BROOM_ITEM,
];

const BUBBLE_PACK: &str =
    include_str!("../../../packages/samples/sample-bubble-blaster/assets/weapons.json");

/// One image state, built up field by field.
struct S(State);
impl S {
    fn new(name: &str, ticks: u32) -> Self {
        Self(State {
            name: name.into(),
            ticks,
            ..State::authored()
        })
    }
    fn timeout(mut self, to: usize) -> Self {
        self.0.timeout = Some(to);
        self
    }
    fn down(mut self, to: usize) -> Self {
        self.0.down = Some(to);
        self
    }
    fn up(mut self, to: usize) -> Self {
        self.0.up = Some(to);
        self
    }
    fn ammo(mut self, to: usize) -> Self {
        self.0.ammo = Some(to);
        self
    }
    fn no_ammo(mut self, to: usize) -> Self {
        self.0.no_ammo = Some(to);
        self
    }
    fn script(mut self, script: &str) -> Self {
        self.0.script = script.into();
        self
    }
    /// Checks its trigger transitions every tick rather than waiting out
    /// its ticks (`stateWaitForTimeout = false`).
    fn eager(mut self) -> Self {
        self.0.wait = false;
        self
    }
    fn locked(mut self) -> Self {
        self.0.allow_change = false;
        self
    }
    fn emitter(mut self, emitter: &str, seconds: f32) -> Self {
        self.0.emitter = emitter.into();
        self.0.emitter_seconds = seconds;
        self
    }
    fn sound(mut self, sound: &str) -> Self {
        self.0.sound = sound.into();
        self
    }
    fn sequence(mut self, sequence: &str) -> Self {
        self.0.sequence = sequence.into();
        self
    }
    fn eject_shell(mut self) -> Self {
        self.0.eject_shell = true;
        self
    }
}

/// Fires while the trigger is held: Activate, Ready, Fire, Reload.
fn automatic(activate: u32, fire: u32, reload: u32) -> Vec<State> {
    vec![
        S::new("Activate", activate).timeout(1).0,
        S::new("Ready", 0).down(2).0,
        S::new("Fire", fire).script("onFire").locked().timeout(3).0,
        S::new("Reload", reload).timeout(1).0,
    ]
}

/// One shot per press: after the shot it waits for the trigger to go up.
fn semi_automatic(activate: u32, fire: u32, smoke: u32) -> Vec<State> {
    vec![
        S::new("Activate", activate).timeout(1).0,
        S::new("Ready", 0).down(2).no_ammo(5).0,
        S::new("Fire", fire)
            .script("onFire")
            .locked()
            .eject_shell()
            .sound("testGunShotSound")
            .timeout(3)
            .0,
        S::new("Smoke", smoke).timeout(4).0,
        S::new("WaitForRelease", 0).up(1).0,
        S::new("NoAmmo", 0).ammo(1).0,
    ]
}

/// Hold to charge, release to throw: releasing before the charge is full
/// aborts it.
fn charged_throw(activate: u32, charge: u32, fire: u32) -> Vec<State> {
    vec![
        S::new("Activate", activate).timeout(1).0,
        S::new("Ready", 0).down(2).0,
        S::new("Charge", charge)
            .script("onCharge")
            .eager()
            .up(4)
            .timeout(3)
            .0,
        S::new("Armed", 0).up(5).0,
        S::new("AbortCharge", 10)
            .script("onAbortCharge")
            .timeout(1)
            .0,
        S::new("Fire", fire).script("onFire").timeout(1).0,
    ]
}

/// A swing: PreFire plays the holder's swing, Fire acts. Held, it swings
/// again and again.
fn swing(activate: u32, prefire: u32, fire: u32, recover: u32) -> Vec<State> {
    vec![
        S::new("Activate", activate).timeout(1).0,
        S::new("Ready", 0).down(2).0,
        S::new("PreFire", prefire).script("onPreFire").timeout(3).0,
        S::new("Fire", fire).script("onFire").timeout(4).0,
        S::new("CheckFire", recover)
            .script("onStopFire")
            .timeout(1)
            .0,
    ]
}

/// The hammer's and wands' [`swing`]. PreFire starts the arm's `armattack`
/// and CheckFire's `onStopFire` returns the arm to `root`, so Fire is long
/// enough (with PreFire, 32 ticks) for an avatar's swing to reach its peak
/// before it is stopped, however a client's frames fall: a shorter one cut
/// the swing at a different point each time.
fn tool_swing() -> Vec<State> {
    swing(1, 2, 30, 10)
}

/// A click: one swing per press, then it waits for the button to come up
/// (a tool that opens a dialog).
fn click(activate: u32, prefire: u32, fire: u32) -> Vec<State> {
    vec![
        S::new("Activate", activate).timeout(1).0,
        S::new("Ready", 0).down(2).0,
        S::new("PreFire", prefire).script("onPreFire").timeout(3).0,
        S::new("Fire", fire).script("onFire").timeout(4).0,
        S::new("WaitForRelease", 0).up(1).0,
    ]
}

/// A point-and-click with no wind-up: it acts the moment it is pressed,
/// then waits for the button to come up.
fn instant_click(fire: u32) -> Vec<State> {
    vec![
        S::new("Activate", 0).timeout(1).0,
        S::new("Ready", 0).down(2).0,
        S::new("Fire", fire).script("onFire").timeout(3).0,
        S::new("WaitForRelease", 0).up(1).0,
    ]
}

fn image(id: &str, name: &str, projectile: Option<&str>, states: Vec<State>) -> Image {
    Image {
        id: id.into(),
        name: name.into(),
        projectile: projectile.map(Into::into),
        color: [0.6, 0.6, 0.6, 1.0],
        correct_muzzle: true,
        arm_ready: true,
        crosshair: true,
        states,
        ..Image::default()
    }
}

fn item(id: &str, name: &str, ui_name: &str, image: &str) -> Item {
    Item {
        id: id.into(),
        name: name.into(),
        ui_name: ui_name.into(),
        image: image.into(),
        ..Item::default()
    }
}

fn projectile(id: &str, name: &str) -> ProjectileDef {
    ProjectileDef {
        id: id.into(),
        name: name.into(),
        ..ProjectileDef::default()
    }
}

fn explosion(effect: &str, damage: f32, radius: f32, impulse: f32) -> Explosion {
    Explosion {
        effect: effect.into(),
        damage,
        radius,
        impulse,
        impulse_radius: radius,
        ..Explosion::default()
    }
}

fn definition(name: &str, class: &str, fields: &[(&str, &str)]) -> Definition {
    Definition {
        name: name.into(),
        class: class.into(),
        parent: None,
        source: Evidence {
            path: "testing".into(),
            sha256: String::new(),
            line: 0,
        },
        fields: fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

fn damage_type(name: &str, verb: &str) -> (String, DamageType) {
    (
        name.to_ascii_lowercase(),
        DamageType {
            name: name.into(),
            suicide_message: format!("%1 {verb} themselves"),
            murder_message: format!("%2 {verb} %1"),
            vehicle_scale: 0.5,
            direct: true,
            special: false,
        },
    )
}

fn explosion_info(name: &str, sound: &str, seconds: f32) -> (String, ExplosionInfo) {
    (
        name.to_ascii_lowercase(),
        ExplosionInfo {
            name: name.into(),
            sound: sound.into(),
            shake: None,
            shape: String::new(),
            seconds,
            play_speed: 1.0,
            face_viewer: true,
            scale: [1.0; 3],
            sizes: vec![],
        },
    )
}

/// Everything above in one valid pack.
pub fn pack() -> Pack {
    let mut items = BTreeMap::new();
    let mut images = BTreeMap::new();
    let mut projectiles = BTreeMap::new();
    let mut add_item = |i: Item| {
        items.insert(i.id.clone(), i);
    };
    let mut add_image = |i: Image| {
        images.insert(i.id.clone(), i);
    };
    let mut add_projectile = |p: ProjectileDef| {
        projectiles.insert(p.id.clone(), p);
    };

    // Core tools and the host's own images.
    // The hammer and wands swing while held; the wrench and printer open a
    // dialog, so they act once per click; the printer has no wind-up.
    for (item_id, image_id, name, ui, states) in [
        (HAMMER, HAMMER_IMAGE, "hammerImage", "Hammer", tool_swing()),
        (
            WRENCH,
            WRENCH_IMAGE,
            "wrenchImage",
            "Wrench",
            click(1, 2, 6),
        ),
        (
            PRINTER,
            PRINTER_IMAGE,
            "printGunImage",
            "Printer",
            instant_click(6),
        ),
        (WAND, WAND_IMAGE, "wandImage", "Wand", tool_swing()),
    ] {
        add_item(item(item_id, &format!("{ui}Item"), ui, image_id));
        add_image(image(image_id, name, None, states));
    }
    add_image(image(
        ADMIN_WAND_IMAGE,
        "adminWandImage",
        None,
        tool_swing(),
    ));
    for (id, name) in [
        (BRICK_IMAGE, "brickImage"),
        (HORSE_BRICK_IMAGE, "horseBrickImage"),
    ] {
        add_image(Image {
            color_shift: true,
            ..image(
                id,
                name,
                Some(BRICK_DEPLOY_PROJECTILE),
                vec![
                    S::new("Activate", 4).timeout(1).0,
                    S::new("Ready", 0).down(2).0,
                    S::new("Fire", 6)
                        .script("onFire")
                        .sequence(BRICK_FIRE_SEQUENCE)
                        .emitter(BRICK_TRAIL_EMITTER, 0.1)
                        .timeout(3)
                        .0,
                    S::new("WaitForRelease", 0).up(1).0,
                ],
            )
        });
    }

    // Guns.
    add_item(item(GUN_ITEM, "gunItem", "Test Gun", GUN_IMAGE));
    add_image(Image {
        casing: GUN_CASING.into(),
        ..image(
            GUN_IMAGE,
            "gunImage",
            Some(GUN_PROJECTILE),
            semi_automatic(10, 8, 12),
        )
    });
    add_projectile(ProjectileDef {
        speed: 400.0,
        gravity: 0.0,
        damage: 25.0,
        damage_type: "$DamageType::TestGun".into(),
        impulse: 50.0,
        explosion: explosion("testGunExplosion", 0.0, 0.0, 0.0),
        brick: BrickImpact {
            radius: 0.0,
            direct: true,
            force: 5.0,
            max_volume: 20.0,
            max_floating_volume: 10.0,
        },
        ..projectile(GUN_PROJECTILE, "testGunProjectile")
    });
    add_item(item(AKIMBO_ITEM, "akimboItem", "Test Akimbo", AKIMBO_IMAGE));
    let mut right = semi_automatic(10, 8, 12);
    // Released: the right gun pulses the left one's trigger.
    right[4] = S::new("WaitForRelease", 0).up(6).0;
    right.push(S::new("FireAkimbo", 8).script("onFireAkimbo").timeout(1).0);
    // Both guns throw the gun's casing, as each fires.
    add_image(Image {
        casing: GUN_CASING.into(),
        ..image(AKIMBO_IMAGE, "akimboGunImage", Some(GUN_PROJECTILE), right)
    });
    let mut left = semi_automatic(10, 8, 12);
    // The left gun sees only one-tick pulses: back to Ready after Smoke.
    left[3] = S::new("Smoke", 12).timeout(1).0;
    add_image(Image {
        casing: GUN_CASING.into(),
        ..image(
            LEFT_GUN_IMAGE,
            "leftHandedGunImage",
            Some(GUN_PROJECTILE),
            left,
        )
    });
    add_item(item(
        SHOTGUN_ITEM,
        "shotgunItem",
        "Test Shotgun",
        SHOTGUN_IMAGE,
    ));
    add_image(Image {
        shot: Some(Shot {
            projectiles: 6,
            spread: 0.02,
            recoil: 5.0,
            ..Shot::SINGLE
        }),
        ..image(
            SHOTGUN_IMAGE,
            "testShotgunImage",
            Some(PELLET_PROJECTILE),
            semi_automatic(12, 10, 30),
        )
    });
    add_projectile(ProjectileDef {
        speed: 200.0,
        gravity: 0.0,
        lifetime_ticks: 120,
        damage: 8.0,
        damage_type: "$DamageType::TestGun".into(),
        impulse: 20.0,
        ..projectile(PELLET_PROJECTILE, "testPelletProjectile")
    });
    add_item(item(BOW_ITEM, "bowItem", "Test Bow", BOW_IMAGE));
    add_image(image(
        BOW_IMAGE,
        "testBowImage",
        Some(ARROW_PROJECTILE),
        automatic(10, 6, 30),
    ));
    add_projectile(ProjectileDef {
        speed: 40.0,
        gravity: 0.5,
        ballistic: true,
        lifetime_ticks: 600,
        fade_ticks: 60,
        // Never armed: it sticks or bounces, and explodes only on players.
        arm_ticks: 600,
        explode_player: true,
        elasticity: 0.5,
        friction: 0.3,
        damage: 20.0,
        damage_type: "$DamageType::TestArrow".into(),
        impulse: 30.0,
        min_stick_speed: 5.0,
        bounce_angle: 60.0,
        stick_effect: "testArrowStickExplosion".into(),
        ..projectile(ARROW_PROJECTILE, "testArrowProjectile")
    });
    add_item(item(ROCKET_ITEM, "rocketItem", "Test Rocket", ROCKET_IMAGE));
    add_image(Image {
        min_shot_ticks: 96,
        ..image(
            ROCKET_IMAGE,
            "testRocketImage",
            Some(ROCKET_PROJECTILE),
            vec![
                S::new("Activate", 8).timeout(1).0,
                S::new("Ready", 0).down(2).0,
                S::new("Fire", 6).script("onFire").locked().timeout(3).0,
                S::new("Smoke", 10).timeout(4).0,
                S::new("CoolDown", 24).timeout(1).0,
            ],
        )
    });
    add_projectile(ProjectileDef {
        speed: 60.0,
        gravity: 0.0,
        lifetime_ticks: 600,
        damage: 30.0,
        damage_type: "$DamageType::TestRocket".into(),
        radius_damage_type: "$DamageType::TestRocketRadius".into(),
        explode_player: true,
        explode_death: true,
        explosion: Explosion {
            impulse_radius: 6.0,
            impulse_vertical: 500.0,
            burn_seconds: 2.0,
            ..explosion(ROCKET_EXPLOSION, 60.0, 5.0, 2000.0)
        },
        brick: BrickImpact {
            radius: 4.0,
            direct: true,
            force: 20.0,
            max_volume: 400.0,
            max_floating_volume: 100.0,
        },
        trail: "testRocketTrailEmitter".into(),
        light_radius: 3.0,
        light_color: [1.0, 0.6, 0.2],
        ..projectile(ROCKET_PROJECTILE, "testRocketProjectile")
    });
    add_item(item(
        BOUNCER_ITEM,
        "bouncerItem",
        "Test Bouncer",
        BOUNCER_IMAGE,
    ));
    add_image(image(
        BOUNCER_IMAGE,
        "testBouncerImage",
        Some(BOUNCER_PROJECTILE),
        automatic(10, 10, 20),
    ));
    add_projectile(ProjectileDef {
        speed: 30.0,
        gravity: 1.0,
        ballistic: true,
        lifetime_ticks: 720,
        arm_ticks: 720,
        elasticity: 1.0,
        friction: 0.5,
        bounce_angle: 10.0,
        rest_speed: 0.5,
        impulse: 100.0,
        bounce_effect: "testBounceExplosion".into(),
        ..projectile(BOUNCER_PROJECTILE, "testBouncerProjectile")
    });
    add_item(item(STICKY_ITEM, "stickyItem", "Test Sticky", STICKY_IMAGE));
    add_image(image(
        STICKY_IMAGE,
        "testStickyImage",
        Some(STICKY_PROJECTILE),
        automatic(10, 10, 40),
    ));
    add_projectile(ProjectileDef {
        speed: 25.0,
        gravity: 1.0,
        ballistic: true,
        lifetime_ticks: 240,
        arm_ticks: 240,
        explode_death: true,
        min_stick_speed: 1.0,
        // Sticks however it lands.
        bounce_angle: 180.0,
        explosion: Explosion {
            impulse_radius: 3.0,
            ..explosion("testStickyExplosion", 20.0, 3.0, 300.0)
        },
        radius_damage_type: "$DamageType::TestRocketRadius".into(),
        ..projectile(STICKY_PROJECTILE, "testStickyProjectile")
    });
    add_item(item(SHOVE_ITEM, "shoveItem", "Test Shove", SHOVE_IMAGE));
    add_image(image(
        SHOVE_IMAGE,
        "testShoveImage",
        Some(SHOVE_PROJECTILE),
        automatic(10, 10, 20),
    ));
    add_projectile(ProjectileDef {
        speed: 80.0,
        gravity: 0.0,
        impulse: 5000.0,
        vertical: 3000.0,
        brick: BrickImpact {
            radius: 0.5,
            direct: true,
            force: 50.0,
            max_volume: 100.0,
            max_floating_volume: 100.0,
        },
        ..projectile(SHOVE_PROJECTILE, "testShoveProjectile")
    });
    add_item(item(SPEAR_ITEM, "spearItem", "Test Spear", SPEAR_IMAGE));
    add_image(image(
        SPEAR_IMAGE,
        "testSpearImage",
        Some(SPEAR_PROJECTILE),
        charged_throw(10, 60, 20),
    ));
    add_projectile(ProjectileDef {
        speed: 50.0,
        gravity: 0.4,
        ballistic: true,
        lifetime_ticks: 1200,
        damage: 50.0,
        damage_type: "$DamageType::TestSpear".into(),
        explode_player: true,
        explosion: explosion("testSpearExplosion", 30.0, 2.0, 400.0),
        ..projectile(SPEAR_PROJECTILE, "testSpearProjectile")
    });
    add_item(item(
        HORSE_RAY_ITEM,
        "horseRayItem",
        "Test Horse Ray",
        HORSE_RAY_IMAGE,
    ));
    add_image(image(
        HORSE_RAY_IMAGE,
        "testHorseRayImage",
        Some(HORSE_RAY_PROJECTILE),
        automatic(6, 20, 10),
    ));
    add_projectile(ProjectileDef {
        speed: 40.0,
        gravity: 0.0,
        lifetime_ticks: 240,
        // Nominal: the ray transforms rather than damages.
        damage: 10.0,
        ..projectile(HORSE_RAY_PROJECTILE, "horseRayProjectile")
    });

    // Melee.
    add_item(item(SWORD_ITEM, "swordItem", "Test Sword", SWORD_IMAGE));
    add_image(Image {
        melee: true,
        ..image(
            SWORD_IMAGE,
            "testSwordImage",
            Some(SWORD_PROJECTILE),
            swing(6, 4, 4, 20),
        )
    });
    add_projectile(ProjectileDef {
        speed: 30.0,
        gravity: 0.0,
        lifetime_ticks: 12,
        fade_ticks: 12,
        damage: 30.0,
        damage_type: "$DamageType::TestSword".into(),
        explosion: explosion("testSwordExplosion", 0.0, 0.0, 0.0),
        ..projectile(SWORD_PROJECTILE, "swordProjectile")
    });
    add_item(item(BROOM_ITEM, "pushBroomItem", "Test Broom", BROOM_IMAGE));
    add_image(Image {
        melee: true,
        ..image(
            BROOM_IMAGE,
            "testPushBroomImage",
            Some(BROOM_PROJECTILE),
            swing(6, 4, 4, 20),
        )
    });
    add_projectile(ProjectileDef {
        speed: 30.0,
        gravity: 0.0,
        lifetime_ticks: 12,
        fade_ticks: 12,
        impulse: 400.0,
        vertical: 900.0,
        ..projectile(BROOM_PROJECTILE, "testPushBroomProjectile")
    });

    // Host commands.
    add_item(item(KEY_ITEM, "redKeyItem", "Test Red Key", KEY_IMAGE));
    add_image(Image {
        color: KEY_COLOR,
        ..image(KEY_IMAGE, "testRedKeyImage", None, swing(4, 2, 6, 10))
    });
    add_item(item(SKIS_ITEM, "skiItem", "Test Skis", SKIS_IMAGE));
    add_image(image(
        SKIS_IMAGE,
        "skiWeaponImage",
        None,
        automatic(6, 10, 10),
    ));

    // Spray cans.
    let can = |id: &str, name: &str, paint: &str, emitter: &str| {
        image(
            id,
            name,
            Some(paint),
            vec![
                S::new("Activate", 4).timeout(1).0,
                S::new("Ready", 0).down(2).0,
                S::new("Fire", 4)
                    .script("onFire")
                    .emitter(emitter, 0.05)
                    .timeout(1)
                    .0,
            ],
        )
    };
    let paint = |id: &str, name: &str, effect: &str| ProjectileDef {
        speed: 20.0,
        gravity: 0.0,
        lifetime_ticks: 48,
        fade_ticks: 24,
        collide_players: true,
        explosion: explosion(effect, 0.0, 0.0, 0.0),
        ..projectile(id, name)
    };
    add_image(Image {
        color_shift: true,
        ..can(
            SPRAY_CAN_IMAGE,
            "blueSprayCanImage",
            PAINT_PROJECTILE,
            "bluePaintEmitter",
        )
    });
    add_projectile(paint(
        PAINT_PROJECTILE,
        "bluePaintProjectile",
        "bluePaintExplosion",
    ));
    for (image_id, paint_id) in FX_CANS {
        let fx = image_id
            .strip_prefix("v20.image.")
            .and_then(|n| n.strip_suffix("spraycanimage"))
            .expect("FX can id");
        add_image(can(
            image_id,
            &format!("{fx}SprayCanImage"),
            paint_id,
            &format!("{fx}PaintEmitter"),
        ));
        add_projectile(paint(
            paint_id,
            &format!("{fx}PaintProjectile"),
            &format!("{fx}PaintExplosion"),
        ));
    }

    // Sports balls.
    let ball = |id: &str, name: &str, image: &str| ProjectileDef {
        speed: 20.0,
        gravity: 1.0,
        ballistic: true,
        lifetime_ticks: 1200,
        arm_ticks: 1200,
        elasticity: 0.6,
        friction: 0.2,
        rest_speed: 1.0,
        sport_image: Some(image.into()),
        bounce_effect: "testBallBounceExplosion".into(),
        ..projectile(id, name)
    };
    add_item(Item {
        sport: true,
        ..item(
            BASKETBALL_ITEM,
            "basketballItem",
            "Test Basketball",
            BASKETBALL_IMAGE,
        )
    });
    add_image(image(
        BASKETBALL_IMAGE,
        "basketballImage",
        Some(BASKETBALL_PROJECTILE),
        vec![
            S::new("Activate", 10).timeout(1).0,
            S::new("Ready", 0).down(2).0,
            S::new("Fire", 4).script("onFire").timeout(1).0,
        ],
    ));
    add_image(image(
        BASKETBALL_SHOOT_IMAGE,
        "basketballShootImage",
        Some(BASKETBALL_PROJECTILE),
        charged_throw(4, 30, 10),
    ));
    add_projectile(ball(
        BASKETBALL_PROJECTILE,
        "basketballProjectile",
        BASKETBALL_IMAGE,
    ));
    add_item(Item {
        sport: true,
        ..item(
            DODGEBALL_ITEM,
            "dodgeballItem",
            "Test Dodgeball",
            DODGEBALL_IMAGE,
        )
    });
    for (id, name) in [
        (DODGEBALL_IMAGE, "dodgeballImage"),
        (HORSE_DODGEBALL_IMAGE, "horseDodgeballImage"),
    ] {
        add_image(image(
            id,
            name,
            Some(DODGEBALL_PROJECTILE),
            charged_throw(10, 30, 10),
        ));
    }
    add_projectile(ball(
        DODGEBALL_PROJECTILE,
        "dodgeballProjectile",
        DODGEBALL_IMAGE,
    ));
    add_item(Item {
        sport: true,
        ..item(
            FOOTBALL_ITEM,
            "footballItem",
            "Test Football",
            FOOTBALL_IMAGE,
        )
    });
    for (id, name) in [
        (FOOTBALL_IMAGE, "footballImage"),
        (HORSE_FOOTBALL_IMAGE, "horseFootballImage"),
    ] {
        add_image(image(
            id,
            name,
            Some(FOOTBALL_PROJECTILE),
            charged_throw(10, 30, 10),
        ));
    }
    add_projectile(ball(
        FOOTBALL_PROJECTILE,
        "footballProjectile",
        FOOTBALL_IMAGE,
    ));
    add_item(Item {
        sport: true,
        ..item(
            SOCCER_ITEM,
            "soccerBallItem",
            "Test Soccer Ball",
            SOCCER_IMAGE,
        )
    });
    add_image(image(
        SOCCER_IMAGE,
        "soccerBallImage",
        Some(SOCCER_PROJECTILE),
        charged_throw(10, 30, 10),
    ));
    add_projectile(ball(
        SOCCER_PROJECTILE,
        "soccerBallProjectile",
        SOCCER_IMAGE,
    ));

    // The host's own projectiles.
    add_projectile(ProjectileDef {
        speed: 0.0,
        gravity: 0.0,
        lifetime_ticks: 2,
        explode_death: true,
        explosion: explosion("testSpawnExplosion", 0.0, 0.0, 0.0),
        ..projectile(SPAWN_PROJECTILE, "spawnProjectile")
    });
    add_projectile(ProjectileDef {
        speed: 0.0,
        gravity: 0.0,
        lifetime_ticks: 2,
        explode_death: true,
        explosion: explosion("testDeathExplosion", 0.0, 0.0, 0.0),
        ..projectile(DEATH_PROJECTILE, "deathProjectile")
    });
    add_projectile(ProjectileDef {
        speed: 1.0,
        gravity: 0.0,
        lifetime_ticks: 24,
        explode_death: true,
        explosion: explosion("testAlarmExplosion", 0.0, 0.0, 0.0),
        ..projectile(ALARM_PROJECTILE, "alarmProjectile")
    });
    add_projectile(ProjectileDef {
        speed: 60.0,
        gravity: 0.0,
        lifetime_ticks: 60,
        explosion: explosion("testBrickDeployExplosion", 0.0, 0.0, 0.0),
        ..projectile(BRICK_DEPLOY_PROJECTILE, "brickDeployProjectile")
    });

    let mut damage_types: BTreeMap<_, _> = [
        damage_type("Default", "died to"),
        damage_type("TestGun", "shot"),
        damage_type("TestArrow", "pinned"),
        damage_type("TestRocket", "rocketed"),
        damage_type("TestRocketRadius", "blew up"),
        damage_type("TestSpear", "speared"),
        damage_type("TestSword", "sliced"),
    ]
    .into_iter()
    .collect();
    let mut explosions: BTreeMap<_, _> = [
        explosion_info("testGunExplosion", "testBulletHitSound", 0.2),
        explosion_info(ROCKET_EXPLOSION, "testRocketBoomSound", 1.0),
        explosion_info("testStickyExplosion", "testRocketBoomSound", 0.6),
        explosion_info("testSpearExplosion", "", 0.4),
        explosion_info("testSwordExplosion", "testSwordHitSound", 0.2),
        explosion_info("testArrowStickExplosion", "testArrowHitSound", 0.1),
        explosion_info("testBounceExplosion", "", 0.1),
        explosion_info("bluePaintExplosion", "", 0.2),
    ]
    .into_iter()
    .collect();
    explosions
        .get_mut(&ROCKET_EXPLOSION.to_ascii_lowercase())
        .expect("rocket")
        .shake = Some(CameraShake {
        frequency: [8.0, 9.0, 7.0],
        amplitude: [1.0, 1.0, 1.0],
        seconds: 0.5,
        radius: 12.0,
        falloff: 4.0,
    });
    let definitions = vec![
        definition(
            GUN_CASING,
            "DebrisData",
            &[
                ("shapefile", "\"test/shell.dts\""),
                ("lifetime", "1.5"),
                ("elasticity", "0.4"),
                ("friction", "0.3"),
                ("numbounces", "2"),
                ("fade", "1"),
                ("gravmodifier", "2"),
            ],
        ),
        definition(
            ROCKET_DEBRIS,
            "DebrisData",
            &[
                ("shapefile", "\"test/chunk.dts\""),
                ("emitters", "\"testDebrisTrailEmitter\""),
                ("lifetime", "2"),
                ("numbounces", "1"),
                ("gravmodifier", "1.5"),
            ],
        ),
        definition(
            ROCKET_EXPLOSION,
            "ExplosionData",
            &[
                ("debris", ROCKET_DEBRIS),
                ("debrisnum", "5"),
                ("debrisnumvariance", "2"),
                ("debristhetamin", "10"),
                ("debristhetamax", "70"),
                ("debrisvelocity", "12"),
                ("debrisvelocityvariance", "3"),
            ],
        ),
    ];

    let mut pack = Pack {
        schema_version: SCHEMA,
        id: "test".into(),
        items,
        images,
        projectiles,
        external_projectiles: Default::default(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        sounds: BTreeMap::new(),
        effects: PackEffects::default(),
        definitions,
        resources: vec![],
        diagnostics: vec![],
        bindings: vec![],
    };
    // The Bubble Blaster sample, as it ships.
    let bubble = Pack::from_json(BUBBLE_PACK.as_bytes()).expect("Bubble Blaster sample pack");
    pack.items.extend(bubble.items);
    pack.images.extend(bubble.images);
    pack.projectiles.extend(bubble.projectiles);
    damage_types.extend(bubble.damage_types);
    explosions.extend(bubble.explosions);
    pack.damage_types = damage_types;
    pack.explosions = explosions;
    pack.validate().expect("the testing pack is valid");
    pack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_every_engine_id_and_role() {
        let p = pack();
        let ids = ENGINE_IDS
            .iter()
            .chain(PROJECTILE_WEAPONS)
            .chain(&SPORT_ITEMS)
            .chain(FX_CANS.iter().flat_map(|(image, paint)| [image, paint]));
        for id in ids {
            assert!(
                p.items.contains_key(*id)
                    || p.images.contains_key(*id)
                    || p.projectiles.contains_key(*id),
                "{id}"
            );
        }
        for item in PROJECTILE_WEAPONS {
            let image = &p.images[&p.items[*item].image];
            assert!(image.projectile.is_some(), "{item}");
        }
        for id in ENGINE_IDS {
            assert!(id.starts_with("v20."), "{id}");
        }
    }
}
