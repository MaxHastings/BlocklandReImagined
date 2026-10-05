//! Coordination (`docs/architecture/bots.md`, "Coordination"). Every bot
//! publishes its current choice as an intent beside the claims: where it
//! goes, what it acts on, a vehicle whose free seats it controls, and the
//! line its weapon will hit. Each bot then reads its allies' intents into
//! its own scores through two terms: overlap (the same target, or a place
//! close to theirs, costs more) and interaction (taking a seat they expose
//! scores more, a place their weapon will hit costs more). A third, mood,
//! raises the chance of an idle flavour as nearby bots of either side are
//! doing one, and of the same one. Nothing here chooses: the brain's
//! chooser, with its surprise and commitments, still picks.
use super::claims::{Intent, Sightline, Space, Target};
use super::*;
use crate::bot_kind::BotTeam;

/// Two places this close (a few body widths) are one spot to crowd.
const CROWD: f32 = 4.0;
/// At most one callout this often, in seconds, as a person would.
const CALLOUT_SECONDS: f32 = 30.0;
/// About how often, in ticks, a bot looks round for the mood: a second,
/// on its own staggered beat (`cadence`).
const MOOD_TICKS: u64 = 120;

/// What one of a bot's options would do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Choice {
    pub place: Option<Vec3>,
    /// Whether it stays at `place` while it does this (a fight's stance)
    /// rather than heading there. Only a stance can stand in an ally's line
    /// of fire; on the way, the ally holds fire (`bot_fire_clear`).
    pub stand: bool,
    pub target: Option<Target>,
    /// The vehicle whose seat it would take.
    pub seat: Option<u64>,
    /// The vehicle it drives there, and that vehicle's origin from its place.
    pub carries: Option<(u64, Vec3)>,
}

/// What allies' intents add to one option's score.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Terms {
    pub overlap: f32,
    pub uses: f32,
    pub harm: f32,
}
impl Terms {
    pub(super) fn total(self) -> f32 {
        self.uses - self.overlap - self.harm
    }
}

/// A bot's coordination state: the terms of its last choice and when it
/// may call out again.
#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub terms: [Terms; Behaviour::COUNT],
    /// The mood as last looked at (`Session::team_mood_now`): the pull
    /// toward any idle flavour and toward each one. Whatever scores a
    /// flavour reads it from here.
    pub mood: Option<(f32, [f32; 11])>,
    pub allies: usize,
    pub next_callout: u64,
    pub said: Option<String>,
}

/// Whether a body standing at `feet`, `body` high, is in `space`.
fn inside(space: &Space, feet: Vec3, body: f32) -> bool {
    space.holds(feet + Vec3::Y * body * 0.5, body * 0.5)
}

/// The terms allies' intents add to `choice`, `me`'s option `option`,
/// which it holds `since`. Only an ally who took its target or place up
/// first makes an overlap, so the later of two gives way and the first
/// keeps it; a place is crowded only by an ally doing the same there.
/// `clear` says whether nothing solid stands between two points.
pub(super) fn terms(
    cfg: &BotTeam,
    me: OwnerId,
    (option, since): (u8, u64),
    choice: Choice,
    allies: &[(OwnerId, Intent)],
    body: f32,
    clear: &dyn Fn(Vec3, Vec3) -> bool,
) -> Terms {
    let mut t = Terms::default();
    for (ally, intent) in allies {
        // A seat is leased exclusively (`claims`), so taking one never
        // crowds the allies around it.
        if (intent.since, *ally) < (since, me) {
            if choice.target.is_some() && choice.target == intent.target {
                t.overlap += cfg.overlap();
            } else if let (Some(a), Some(b), None) = (choice.place, intent.place, choice.seat)
                && intent.option == option
                && a.distance(b) < CROWD
            {
                t.overlap += cfg.overlap() * (1.0 - a.distance(b) / CROWD);
            }
        }
        if choice.seat.is_some() && choice.seat == intent.seats {
            t.uses += cfg.uses();
        }
        // Taking an ally's mount where it sees what it is after.
        if let (Some(at), Some((vehicle, origin)), Some(s)) =
            (choice.place, choice.carries, intent.sight)
            && vehicle == s.vehicle
            && clear(at + origin + s.offset, s.to)
        {
            t.uses += cfg.uses();
        }
        if let (Some(at), Some(harm), true) = (choice.place, intent.harm.as_ref(), choice.stand)
            && inside(harm, at, body)
        {
            t.harm += cfg.harm();
        }
    }
    t
}

