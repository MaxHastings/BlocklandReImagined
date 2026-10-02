//! What v20's stock images did in their TorqueScript state callbacks
//! (`spearImage::onCharge`, `keyImage::onFire`, `basketballImage::onFire`),
//! which the engine reproduces by the image's datablock name.
//!
//! This table is the only place the state-script callback
//! ([`WeaponsWorld::callback`]) looks at an image's name: the callback
//! reads a [`Stock`] and stays generic. Add-On images never land here; they
//! say what their scripts do in data (`Image::scripts`, `Image::commands`,
//! `Image::shot`, ...), and a behaviour two images need is a data field,
//! not another name. See docs/architecture/weapon-scripts.md.
use super::{HOST_TOOL_IMAGES, Image, ProjectileDef};

/// What an image's `onFire` does instead of launching its projectile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StockFire {
    /// Launches its projectile (`Parent::onFire`).
    Projectile,
    /// Raycasts and acts on what it hits, in the host
    /// ([`HOST_TOOL_IMAGES`], [`super::Event::ToolFire`]).
    HostTool,
    /// Puts the skis on or takes them off (`skiWeaponImage::onFire`).
    Skis,
    /// Tries the brick it points at (`keyImage::onFire`).
    Key,
    /// Swaps the ball for its shooting image (`basketballImage::onFire`).
    ShootBasketball,
}

/// What a held sports ball does with the movement keys
/// ([`super::WeaponsWorld::sport_trigger`], Item_Sports' `onTrigger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SportKeys {
    /// Jet pressed on foot: a football's lateral.
    Lateral,
    /// Jet pressed pops a soccer ball up; any other key drops it.
    Pop,
    /// Jet released on foot: a basketball's pass.
    Pass,
}

/// The Item_Sports balls that catches and drops treat differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ball {
    Basketball,
    Football,
}

/// A stock image's script behaviour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Stock {
    /// The arm animation `onCharge` plays.
    pub charge_arm: Option<&'static str>,
    /// The arm animation `onPreFire` plays.
    pub prefire_arm: Option<&'static str>,
    pub fire: StockFire,
    /// The arm animation after a shot, before the state's own arm or a left
    /// hand's recoil (a throw).
    pub throw_arm: Option<&'static str>,
    /// The arm animation after a shot when nothing else played one (a
    /// gun's kick).
    pub recoil_arm: Option<&'static str>,
    /// A sports ball's throw: forward speed and upward speed, before the
    /// holder's own velocity (Item_Sports' `onFire`).
    pub throw: (f32, f32),
    /// A basketball's throw aims at what the holder looks at.
    pub aimed_throw: bool,
    /// Ticks after spawning before it can be thrown (a dodgeball's).
    pub spawn_grace_ticks: u64,
    /// Its projectile counts as thrown (a football's, which can be caught).
    pub thrown: bool,
    /// What the movement keys do while it is held.
    pub sport_keys: Option<SportKeys>,
    /// The image its mount puts in the left hand, by v20 datablock name
    /// (`AkimboGunImage::onMount`), when it declares no `left_image`.
    pub left_image: Option<&'static str>,
    /// The sports ball it is, for the catches and drops that treat one
    /// ball differently (`runtime/sports.rs`).
    pub ball: Option<Ball>,
}

impl Stock {
    /// The behaviour of `image`, by its datablock name.
    pub(super) fn of(image: &Image) -> Self {
        Self::named(&image.name.to_ascii_lowercase())
    }
    /// The behaviour of the image named `name`, lower case.
    fn named(name: &str) -> Self {
        let has = |part: &str| name.contains(part);
        let charge_arm = (has("spear") || has("football")).then_some("spearReady");
        let prefire_arm = if has("key") {
            Some("shiftLeft")
        } else if name == "wrenchimage" {
            Some("wrench")
        } else if has("sword") || matches!(name, "hammerimage" | "wandimage" | "adminwandimage") {
            Some("armattack")
        } else {
            None
        };
        let fire = if HOST_TOOL_IMAGES.contains(&name) {
            StockFire::HostTool
        } else if name == "skiweaponimage" {
            StockFire::Skis
        } else if has("keyimage") {
            StockFire::Key
        } else if name == "basketballimage" {
            StockFire::ShootBasketball
        } else {
            StockFire::Projectile
        };
        let throw_arm = if has("spear") || has("football") {
            Some("spearThrow")
        } else if has("pushbroom") {
            Some("rotCW")
        } else {
            None
        };
        let recoil_arm = (has("gun") || has("horseray")).then_some("shiftAway");
        let throw = if has("dodgeball") {
            (30.0, 4.0)
        } else if has("football") {
            (40.0, 0.0)
        } else if has("soccer") {
            (20.0, 3.0)
        } else {
            (7.0, 7.5)
        };
        Self {
            charge_arm,
            prefire_arm,
            fire,
            throw_arm,
            recoil_arm,
            throw,
            aimed_throw: has("basketball"),
            spawn_grace_ticks: if has("dodgeball") { 120 } else { 0 },
            thrown: has("football"),
            sport_keys: if has("football") {
                Some(SportKeys::Lateral)
            } else if has("soccer") {
                Some(SportKeys::Pop)
            } else if has("basketballshoot") {
                Some(SportKeys::Pass)
            } else {
                None
            },
            left_image: (name == "akimbogunimage").then_some("LeftHandedGunImage"),
            ball: if has("basketball") {
                Some(Ball::Basketball)
            } else if has("football") {
                Some(Ball::Football)
            } else {
                None
            },
        }
    }
}

