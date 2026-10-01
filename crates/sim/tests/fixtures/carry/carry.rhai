// Test fixture for the riding seams (never shipped): with nothing in your
// hand, a click on a player within two body lengths lifts them onto your
// left hand; the next click throws them where you look.
//
// - the target must be no bigger than you and a Blockhead, and either in
//   a minigame whose weapons may hurt them, or a bot from your own brick;
// - the holder becomes a No-Jet player and raises their arms;
// - the held player shrinks to 0.6, goes limp (`death1`), holds nothing,
//   cannot jump off, and rides mount point 1;
// - a throw gives them the look direction times 50, credited to the
//   thrower, and puts both back as they were.

// Mount point 1, the left hand; the throw's speed; the held player's size;
// the holder's body while carrying.
fn hand() { 1 }
fn throw_speed() { 50.0 }
fn held_scale() { 0.6 }
fn no_jet() { "v20.player.playernojet" }

fn blockhead(p) {
    p.archetype.starts_with("v20.player.") && p.archetype != "v20.player.horsearmor"
}

// The held player as they were: full size, free look, arms down.
fn set_free(held) {
    let t = player(held);
    if t == () {
        return;
    }
    set_scale(held, 1.0);
    set_look_limits(held, ());
    if t.alive {
        play_thread(held, 2, "root");
    }
}

// The holder as they were.
fn put_down(who) {
    let me = player(who);
    set_player(who, "holding", -1);
    if me == () {
        return;
    }
    let body = get_player(who, "body");
    if body != "" && me.alive {
        set_archetype(who, body);
        // Their body, not a choice kept across respawns.
        set_archetype(who, "");
    }
    set_player(who, "body", "");
    play_thread(who, 2, "root");
}

fn fling(who, held) {
    let me = player(who);
    let t = player(held);
    if t != () && t.riding == who {
        unmount_object(held);
        push("player:" + held, me.lx * throw_speed(), me.ly * throw_speed(), me.lz * throw_speed(), who);
    }
    set_free(held);
    put_down(who);
}

fn grab(who, me) {
    let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], 2.0 * me.scale, who);
    if hit == () || hit.kind != "player" {
        return false;
    }
    let t = player(hit.id);
    if t == () || !t.alive || t.mounted || t.scale > me.scale {
        return false;
    }
    if !blockhead(t) || !blockhead(me) {
        return false;
    }
    let proper = (t.minigame != () && can_damage(who, t.id))
        || (t.bot && t.bot_owner == who);
    if !proper {
        return false;
    }
    set_player(who, "holding", t.id);
    set_player(who, "body", me.archetype);
    set_archetype(who, no_jet());
    set_archetype(who, "");
    set_scale(t.id, held_scale());
    set_look_limits(t.id, 0.5, 0.5);
    unmount_image(t.id);
    mount_object(who, t.id, hand(), false);
    play_thread(t.id, 2, "death1");
    play_thread(who, 2, if t.scale < 0.5 { "armreadyleft" } else { "armreadyboth" });
    true
}

fn on_activate(who) {
    let me = player(who);
    if me == () || !me.alive {
        return ();
    }
    let held = get_player(who, "holding");
    if held >= 0 {
        fling(who, held);
        return true;
    }
    if me.mounted {
        return ();
    }
    if grab(who, me) {
        return true;
    }
    ()
}

// Whoever lost what they carried (it died, left, or was taken off) puts
// their arms down and gets their body back; whoever was dropped is set
// free.
fn on_tick() {
    for p in players() {
        let held = get_player(p.id, "holding");
        if held < 0 {
            continue;
        }
        let t = player(held);
        if t == () || t.riding != p.id || !p.alive {
            set_free(held);
            put_down(p.id);
        }
    }
}

// A new body carries no one.
fn on_spawn(who) {
    let held = get_player(who, "holding");
    if held >= 0 {
        set_free(held);
    }
    set_player(who, "holding", -1);
    set_player(who, "body", "");
}

fn on_leave(who) {
    let held = get_player(who, "holding");
    if held >= 0 {
        set_free(held);
    }
}
