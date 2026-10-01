// Player Throwing (Script_PlayerThrowing by Electrk), ported as a host rule.
// Each function names the v20 function it stands for. The values filled in
// below are read from the imported copy's own script.

// $Pref::Server::PlayerThrowing::*, at their defaults (prefs.cs). Times in
// ticks, 120 a second.
fn grab_timeout() { 600 }       // GrabTimeout 5 s
fn escape_timeout() { 360 }     // EscapeTimeout 3 s
fn charge_rate() { 12 }         // ChargeRate 100 ms
fn multiplier() { 2.5 }         // Multiplier
fn charge_throw() { true }      // ChargeThrow
fn throw_amount() { 12 }        // ThrowAmount

// Read from this copy (Player::attemptGrab, throwPlayer, chargeThrow,
// removeFromGrasp, isFrontClear).
fn node() { {{node}} }
fn held_scale() { parse_float("{{held_scale}}") }
fn reach() { parse_float("{{reach}}") }
fn look_up() { parse_float("{{look_up}}") }
fn look_down() { parse_float("{{look_down}}") }
fn min_amount() { {{min_amount}} }
fn max_amount() { {{max_amount}} }
fn max_charge() { {{max_charge}} }
fn front_reach() { parse_float("{{front_reach}}") }
fn down_look() { parse_float("{{down_look}}") }
fn ground() { parse_float("{{ground}}") }
fn held_sequence() { "{{held_sequence}}" }
fn holder_sequence() { "{{holder_sequence}}" }
fn orbit_distance() { parse_float("{{orbit}}") }
fn orbit_nearest() { parse_float("{{orbit_min}}") }
fn orbit_farthest() { parse_float("{{orbit_max}}") }

// The held body's turn on the hand, in degrees clockwise seen from above:
// what Torque makes of `setTransform(pos @ " 0 0 <axis> <angle>")` on a
// mounted player, checked against Torque3D 1.1's source. TypeTransformF
// (mathTypes.cpp) reads the angle as radians; AngAxisF::setMatrix
// (mAngAxis.cpp) builds QuatF (0, 0, axis sin(angle/2), cos(angle/2))
// without normalising the axis; m_quatF_set_matF_C (mMath_C.cpp) gives its
// forward column (2wz, 1 - 2z^2); Player::setTransform (player.cpp) takes
// -atan2(-forward.x, forward.y) as mRot.z.
fn held_turn() {
    let half = parse_float("{{turn_angle}}") / 2.0;
    let z = parse_float("{{turn_axis}}") * half.sin();
    let w = half.cos();
    atan(2.0 * w * z, 1.0 - 2.0 * z * z).to_degrees()
}

// v20 plays these on threads 0 and 1; here the body's action thread (2)
// carries them, layered by their own priority.
fn anim_thread() { 2 }

// Player::isHoldingPlayer: whom `who` carries, or -1.
fn holding(who) {
    let held = get_player(who, "holding");
    if held < 0 {
        return -1;
    }
    let t = player(held);
    if t == () || t.riding != who {
        return -1;
    }
    held
}

// Player::isBeingHeld.
fn being_held(who) {
    let holder = get_player(who, "holder");
    holder >= 0 && player(holder) != ()
}

// Player::isFrontClear(mask): nothing solid within reach where `p` looks
// (bricks and the map; players and vehicles do not count here).
fn front_clear(p) {
    let hit = raycast([p.ex, p.ey, p.ez], [p.lx, p.ly, p.lz], front_reach() * p.scale, p.id);
    hit == () || (hit.kind != "brick" && hit.kind != "map")
}

// Player::removeFromGrasp: let `held` go. True when they may be thrown.
fn remove_from_grasp(held) {
    let h = get_player(held, "holder");
    if h < 0 {
        return false;
    }
    let me = player(held);
    if me != () && me.alive {
        play_thread(held, anim_thread(), "root");
        set_scale(held, get_player(held, "old_scale"));
        set_look_limits(held, ());
        // %this.client.setControlObject(%this)
        orbit_camera(held, ());
    }
    set_player(held, "holder", -1);
    set_player(held, "timeout_at", tick());
    let holder = player(h);
    if holder == () {
        return false;
    }
    let toss = true;
    if me != () && me.riding == h {
        // Off the holder's hand where they stand, or, facing a wall,
        // dropped without a throw.
        if !front_clear(holder) {
            toss = false;
        }
        unmount_object(held);
        // Looking straight down at the floor: set them down in front.
        if holder.ly <= down_look() && holder.y < ground() {
            toss = false;
            let fx = holder.lx;
            let fz = holder.lz;
            let flat = (fx * fx + fz * fz).sqrt();
            if flat > 0.001 {
                teleport(held, holder.x + fx / flat, holder.y, holder.z + fz / flat);
            }
        }
    }
    if holder.alive {
        play_thread(h, anim_thread(), "root");
    }
    set_player(h, "holding", -1);
    set_player(h, "timeout_at", tick());
    set_player(h, "charge_at", -1);
    toss
}

// Player::clearAllHolding.
fn clear_all_holding(who) {
    if get_player(who, "holder") >= 0 {
        remove_from_grasp(who);
    } else {
        let held = get_player(who, "holding");
        if held >= 0 {
            remove_from_grasp(held);
        }
    }
}