/// v20 stock projectiles whose `onCollision` scripts did more than their
/// data says, by datablock name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StockProjectile {
    /// A hit before it bounces knocks the player out (`dodgeballProjectile`).
    Dodgeball,
    /// A catch scores the throw's distance, and at rest it is a
    /// `footballItem` (`footballProjectile`).
    Football,
    /// A hit turns the player into a horse (`horseRayProjectile`).
    HorseRay,
}
impl StockProjectile {
    pub(super) fn of(d: &ProjectileDef) -> Option<Self> {
        match d.name.to_ascii_lowercase().as_str() {
            "dodgeballprojectile" => Some(Self::Dodgeball),
            "footballprojectile" => Some(Self::Football),
            "horserayprojectile" => Some(Self::HorseRay),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_images_keep_their_v20_script_behaviour() {
        let spear = Stock::named("spearimage");
        assert_eq!(spear.charge_arm, Some("spearReady"));
        assert_eq!(spear.throw_arm, Some("spearThrow"));
        assert_eq!(Stock::named("wrenchimage").prefire_arm, Some("wrench"));
        assert_eq!(Stock::named("wrenchimage").fire, StockFire::HostTool);
        assert_eq!(Stock::named("swordimage").prefire_arm, Some("armattack"));
        assert_eq!(Stock::named("keyredimage").prefire_arm, Some("shiftLeft"));
        assert_eq!(Stock::named("skiweaponimage").fire, StockFire::Skis);
        assert_eq!(
            Stock::named("basketballimage").fire,
            StockFire::ShootBasketball
        );
        assert!(Stock::named("basketballshootimage").aimed_throw);
        let football = Stock::named("footballimage");
        assert!(football.thrown);
        assert_eq!(football.throw, (40.0, 0.0));
        assert_eq!(football.sport_keys, Some(SportKeys::Lateral));
        assert_eq!(football.ball, Some(Ball::Football));
        assert_eq!(
            Stock::named("basketballshootimage").ball,
            Some(Ball::Basketball)
        );
        assert_eq!(
            Stock::named("soccerballimage").sport_keys,
            Some(SportKeys::Pop)
        );
        assert_eq!(
            Stock::named("basketballshootimage").sport_keys,
            Some(SportKeys::Pass)
        );
        assert_eq!(Stock::named("dodgeballimage").spawn_grace_ticks, 120);
        assert_eq!(Stock::named("gunimage").recoil_arm, Some("shiftAway"));
        assert_eq!(
            Stock::named("akimbogunimage").left_image,
            Some("LeftHandedGunImage")
        );
        let plain = Stock::named("rocketlauncherimage");
        assert_eq!(plain.fire, StockFire::Projectile);
        assert_eq!(plain.charge_arm, None);
        assert_eq!(plain.prefire_arm, None);
        assert_eq!(plain.recoil_arm, None);
    }

    /// The weapons runtime reads [`Stock`] and [`StockProjectile`], never
    /// an image's or projectile's name: a v20 compatibility case goes in
    /// this file or in pack data.
    #[test]
    fn the_runtime_never_matches_datablock_names() {
        let sources = [
            ("runtime.rs", include_str!("../runtime.rs")),
            ("runtime/sports.rs", include_str!("sports.rs")),
            ("runtime/persistence.rs", include_str!("persistence.rs")),
        ];
        let banned = [
            "name.contains(",
            "name == \"",
            "name.as_str()",
            "image.contains(",
            "image.to_ascii_lowercase()",
            "projectile.contains(",
            "name.eq_ignore_ascii_case(\"",
            "== native_id(",
        ];
        for (file, source) in sources {
            for pattern in banned {
                assert!(
                    !source.contains(pattern),
                    "{file} matches a datablock name (`{pattern}`); \
                     put the case in runtime/stock.rs or in pack data"
                );
            }
        }
    }
}