/// The nearest place to `feet` that no ally's weapon will hit, when it
/// stands where one will, and that has floor to stand on (`floor`): out of
/// a line by its nearer side, else by the other; none when both sides of
/// it are a drop.
pub(super) fn exit(
    allies: &[(OwnerId, Intent)],
    feet: Vec3,
    body: f32,
    floor: &dyn Fn(Vec3) -> bool,
) -> Option<Vec3> {
    let mut at = feet;
    for _ in 0..3 {
        let Some(h) = allies
            .iter()
            .filter_map(|(_, i)| i.harm)
            .find(|h| inside(h, at, body))
        else {
            return (at != feet).then_some(at);
        };
        let line = flat(h.to - h.from);
        let along = flat(at - h.from).dot(line) / line.length_squared().max(1e-6);
        let nearest = h.from + (h.to - h.from) * along.clamp(0.0, 1.0);
        let away = flat(at - nearest)
            .try_normalize()
            .unwrap_or_else(|| Vec3::new(-line.z, 0.0, line.x).normalize_or(Vec3::X));
        let wide = h.radius + h.spread * (nearest - h.from).length();
        let side = |away: Vec3| {
            let out = flat(nearest) + away * (wide + body + 0.5);
            Some(Vec3::new(out.x, feet.y, out.z)).filter(|at| floor(*at))
        };
        at = side(away).or_else(|| side(-away))?;
    }
    None
}

/// Adjust each live option's score by allies' intents. An option keeps a
/// score above zero: what the brain could do stays an option.
#[allow(clippy::too_many_arguments)]
pub(super) fn adjust(
    cfg: &BotTeam,
    me: OwnerId,
    sinces: impl Fn(usize) -> u64,
    scores: &mut [f32; Behaviour::COUNT],
    choices: &[Choice; Behaviour::COUNT],
    allies: &[(OwnerId, Intent)],
    body: f32,
    clear: &dyn Fn(Vec3, Vec3) -> bool,
) -> [Terms; Behaviour::COUNT] {
    let mut all = [Terms::default(); Behaviour::COUNT];
    for (b, score) in scores.iter_mut().enumerate() {
        if *score > 0.0 {
            all[b] = terms(
                cfg,
                me,
                (b as u8, sinces(b)),
                choices[b],
                allies,
                body,
                clear,
            );
            *score = (*score + all[b].total()).max(f32::MIN_POSITIVE);
        }
    }
    all
}

/// The callout for a choice the terms changed from `before` to `after`:
/// the template of whichever term moved it most.
pub(super) fn callout<'a>(
    cfg: &'a BotTeam,
    terms: &[Terms; Behaviour::COUNT],
    before: usize,
    after: usize,
) -> Option<&'a str> {
    let pulls = [
        ("overlap", terms[before].overlap - terms[after].overlap),
        ("uses", terms[after].uses - terms[before].uses),
        ("harm", terms[before].harm - terms[after].harm),
    ];
    let (term, by) = pulls.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
    (by > 0.0).then(|| cfg.callouts.get(term).map(String::as_str))?
}

/// What a player it sees is visibly doing, for the mood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Doing {
    Idle,
    /// Going about the game: moving with purpose, firing, holding something.
    Play,
    /// An idle flavour (`surprise::Flavour` index).
    Goof(u8),
}

/// How much more likely an idle flavour is, and each one, from players it
/// sees within `radius` (its sight) of `at`, of either side: `mood` times
/// the share goofing less the share playing, and times the share doing
/// each flavour, both at most `mood_cap`. Each counts by its weight (a
/// person `mood_human`, a bot 1); one out of sight counts not at all.
pub(super) fn mood(
    cfg: &BotTeam,
    me: OwnerId,
    at: Vec3,
    radius: f32,
    others: impl Iterator<Item = (OwnerId, Vec3, Doing, f32, bool)>,
) -> (f32, [f32; 11]) {
    let (mut near, mut play) = (0.0f32, 0.0f32);
    let mut doing = [0.0f32; 11];
    for (who, feet, what, weight, seen) in others {
        if who != me && seen && feet.distance(at) < radius {
            near += weight;
            match what {
                Doing::Goof(f) if usize::from(f) < 11 => doing[usize::from(f)] += weight,
                Doing::Play => play += weight,
                _ => {}
            }
        }
    }
    if near <= 0.0 {
        return (0.0, [0.0; 11]);
    }
    let pull = |n: f32| (cfg.mood * n.max(0.0) / near).min(cfg.mood_cap);
    (pull(doing.iter().sum::<f32>() - play), doing.map(pull))
}

/// What a teammate's success is worth to a bot that `seen` it: `copy`, and
/// nothing out of sight.
pub(super) fn copied(cfg: &BotTeam, seen: bool) -> f32 {
    if seen { cfg.copy } else { 0.0 }
}

