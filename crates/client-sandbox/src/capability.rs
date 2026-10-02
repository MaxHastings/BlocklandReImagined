//! What client code may ask the engine to do. Each capability is one named
//! group of host functions; a module can import only the functions of the
//! capabilities its Add-On declares, so an undeclared request fails when the
//! module is linked, before any of its code runs.
use serde::{Deserialize, Serialize};

/// How much a player must trust a server before a capability runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Data only: downloads and loads without asking.
    Data,
    /// Sandboxed code: runs after the player trusts the server once.
    Sandboxed,
    /// Beyond the sandbox: runs only after the player chooses to fully
    /// trust this server for this exact Add-On.
    Elevated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Draw meshes into the Add-On's own render layer.
    #[serde(rename = "render.layer")]
    RenderLayer,
    /// Use the Add-On's own WGSL shaders for its materials.
    #[serde(rename = "render.shader")]
    RenderShader,
    /// Play sounds shipped in the Add-On.
    #[serde(rename = "audio")]
    Audio,
    /// Read keys, only while the Add-On's panel has focus.
    #[serde(rename = "input.focused")]
    InputFocused,
    /// Exchange messages with the Add-On's own server script.
    #[serde(rename = "net.message")]
    NetMessage,
    /// Read what the player's game already shows: where players and
    /// vehicles are, and the server's public Add-On state.
    #[serde(rename = "world.read")]
    WorldRead,
    /// Simulate its own bodies and joints on the player's PC only: they
    /// collide with the world as drawn and never touch gameplay.
    #[serde(rename = "physics.local")]
    PhysicsLocal,
    /// Pose the bodies of players as this client draws them (a ragdoll, a
    /// dance); where players are and what they do stays the server's.
    #[serde(rename = "avatar.pose")]
    AvatarPose,
    /// Fetch from URLs (video streams, web images). Reveals the player's
    /// address to whoever runs the URL.
    #[serde(rename = "net.http")]
    NetHttp,
    /// Read and write files in the Add-On's own folder on the player's PC.
    #[serde(rename = "files.addon_folder")]
    FilesAddOnFolder,
    /// A native plugin: full access to the player's PC. Elevated, and the
    /// strongest prompt: the player types the server's name. Designed, not
    /// built: nothing runs it yet (see `docs/architecture/client-sandbox.md`).
    #[serde(rename = "native")]
    Native,
}

impl Capability {
    pub const ALL: &[Capability] = &[
        Self::RenderLayer,
        Self::RenderShader,
        Self::Audio,
        Self::InputFocused,
        Self::NetMessage,
        Self::WorldRead,
        Self::PhysicsLocal,
        Self::AvatarPose,
        Self::NetHttp,
        Self::FilesAddOnFolder,
        Self::Native,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::RenderLayer => "render.layer",
            Self::RenderShader => "render.shader",
            Self::Audio => "audio",
            Self::InputFocused => "input.focused",
            Self::NetMessage => "net.message",
            Self::WorldRead => "world.read",
            Self::PhysicsLocal => "physics.local",
            Self::AvatarPose => "avatar.pose",
            Self::NetHttp => "net.http",
            Self::FilesAddOnFolder => "files.addon_folder",
            Self::Native => "native",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.name() == name)
    }

    pub fn tier(self) -> Tier {
        match self {
            Self::RenderLayer
            | Self::RenderShader
            | Self::Audio
            | Self::InputFocused
            | Self::NetMessage
            | Self::WorldRead
            | Self::PhysicsLocal
            | Self::AvatarPose => Tier::Sandboxed,
            Self::NetHttp | Self::FilesAddOnFolder | Self::Native => Tier::Elevated,
        }
    }

    /// Whether this build can run it. Elevated capabilities are designed
    /// and can be trusted, but no host functions exist for them yet.
    pub fn available(self) -> bool {
        self.tier() == Tier::Sandboxed
    }

    /// What it lets the Add-On do, in the player's words, for the trust
    /// prompt and the Add-Ons screen.
    pub fn plain_words(self) -> &'static str {
        match self {
            Self::RenderLayer => "Draw its own 3D shapes in the world",
            Self::RenderShader => "Use its own graphics effects (shaders)",
            Self::Audio => "Play sounds that come with it",
            Self::InputFocused => "Read your keys while its panel is selected",
            Self::NetMessage => "Talk to its part running on the server",
            Self::WorldRead => "See where players and vehicles are, as your screen shows them",
            Self::PhysicsLocal => "Throw its own objects around with physics on your screen only",
            Self::AvatarPose => "Change how players' bodies move on your screen only",
            Self::NetHttp => {
                "Load things from the internet, which shows your IP address to those sites"
            }
            Self::FilesAddOnFolder => "Read and write files in its own folder on your PC",
            Self::Native => "Run as a normal program with full access to your PC",
        }
    }
}

/// The import module every host function lives in.
pub const IMPORT_MODULE: &str = "bri";

/// A name that is not one of the host's functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownFunction;

/// The capability a host function belongs to; `None` for functions every
/// module may use.
pub fn function_capability(name: &str) -> Result<Option<Capability>, UnknownFunction> {
    Ok(match name {
        "log" | "random" => None,
        "mesh_create" | "material_create" | "material_set" | "material_blend"
        | "material_space" | "draw" | "draw_with" | "camera" | "environment" | "view" => {
            Some(Capability::RenderLayer)
        }
        "shader" => Some(Capability::RenderShader),
        "sound_play" | "sound_at" => Some(Capability::Audio),
        "key_down" => Some(Capability::InputFocused),
        "send" | "recv" => Some(Capability::NetMessage),
        "players" | "vehicles" | "entities" | "vehicle_kind" | "archetype_kind" | "image_kind"
        | "state_num" | "local_player" | "life" | "held" | "image_mesh" | "sight" => {
            Some(Capability::WorldRead)
        }
        "rigid_create" | "rigid_joint" | "rigid_remove" | "rigid_push" | "rigid_get"
        | "rigid_find" | "rigid_hold" => Some(Capability::PhysicsLocal),
        "skeleton" | "skeleton_node" | "skeleton_part" | "pose" => Some(Capability::AvatarPose),
        _ => return Err(UnknownFunction),
    })
}
