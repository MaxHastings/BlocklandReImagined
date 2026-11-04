//! `on_vehicle_lights`: a driver turns its vehicle's lamps on or off with the
//! light key and the rules hear it at once - an Add-On that plays the click of
//! the switch, dims a garage map or logs the state needs nothing else from the
//! engine. A game without the hook never pays for it.
use super::game_hooks::declaring;
use super::*;

impl Session {
    /// Tell `on_vehicle_lights` hooks that `driver` turned `vehicle`'s lamps
    /// to `lights`.
    pub(in crate::session) fn deliver_vehicle_lights(
        &mut self,
        driver: OwnerId,
        vehicle: u64,
        lights: bool,
    ) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks = declaring(host, |b| b.on_vehicle_lights);
        if hooks.is_empty() {
            return;
        }
        for package in &hooks {
            let _ = self.run_package(
                package,
                "on_vehicle_lights",
                vec![
                    Dynamic::from_int(driver as i64),
                    ObjectRef::Vehicle(vehicle).to_string().into(),
                    Dynamic::from_bool(lights),
                ],
                Budget::Command,
                None,
                None,
                None,
            );
            self.charge_work(package);
        }
    }
}
