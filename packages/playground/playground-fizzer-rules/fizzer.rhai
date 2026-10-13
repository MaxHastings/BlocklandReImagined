// The Fizzer's pop. The shared bot brain walked it up to its target and
// burned its wick (bots.json's `ability`); this is what happens when the
// wick runs out: a blast at its feet, as its own shot, so the game's rules
// decide who it hurts and whether bricks break, and the Fizzer is gone.

fn kind() { "playground-fizzer:bot/fizzer" }

fn on_bot_ability(bot, kind, ability, target) {
    if kind != kind() { return; }
    let me = player(bot);
    if me == () || !me.alive { return; }
    fire("playground-fizzer:projectile/pop", me.x, me.y + 0.3, me.z, 0.0, -40.0, 0.0, bot);
    damage(bot, 1000.0, bot, "FizzerPop");
}