/// Why a bot's last choice moved, for the why-view: how many allies'
/// intents it read, each option's terms that are not zero (behaviour,
/// overlap, uses, harm), and its last callout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotTeamView {
    pub allies: usize,
    pub terms: Vec<(&'static str, f32, f32, f32)>,
    pub said: Option<String>,
}
impl State {
    pub(super) fn view(&self) -> BotTeamView {
        BotTeamView {
            allies: self.allies,
            terms: Behaviour::ALL
                .into_iter()
                .zip(self.terms)
                .filter(|(_, t)| *t != Terms::default())
                .map(|(b, t)| (b.name(), t.overlap, t.uses, t.harm))
                .collect(),
            said: self.said.clone(),
        }
    }
}

impl Session {
    /// Allies' live intents, read by `bot`. The crew of one vehicle act as
    /// one: a crewmate neither crowds its target or spot nor endangers it.
    pub(super) fn team_intents(&self, bot: OwnerId, tick: u64) -> Vec<(OwnerId, Intent)> {
        let vehicle = self.mounted(bot).map(|(v, _)| v);
        self.bots
            .claims
            .intents(tick)
            .filter(|(o, _)| *o != bot && self.bot_allies(bot, *o))
            .map(|(o, mut i)| {
                if vehicle.is_some() && i.mount == vehicle {
                    (i.harm, i.target, i.place) = (None, None, None);
                }
                (o, i)
            })
            .collect()
    }

    /// How many of `bot`'s allies, crew of its vehicle aside, went after
    /// `enemy` before it did. As with every overlap the first keeps it:
    /// counting later ones too made two bots on one enemy both give way,
    /// then both come back.
    pub(super) fn team_crowd(&self, bot: OwnerId, enemy: OwnerId) -> f32 {
        let tick = self.simulation.state().tick;
        let target = Some(Target::Player(enemy));
        let mine = self
            .bots
            .claims
            .intents(tick)
            .find(|(o, i)| *o == bot && i.target == target)
            .map_or(tick, |(_, i)| i.since);
        // As `team_intents`, but the cheap target test first: this runs for
        // each enemy in sight, and most intents are on something else.
        let vehicle = self.mounted(bot).map(|(v, _)| v);
        self.bots
            .claims
            .intents(tick)
            .filter(|(o, i)| i.target == target && (i.since, *o) < (mine, bot))
            .filter(|(_, i)| vehicle.is_none() || i.mount != vehicle)
            .filter(|(o, _)| *o != bot && self.bot_allies(bot, *o))
            .count() as f32
    }

    /// How crowded `at` is for `bot`'s `option`: the overlap of allies
    /// that took up the same option there before it did. A bot choosing
    /// among places for one option (an item to arm with) prefers one nobody
    /// crowds, so crowding moves it to another rather than off the option.
    pub(super) fn team_place_crowd(&self, bot: OwnerId, option: Behaviour, at: Vec3) -> f32 {
        let tick = self.simulation.state().tick;
        let Some(brain) = self.bots.brains.get(&bot) else {
            return 0.0;
        };
        let since = self.bots.claims.held_since(bot, option as u8, None, tick);
        let choice = Choice {
            place: Some(at),
            ..Default::default()
        };
        let allies = self.team_intents(bot, tick);
        let open = |_: Vec3, _: Vec3| true;
        terms(
            &brain.kind.team,
            bot,
            (option as u8, since),
            choice,
            &allies,
            0.0,
            &open,
        )
        .overlap
    }

    /// How far `bot`'s team trails the best other team of its game, from 0
    /// (level or ahead) toward 1: points behind over one more than that.
    pub(super) fn team_deficit(&self, bot: OwnerId) -> f32 {
        let Some(player) = self
            .peers
            .get(&bot)
            .and_then(|p| self.minigames.player(p.combat.player).ok())
        else {
            return 0.0;
        };
        let (Some(game), Some(team)) = (player.game, player.team) else {
            return 0.0;
        };
        let Ok(g) = self.minigames.game(game) else {
            return 0.0;
        };
        let score = |t| self.minigames.team_score(game, t).unwrap_or(0);
        let ours = score(team);
        let best = g
            .teams
            .list
            .iter()
            .filter(|t| !g.teams.allied(team, t.id))
            .map(|t| score(t.id))
            .max()
            .unwrap_or(ours);
        let behind = best.saturating_sub(ours).max(0) as f32;
        behind / (behind + 1.0)
    }

    /// The vehicle whose controls `bot` holds while one of its seats is free
    /// and it would wait for crew: still at rest, within the driver's wait
    /// for crew (`interactions::CREW_WAIT`). A seat on a vehicle already
    /// under way is no offer.
    pub(super) fn team_seats(&self, bot: OwnerId, tick: u64) -> Option<u64> {
        let (vehicle, seat) = self.mounted(bot)?;
        let w = self.vehicles.world.as_ref()?;
        let d = w.definition_of(bri_vehicles::VehicleId(vehicle))?;
        let v = self.bots.objects.iter().find(|v| v.id.0 == vehicle)?;
        let waits = self
            .bots
            .brains
            .get(&bot)?
            .vehicle_since
            .is_none_or(|(id, since)| id != vehicle || tick < since + interactions::CREW_WAIT);
        (waits
            && Vec3::from(v.velocity).length() < 2.0
            && d.seats.get(usize::from(seat))?.controls
            && (0..d.seats.len()).any(|s| {
                w.seat_occupant(bri_vehicles::VehicleId(vehicle), s)
                    .is_none()
            }))
        .then_some(vehicle)
    }

