fn on_vehicle_lights(driver, vehicle, lights) {
    let log = get("log");
    log.push(`${driver} ${vehicle} ${lights}`);
    set("log", log);
}