// Player::escapeFromGrasp.
fn escape_from_grasp(who) {
    if !being_held(who) {
        return false;
    }
    if tick() - get_player(who, "grabbed_at") < escape_timeout() {
        return false;
    }
    remove_from_grasp(who);
    true
}

// Player::canGrab: a living player (not a bot) in your own minigame, one
// your weapons may hurt, whom nobody holds.
fn can_grab(me, t) {
    if t == () || t.bot || !t.alive {
        return false;
    }
    if t.minigame == () || t.minigame != me.minigame {
        return false;
    }
    if !can_damage(me.id, t.id) {
        return false;
    }
    !being_held(t.id)
}

// Player::attemptGrab.
fn attempt_grab(who) {
    if tick() - get_player(who, "timeout_at") < grab_timeout() {
        return false;
    }
    let me = player(who);
    let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], reach() * me.scale, who);
    if hit == () || hit.kind != "player" {
        return false;
    }
    let t = player(hit.id);
    if !can_grab(me, t) {
        return false;
    }
    if t.mounted {
        unmount_object(t.id);
    }
    set_player(t.id, "old_scale", t.scale);
    unmount_image(t.id);
    mount_object(who, t.id, node(), false, held_turn());
    set_scale(t.id, me.scale * held_scale());
    play_thread(t.id, anim_thread(), held_sequence());
    set_look_limits(t.id, look_up(), look_down());
    // The "Grabbed" camera: orbiting the holder; its click struggles
    // (Observer::onTrigger), which reaches on_activate below.
    orbit_camera(t.id, who, orbit_nearest(), orbit_farthest(), orbit_distance());
    play_thread(who, anim_thread(), holder_sequence());
    set_player(t.id, "holder", who);
    set_player(t.id, "grabbed_at", tick());
    set_player(who, "holding", t.id);
    set_player(who, "timeout_at", tick());
    true
}

// Player::throwPlayer.
fn throw_player(who, amount) {
    let held = holding(who);
    if held < 0 || being_held(who) {
        return;
    }
    let amount = if amount < min_amount() { min_amount() } else if amount > max_amount() { max_amount() } else { amount };
    let me = player(who);
    if remove_from_grasp(held) {
        let speed = amount * multiplier();
        push("player:" + held, me.lx * speed, me.ly * speed, me.lz * speed, who);
    }
}

// Player::chargeThrow: the charge so far, one notch per ChargeRate, as
// %chargeThrowAmount stands after each scheduled call.
fn charge(who) {
    let at = get_player(who, "charge_at");
    if at < 0 {
        return 0;
    }
    let notches = (tick() - at) / charge_rate() + 2;
    if notches > max_charge() + 1 { max_charge() + 1 } else { notches }
}

// The bar chargeThrow prints.
fn charge_bar(who) {
    let at = get_player(who, "charge_at");
    let shown = (tick() - at) / charge_rate() + 1;
    let shown = if shown > max_charge() { max_charge() } else { shown };
    let bars = "";
    for i in 0..max_charge() {
        if i == shown && shown < max_charge() {
            bars += "<color:000000>";
        }
        bars += "|";
    }
    let colour = if shown < max_charge() { "<color:FFFF00>" } else { "<color:00FF00>" };
    bottom_print(who, "<just:center><color:FF0000>Throw! <color:FFFFFF>:" + colour + bars, 1.0);
}

// Armor::onTrigger, fire with nothing in hand: charge while held down,
// throw on letting go.
fn on_trigger(who, trigger, down) {
    if trigger != 0 || holding(who) < 0 {
        return ();
    }
    if down {
        if charge_throw() {
            set_player(who, "charge_at", tick());
            charge_bar(who);
        } else {
            throw_player(who, throw_amount());
        }
        return true;
    }
    let amount = charge(who);
    if amount > 0 {
        throw_player(who, amount);
        return true;
    }
    ()
}

// Player::activateStuff, and Observer::onTrigger in "Grabbed" mode: the
// held player's click struggles.
fn on_activate(who) {
    if being_held(who) {
        if escape_from_grasp(who) {
            return true;
        }
        return ();
    }
    if holding(who) >= 0 {
        return true;
    }
    if attempt_grab(who) {
        return true;
    }
    ()
}

// GameConnection::onDeath, Armor::onRemove.
fn on_death(victim, killer) {
    clear_all_holding(victim);
}
fn on_leave(who) {
    clear_all_holding(who);
}

// serverCmdUseTool, UnUseTool, UseSprayCan, UseFXCan
// (PlayerThrowing_CanUseTools).
fn allow_equip(who) {
    !(being_held(who) || holding(who) >= 0)
}

fn on_tick() {
    for p in players() {
        // chargeThrow's bar, every ChargeRate.
        if get_player(p.id, "charge_at") >= 0 && holding(p.id) >= 0 {
            charge_bar(p.id);
        }
        // Whoever no longer carries the player they held (taken off by
        // something else) lets go.
        let held = get_player(p.id, "holding");
        if held >= 0 && holding(p.id) < 0 {
            remove_from_grasp(held);
        }
    }
}