    /// The mood pull on `bot`, with the surprise and the mood on: looked at
    /// afresh on the bot's own beat, about every `MOOD_TICKS`, while nothing
    /// threatens it, and kept between (and while it is under threat), so a
    /// crowd costs a few rays a bot each second, not each tick, and a match
    /// keeps a mood for a flavour to be scored by.
    pub(super) fn team_mood_now(
        &mut self,
        bot: OwnerId,
        (at, eye): (Vec3, Vec3),
        threatened: bool,
        tick: u64,
    ) -> (f32, [f32; 11]) {
        let Some(brain) = self.bots.brains.get(&bot) else {
            return (0.0, [0.0; 11]);
        };
        if brain.kind.team.mood <= 0.0 || brain.kind.surprise.strength <= 0.0 {
            self.bots.brains.get_mut(&bot).unwrap().team.mood = None;
            return (0.0, [0.0; 11]);
        }
        if threatened {
            return brain.team.mood.unwrap_or_default();
        }
        if let Some(mood) = brain.team.mood
            && !cadence::beat(bot, cadence::salt::MOOD, tick, MOOD_TICKS)
        {
            return mood;
        }
        let mood = self.team_mood(bot, at, eye, tick);
        self.bots.brains.get_mut(&bot).unwrap().team.mood = Some(mood);
        mood
    }

    /// The mood pull on `bot` at `at`, its eye at `eye`: each bot's
    /// published flavour or work, and a person's visible goof (an emote in
    /// the last `EMOTED` ticks) or play (firing, holding something, moving
    /// faster than `PURPOSE`); only those it has in sight count.
    pub(super) fn team_mood(
        &self,
        bot: OwnerId,
        at: Vec3,
        eye: Vec3,
        tick: u64,
    ) -> (f32, [f32; 11]) {
        const EMOTED: u64 = 240;
        /// Faster than a stroll, in world units a second: going somewhere.
        const PURPOSE: f32 = 3.0;
        let Some(brain) = self.bots.brains.get(&bot) else {
            return (0.0, [0.0; 11]);
        };
        let cfg = &brain.kind.team;
        let radius = brain.kind.sight;
        let intents: BTreeMap<_, _> = self.bots.claims.intents(tick).collect();
        let others = self.peers.iter().filter_map(|(o, p)| {
            let feet = Vec3::from(p.player.state().feet);
            if !p.combat.alive || *o == bot || feet.distance(at) >= radius {
                return None;
            }
            let seen = self
                .bot_sees_player(bot, *o, eye, radius, SightUrgency::Ordinary)
                .is_some();
            if self.bots.is_bot(*o) {
                let i = intents.get(o)?;
                let what = match i.flavour {
                    Some(f) => Doing::Goof(f),
                    None if i.option != Behaviour::Wander as u8 => Doing::Play,
                    None => Doing::Idle,
                };
                return Some((*o, feet, what, 1.0, seen));
            }
            let emoted = p
                .combat
                .voice
                .is_some_and(|t| tick.saturating_sub(t) < EMOTED);
            let moving = flat(Vec3::from(p.player.state().velocity)).length() > PURPOSE;
            let firing = self
                .weapons
                .actor(ActorId(*o))
                .is_some_and(|a| a.trigger_held());
            let what = if emoted {
                Doing::Goof(1)
            } else if moving || firing || self.held_by(*o).is_some() {
                Doing::Play
            } else {
                Doing::Idle
            };
            Some((*o, feet, what, cfg.mood_human, seen))
        });
        mood(cfg, bot, at, radius, others)
    }

    /// Allies that see `bot` and its `worked` options succeed take a liking
    /// to the same options (`copy`, fading as the enemy adapts).
    pub(super) fn team_copy(
        &mut self,
        bot: OwnerId,
        worked: &[(surprise::Domain, u32)],
        tick: u64,
    ) {
        if worked.is_empty() || !self.peers.contains_key(&bot) {
            return;
        }
        let watchers: Vec<(OwnerId, f32)> = self
            .bots
            .brains
            .iter()
            .filter(|(o, _)| **o != bot && self.bot_allies(bot, **o))
            .filter_map(|(o, brain)| {
                let p = self.peers.get(o).filter(|p| p.combat.alive)?;
                let from = p.player.eye();
                let seen = self
                    .bot_sees_player(*o, bot, from, brain.kind.sight, SightUrgency::Ordinary)
                    .is_some();
                Some((*o, copied(&brain.kind.team, seen)))
            })
            .filter(|(_, copy)| *copy > 0.0)
            .collect();
        for (o, copy) in watchers {
            let brain = self.bots.brains.get_mut(&o).unwrap();
            for (domain, option) in worked {
                brain.surprise.saw(*domain, *option, copy, tick);
            }
        }
    }

