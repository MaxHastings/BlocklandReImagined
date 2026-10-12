//! Player archetypes: what a player is, as data both sides share.
pub use bri_motor::archetype::*;
pub use bri_motor::controller::{ControllerSource, ControllerState};

/// Make an archetype table's movement controllers run (each side binds the
/// table it moves players by: the host when its Add-Ons load, a client
/// when the host's table arrives). Returns the archetypes whose module was
/// refused, with why; they move by the stock motor.
pub fn bind_controllers(archetypes: &mut Archetypes) -> Vec<(String, anyhow::Error)> {
    bri_controller::bind(archetypes)
}
