// Steel Ball: a heavy, perfectly round, polished steel ball. Build a vehicle
// spawn brick and pick the Steel Ball in its wrench, like any vehicle. Each
// player keeps three at most (the kit's `per_player`); a spawn brick past
// that tells its builder so. /clearballs puts yours away until you respawn
// them from the brick's wrench.
//
// The ball is a vehicle from steel-ball-kit: the engine rolls it as a real
// sphere and pushes players and vehicles aside with its weight. Only in
// minigames does it do harm (`harms_only_in_minigames`): a hard hit punches
// through bricks under the same rules a rocket's hit follows, kills the
// players it runs down and wrecks vehicles.

fn cmd_clearballs(p) {
    let ball = get("ball");
    for o in objects() {
        if o.definition == ball && o.owner == p {
            remove_vehicle(o.ref);
        }
    }
}