    /// What a seated bot needs from whoever drives it: a line from its
    /// mount (`eye`) to what it is after.
    pub(super) fn team_sightline(
        &self,
        bot: OwnerId,
        eye: Vec3,
        to: Option<Vec3>,
    ) -> Option<Sightline> {
        let (vehicle, _) = self.mounted(bot)?;
        let v = self.bots.objects.iter().find(|v| v.id.0 == vehicle)?;
        Some(Sightline {
            vehicle,
            offset: eye - Vec3::from(v.transform.position),
            to: to?,
        })
    }

    /// The vehicle `bot` drives, and its origin from the bot's `feet`.
    pub(super) fn team_carries(&self, bot: OwnerId, feet: Vec3) -> Option<(u64, Vec3)> {
        let (vehicle, _) = self.mounted(bot)?;
        let v = self.bots.objects.iter().find(|v| v.id.0 == vehicle)?;
        Some((vehicle, Vec3::from(v.transform.position) - feet))
    }

    /// Say a callout in team chat, as a player would type it.
    pub(super) fn team_say(&mut self, bot: OwnerId, line: String, tick: u64) -> Result<()> {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return Ok(());
        };
        brain.team.next_callout = tick + (CALLOUT_SECONDS * 120.0) as u64;
        brain.team.said = Some(line.clone());
        let sequence = brain.sequence;
        let _ = self.command(bot, sequence, Command::TeamChat(line));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> BotTeam {
        BotTeam {
            teamwork: 1.0,
            callouts: [("harm".to_string(), "Moving!".to_string())].into(),
            ..Default::default()
        }
    }

    fn intent(since: u64) -> Intent {
        Intent {
            option: 0,
            since,
            place: None,
            target: None,
            seats: None,
            harm: None,
            mount: None,
            sight: None,
            flavour: None,
            until: u64::MAX,
        }
    }

    fn open(_: Vec3, _: Vec3) -> bool {
        true
    }

    fn at(x: f32, z: f32) -> Choice {
        Choice {
            place: Some(Vec3::new(x, 0.0, z)),
            stand: true,
            ..Default::default()
        }
    }

    fn shooter() -> Intent {
        Intent {
            harm: Some(Space {
                from: Vec3::ZERO,
                to: Vec3::new(0.0, 0.0, -20.0),
                radius: 3.0,
                spread: 0.0,
            }),
            ..intent(0)
        }
    }

    #[test]
    fn overlap_with_an_earlier_allys_target_or_place_costs_more() {
        let cfg = cfg();
        let x = Target::Player(9);
        let ally = Intent {
            target: Some(x),
            place: Some(Vec3::ZERO),
            ..intent(10)
        };
        let same = Choice {
            target: Some(x),
            ..Default::default()
        };
        let cost = |allies: &[(OwnerId, Intent)], since| {
            terms(&cfg, 2, (0, since), same, allies, 2.0, &open).overlap
        };
        assert_eq!(cost(&[(1, ally)], 20), cfg.overlap());
        assert!(
            cost(&[(1, ally), (3, ally)], 20) > cfg.overlap(),
            "each ally on it adds"
        );
        assert_eq!(
            cost(&[(1, ally)], 5),
            0.0,
            "the one who took it first keeps it"
        );
        let near = |d: f32, option| {
            terms(&cfg, 2, (option, 20), at(d, 0.0), &[(1, ally)], 2.0, &open).overlap
        };
        assert!(near(1.0, 0) > near(3.0, 0) && near(3.0, 0) > 0.0 && near(5.0, 0) == 0.0);
        assert_eq!(near(1.0, 1), 0.0, "an ally doing something else there");
    }

