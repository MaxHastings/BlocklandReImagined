fn on_tick() {
    if get("started") { return; }
    for bed in map_beds() {
        let options = #{fade:0.25};
        if bed.at != () { options.at = bed.at; }
        play_ambient_all(bed.key,bed.track,options);
    }
    set("started",true);
}