    #[test]
    fn an_exposed_seat_scores_more_and_a_harm_volume_costs() {
        let cfg = cfg();
        let driver = Intent {
            seats: Some(44),
            ..intent(0)
        };
        let seat = |vehicle| Choice {
            seat: Some(vehicle),
            ..Default::default()
        };
        assert_eq!(
            terms(&cfg, 2, (0, 0), seat(44), &[(1, driver)], 2.0, &open).uses,
            cfg.uses()
        );
        assert_eq!(
            terms(&cfg, 2, (0, 0), seat(45), &[(1, driver)], 2.0, &open).uses,
            0.0
        );
        let harm = |c| terms(&cfg, 2, (0, 0), c, &[(1, shooter())], 2.0, &open).harm;
        // On the line, and in its blast radius at the end: cost. Beside: none.
        assert_eq!(harm(at(0.5, -10.0)), cfg.harm());
        assert_eq!(harm(at(2.0, -21.0)), cfg.harm());
        assert_eq!(harm(at(8.0, -10.0)), 0.0);
        let ground = |_: Vec3| true;
        let out = exit(&[(1, shooter())], Vec3::new(0.5, 0.0, -10.0), 2.0, &ground).unwrap();
        assert!(!inside(&shooter().harm.unwrap(), out, 2.0) && out.x > 0.5);
        assert_eq!(
            exit(&[(1, shooter())], Vec3::new(8.0, 0.0, -10.0), 2.0, &ground),
            None
        );
    }

    #[test]
    fn a_way_out_of_a_line_of_fire_is_never_off_the_floor() {
        // Standing on the line by its +x side, on a deck that ends at
        // x = 1: the nearer way out (+x) is a drop, so it goes out by -x.
        let deck = |at: Vec3| at.x < 1.0;
        let out = exit(&[(1, shooter())], Vec3::new(0.5, 0.0, -10.0), 2.0, &deck).unwrap();
        assert!(
            !inside(&shooter().harm.unwrap(), out, 2.0) && out.x < 0.0,
            "{out}"
        );
        // No floor either side: it stays where it is.
        let ledge = |_: Vec3| false;
        assert_eq!(
            exit(&[(1, shooter())], Vec3::new(0.5, 0.0, -10.0), 2.0, &ledge),
            None
        );
    }

    #[test]
    fn a_driver_scores_places_that_give_its_seated_ally_a_line_of_sight() {
        let cfg = cfg();
        // The gunner's mount is 2 up from the vehicle's origin; a wall
        // stands between x < 0 and the target at the origin's far side.
        let gunner = Intent {
            sight: Some(Sightline {
                vehicle: 7,
                offset: Vec3::Y * 2.0,
                to: Vec3::new(0.0, 1.0, -30.0),
            }),
            ..intent(0)
        };
        let wall = |from: Vec3, _: Vec3| from.x >= 0.0;
        let drive = |x: f32, vehicle: u64| Choice {
            carries: Some((vehicle, Vec3::Y * 0.5)),
            ..at(x, 0.0)
        };
        let uses = |c| terms(&cfg, 2, (0, 0), c, &[(1, gunner)], 2.0, &wall).uses;
        assert_eq!(
            uses(drive(4.0, 7)),
            cfg.uses(),
            "a place the mount sees from"
        );
        assert_eq!(uses(drive(-4.0, 7)), 0.0, "a place behind the wall");
        assert_eq!(uses(drive(4.0, 8)), 0.0, "another vehicle's mount");
        assert_eq!(uses(at(4.0, 0.0)), 0.0, "on foot it carries no mount");
    }

    #[test]
    fn a_clearly_better_own_option_still_wins_and_options_stay_options() {
        let cfg = cfg();
        let mut choices = [Choice::default(); Behaviour::COUNT];
        choices[Behaviour::Fight as usize] = at(0.0, -10.0);
        let pick = |fight: f32| {
            let mut scores = [0.0; Behaviour::COUNT];
            scores[Behaviour::Fight as usize] = fight;
            scores[Behaviour::Wander as usize] = 0.1;
            let all = [(1, shooter())];
            adjust(&cfg, 2, |_| 0, &mut scores, &choices, &all, 2.0, &open);
            (behaviour::best(&scores), scores)
        };
        assert_eq!(pick(1.0).0, Behaviour::Fight, "0.4 still beats 0.1");
        let (b, scores) = pick(0.65);
        assert_eq!(b, Behaviour::Wander);
        assert!(
            scores[Behaviour::Fight as usize] > 0.0,
            "a costed option stays one"
        );
        assert_eq!(scores[0], 0.0, "no option is made up");
    }

    #[test]
    fn the_surprise_band_and_hold_still_apply_to_adjusted_scores() {
        let team = cfg();
        let surprise = crate::bot_kind::BotSurprise {
            strength: 1.0,
            ..Default::default()
        };
        // An ally on the same target costs Fight into Chase's band: over
        // many seeds the chooser takes each sometimes, and holds its pick.
        let ally = Intent {
            target: Some(Target::Player(9)),
            ..intent(0)
        };
        let mut choices = [Choice::default(); Behaviour::COUNT];
        choices[Behaviour::Fight as usize].target = Some(Target::Player(9));
        let (mut fights, mut chases) = (0, 0);
        for seed in 1..200u64 {
            let mut scores = [0.0; Behaviour::COUNT];
            scores[Behaviour::Fight as usize] = 0.6 + team.overlap();
            scores[Behaviour::Chase as usize] = 0.6;
            let all = [(0, ally)];
            adjust(&team, seed, |_| 10, &mut scores, &choices, &all, 2.0, &open);
            let mut mind = surprise::Mind::new(seed);
            let gate = surprise::Gate::default();
            let hold = crate::bot_kind::BotHold::default();
            let must = &behaviour::MUST;
            let first = surprise::behaviour(
                &mut mind, &surprise, hold, &scores, false, false, must, gate, 100,
            );
            let again = surprise::behaviour(
                &mut mind, &surprise, hold, &scores, false, false, must, gate, 101,
            );
            assert_eq!(first, again, "a pick is held");
            match first {
                Behaviour::Fight => fights += 1,
                Behaviour::Chase => chases += 1,
                _ => {}
            }
        }
        assert!(fights > 0 && chases > 0, "{fights} fights, {chases} chases");
    }

    #[test]
    fn mood_pull_rises_with_the_share_seen_doing_a_flavour_and_is_capped() {
        let cfg = cfg();
        let crowd = |doing: u64, of: u64| {
            let others = (1..=of).map(move |o| {
                let what = if o <= doing {
                    Doing::Goof(2)
                } else {
                    Doing::Idle
                };
                (o, Vec3::X, what, 1.0, true)
            });
            mood(&cfg, 0, Vec3::ZERO, 20.0, others)
        };
        assert_eq!(crowd(0, 4).0, 0.0);
        assert!(crowd(1, 8).0 < crowd(2, 8).0 && crowd(2, 8).0 < crowd(4, 8).0);
        assert_eq!(crowd(8, 8).0, cfg.mood_cap);
        assert_eq!(crowd(1, 4).1[2], cfg.mood * 0.25);
        assert_eq!(crowd(1, 4).1[3], 0.0);
        let one = |feet: Vec3, seen: bool, weight: f32| {
            let first = [(1, feet, Doing::Goof(2), weight, seen)];
            let rest = (2..=8).map(|o| (o, Vec3::X, Doing::Idle, 1.0, true));
            mood(&cfg, 0, Vec3::ZERO, 20.0, first.into_iter().chain(rest)).0
        };
        assert!(one(Vec3::X, true, 1.0) > 0.0);
        assert_eq!(one(Vec3::X, false, 1.0), 0.0, "out of sight, no pull");
        assert_eq!(one(Vec3::X * 99.0, true, 1.0), 0.0, "out of range, no pull");
        assert!(
            one(Vec3::X, true, cfg.mood_human) > one(Vec3::X, true, 1.0),
            "a person pulls harder than a bot"
        );
        let people = (1..=8).map(|o| (o, Vec3::X, Doing::Goof(2), cfg.mood_human, true));
        assert_eq!(mood(&cfg, 0, Vec3::ZERO, 20.0, people).0, cfg.mood_cap);
        // A person seen playing pulls the other way; an idle one does not.
        let beside = |what| {
            let them = [
                (1, Vec3::X, Doing::Goof(2), 1.0, true),
                (2, Vec3::X, what, cfg.mood_human, true),
            ];
            mood(&cfg, 0, Vec3::ZERO, 20.0, them.into_iter()).0
        };
        assert!(
            beside(Doing::Play) < beside(Doing::Idle),
            "play counts against goofing"
        );
        assert_eq!(
            beside(Doing::Play),
            0.0,
            "one person playing outweighs a goofing bot"
        );
    }

    /// Twelve bots in sight of each other at a long pause, each taking up
    /// flavours by its own chooser with the mood pull from the others: the
    /// share doing one, once a second, over an hour.
    fn waves(team: &BotTeam) -> Vec<f32> {
        let surprise = crate::bot_kind::BotSurprise {
            strength: 1.0,
            ..Default::default()
        };
        let mut minds: Vec<_> = (0..12u64).map(surprise::Mind::new).collect();
        let mut doing: Vec<Option<u8>> = vec![None; minds.len()];
        let mut shares = Vec::new();
        for tick in 0..120 * 3600u64 {
            for (i, mind) in minds.iter_mut().enumerate() {
                let others = doing.iter().enumerate().map(|(o, f)| {
                    (
                        o as u64,
                        Vec3::X,
                        f.map_or(Doing::Idle, Doing::Goof),
                        1.0,
                        true,
                    )
                });
                let (pull, copy) = mood(team, i as u64, Vec3::ZERO, 20.0, others);
                let pause = surprise::Pause {
                    natural: true,
                    play: 0.75,
                    pull,
                    copy,
                    ..Default::default()
                };
                mind.goof(&surprise, crate::bot_kind::BotHold::default(), &pause, tick);
                doing[i] = mind.flavour().map(|f| f as u8);
            }
            if tick % 120 == 0 {
                shares.push(doing.iter().flatten().count() as f32 / doing.len() as f32);
            }
        }
        shares
    }

    fn spread(shares: &[f32]) -> f32 {
        let mean = shares.iter().sum::<f32>() / shares.len() as f32;
        (shares.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / shares.len() as f32).sqrt()
    }

    #[test]
    fn mood_makes_irregular_waves_that_flatten_without_it() {
        let team = cfg();
        let shares = waves(&team);
        let plain = waves(&BotTeam {
            mood: 0.0,
            ..team.clone()
        });
        let busy = |s: &[f32]| s.iter().filter(|v| **v >= 0.5).count();
        // Pinned at neither end, swinging wider with the pull than without.
        assert!(shares.contains(&0.0) && shares.iter().any(|s| *s > 0.0));
        assert!(shares.iter().filter(|s| **s >= 1.0).count() < shares.len() / 10);
        assert!(
            spread(&shares) > spread(&plain) * 1.2,
            "{} vs {}",
            spread(&shares),
            spread(&plain)
        );
        assert!(busy(&shares) > busy(&plain));
        // No fixed period: the gaps between waves vary.
        let rises: Vec<usize> = shares
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.5 && w[1] >= 0.5)
            .map(|(i, _)| i)
            .collect();
        let gaps: Vec<usize> = rises.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.len() >= 3, "waves: {rises:?}");
        assert!(gaps.iter().min() != gaps.iter().max(), "gaps {gaps:?}");
        eprintln!(
            "mood waves: spread {:.3} (plain {:.3}), seconds at half or more {} (plain {}), mean {:.3}, gaps {:?}",
            spread(&shares),
            spread(&plain),
            busy(&shares),
            busy(&plain),
            shares.iter().sum::<f32>() / shares.len() as f32,
            &gaps[..gaps.len().min(12)]
        );
    }

    #[test]
    fn a_seen_teammate_success_lifts_that_option_and_fades_capped() {
        use surprise::Domain;
        let team = BotTeam::default();
        let (fight, chase) = (Behaviour::Fight as u32, Behaviour::Chase as u32);
        let mut mind = surprise::Mind::new(1);
        mind.saw(Domain::Behaviour, fight, copied(&team, true), 0);
        let lift = mind.seen(Domain::Behaviour, fight, 0);
        assert!(lift > 0.0, "a seen success lifts the same option");
        assert_eq!(mind.seen(Domain::Behaviour, chase, 0), 0.0);
        assert_eq!(mind.seen(Domain::Aim, fight, 0), 0.0);
        // Occluded: it never saw it.
        let mut blind = surprise::Mind::new(1);
        blind.saw(Domain::Behaviour, fight, copied(&team, false), 0);
        assert_eq!(blind.seen(Domain::Behaviour, fight, 0), 0.0);
        // It fades to nothing as the enemy adapts.
        let half = (surprise::EFFECTIVENESS_SECONDS * 120.0) as u64;
        let later = mind.seen(Domain::Behaviour, fight, half);
        assert!(
            (later - lift / 2.0).abs() < 1e-4,
            "{later} after a half-life"
        );
        assert_eq!(mind.seen(Domain::Behaviour, fight, half * 20), 0.0);
        // However often it sees it, the lift stays within `copy`.
        for t in 0..50 {
            mind.saw(Domain::Behaviour, fight, team.copy, t);
        }
        assert_eq!(mind.seen(Domain::Behaviour, fight, 49), team.copy);
    }

    /// Coordination reads data, never names: outside comments the module's
    /// only text is its own term names.
    #[test]
    fn the_module_names_no_game_item_or_vehicle() {
        let source = include_str!("team.rs");
        let code = &source[..source.find("#[cfg(test)]").unwrap()];
        for line in code.lines().map(str::trim).filter(|l| !l.starts_with("//")) {
            for text in line.split('"').skip(1).step_by(2) {
                assert!(
                    crate::bot_kind::TERMS.contains(&text),
                    "text `{text}` in: {line}"
                );
            }
        }
    }

    #[test]
    fn a_callout_names_the_term_that_changed_the_choice() {
        let cfg = cfg();
        let mut terms = [Terms::default(); Behaviour::COUNT];
        let (fight, chase, wander) = (
            Behaviour::Fight as usize,
            Behaviour::Chase as usize,
            Behaviour::Wander as usize,
        );
        terms[fight].harm = 0.6;
        assert_eq!(callout(&cfg, &terms, fight, wander), Some("Moving!"));
        assert_eq!(
            callout(&cfg, &terms, wander, fight),
            None,
            "the harm did not move it"
        );
        terms[chase].uses = 0.15;
        assert_eq!(
            callout(&cfg, &terms, wander, chase),
            None,
            "no template for uses"
        );
    }
}
