//! Bot surprise: variation among a bot's choices that changes over time,
//! so bots do not always do the one predictable thing
//! (`docs/architecture/bots.md`, "Surprise"). No personality, mood or
//! script: plain terms on choices the brain already scores, then the one
//! hold rule every choice goes through ([`behaviour::Hold`]).
//!
//! [`Mind::pick`] serves each choice point the brain has (which behaviour,
//! which weapon, which aim point, which way round a chase, and whether to
//! goof). The brain scores its options as before; the mind then scales
//! each score by three terms before the hold rule chooses:
//!
//! - *effectiveness*: an option that is not working (shots dodged, no
//!   damage, stuck) loses score, so another takes over;
//! - a per-bot *drift* that wanders slowly, and *boredom*, which grows
//!   while an option is in use and fades once it is not. Together they
//!   move a score at most [`BAND`] either way, so only near options trade.
//!
//! Nothing varies while it carries an objective or is urgent (low health,
//! hurt at close range), nor for an option that cannot work now (a zero
//! score is no option). Holding a choice (commitment) is the hold rule's,
//! at every strength.
//!
//! Goofing is a choice like any other ([`Domain::Flavour`]): playing, or
//! something idle a player could do (looks at a player, emotes, hops, runs
//! a little circle, walks a detour, looks round, crouches, sprays paint
//! toward a player, takes out another tool or drops its weapon, or flicks
//! its light). Playing grows boring, faster when there is nothing to do,
//! until a goof wins; a goof relieves the boredom. So bots goof now and
//! then even in an objective game, never while they carry an objective.
//!
//! `strength` 0 changes nothing but the plain scores going to the hold
//! rule: every term is 1 and no random number is drawn. The mind still
//! records the decisions for the readout.
use super::behaviour::{Ask, Hold};
use super::*;
use crate::bot_kind::{BotHold, BotSurprise, FLAVOURS};

/// The share either way drift and boredom move a score, at strength 1.
const BAND: f32 = 0.15;
/// How far a drift goes either way (a factor of e^drift), at strength 1,
/// and about how many seconds it takes to cross its range.
const DRIFT: f32 = 0.4;
const DRIFT_SECONDS: f32 = 90.0;
/// Boredom an option gains a second in use, at strength 1, and the
/// seconds it takes to halve.
const BOREDOM: f32 = 0.02;
const BOREDOM_SECONDS: f32 = 20.0;
/// Share of effectiveness one failed outcome takes away (at strength 1),
/// share of the gap to full one success gives back, and the seconds lost
/// effectiveness takes to halve on its own.
const FAILURE: f32 = 0.25;
const SUCCESS: f32 = 0.5;
const EFFECTIVENESS_SECONDS: f32 = 30.0;
/// Urgent, so nothing varies: under this share of health, or hurt by an
/// enemy this close this recently.
const URGENT_HEALTH: f32 = 0.3;
const URGENT_RANGE: f32 = 8.0;
const URGENT_SECONDS: f32 = 1.5;
/// How far to the side a flanking chase aims, and what it scores against
/// straight at them.
const FLANK_DISTANCE: f32 = 5.0;
const FLANK_SCORE: f32 = 0.95;
/// Playing's boredom a second at strength 1 (twice that with nothing to
/// do), what a goof scores against playing's 1, and how long one lasts
/// (up to half again).
const PLAY_BOREDOM: f32 = 0.12;
const IDLE_BOREDOM: f32 = 0.24;
const GOOF_SCORE: f32 = 0.5;
const GOOF_SECONDS: f32 = 2.0;
/// The [`Domain::Flavour`] options.
const PLAY: u32 = 0;
const GOOF: u32 = 1;

/// The choice points the mind serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Domain {
    Behaviour,
    Weapon,
    Aim,
    Route,
    /// Playing, or goofing.
    Flavour,
}
impl Domain {
    const ALL: [Domain; 5] = [
        Self::Behaviour,
        Self::Weapon,
        Self::Aim,
        Self::Route,
        Self::Flavour,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Behaviour => "behaviour",
            Self::Weapon => "weapon",
            Self::Aim => "aim",
            Self::Route => "route",
            Self::Flavour => "flavour",
        }
    }
    fn label(self, option: u32) -> String {
        match self {
            Self::Behaviour => Behaviour::ALL
                .get(option as usize)
                .map_or("?", |b| b.name())
                .into(),
            Self::Weapon => format!("slot {option}"),
            Self::Aim => AIMS.get(option as usize).copied().unwrap_or("?").into(),
            Self::Route => ROUTES.get(option as usize).copied().unwrap_or("?").into(),
            Self::Flavour => ["play", "goof"]
                .get(option as usize)
                .copied()
                .unwrap_or("?")
                .into(),
        }
    }
}
/// Where a splash weapon may aim: the body, its feet, or a surface beside
/// it the blast still reaches.
pub(super) const AIMS: [&str; 3] = ["torso", "feet", "surface"];
pub(super) const AIM_TORSO: u32 = 0;
pub(super) const AIM_FEET: u32 = 1;
pub(super) const AIM_SURFACE: u32 = 2;
/// Which way round a chase goes: straight at them, or wide to a side.
pub(super) const ROUTES: [&str; 3] = ["direct", "left", "right"];
/// Behaviours whose scores vary. The rest (carrying a catch, arming,
/// flying, walking home) keep their plain scores.
const VARIED: [Behaviour; 6] = [
    Behaviour::Interact,
    Behaviour::Fight,
    Behaviour::Chase,
    Behaviour::Search,
    Behaviour::Objective,
    Behaviour::Wander,
];
const TICKS: f32 = 120.0;

/// Why nothing varies now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Gate {
    /// Carrying an objective: a held body, a mount to deliver, a pushed
    /// ball or a picked-up item on its way to a destination.
    pub carrying: bool,
    /// Low on health, or just hurt by an enemy close by.
    pub urgent: bool,
}
impl Gate {
    fn closed(self) -> Option<&'static str> {
        if self.carrying {
            Some("carrying")
        } else if self.urgent {
            Some("urgent")
        } else {
            None
        }
    }
}

/// What the mind keeps about one option.
#[derive(Clone, Copy, Debug)]
struct Drive {
    domain: Domain,
    option: u32,
    /// Its score is multiplied by e^drift (within the band).
    drift: f32,
    boredom: f32,
    /// 1 works; lower when it has not been working.
    effectiveness: f32,
    updated: u64,
}

/// One option's terms in a decision.
#[derive(Clone, Copy, Debug)]
pub(super) struct Term {
    pub option: u32,
    pub score: f32,
    /// The score the hold rule sees, after every term.
    pub adjusted: f32,
    pub drift: f32,
    pub boredom: f32,
    pub effectiveness: f32,
}
#[derive(Clone, Debug)]
pub(super) struct Decision {
    pub domain: Domain,
    pub tick: u64,
    /// The best by the plain scores.
    pub plain: u32,
    pub chosen: u32,
    /// The terms put another option first.
    pub varied: bool,
    /// What the hold rule did (`behaviour::Held::name`).
    pub reason: &'static str,
    pub terms: Vec<Term>,
}

/// One choice to make ([`Mind::pick`]).
pub(super) struct Choice<'a> {
    pub domain: Domain,
    /// Each option and its plain score; 0 is not possible now.
    pub options: &'a [(u32, f32)],
    /// Something happened the choice must answer at once.
    pub interrupt: bool,
    /// The held option scores nothing only because it is between steps.
    pub paused: bool,
    /// Options that take over at once when they win.
    pub must: &'a [u32],
    /// Options whose scores never vary.
    pub fixed: &'a [u32],
}

/// A shot whose outcome is not known yet.
#[derive(Clone, Copy, Debug)]
pub(super) struct Shot {
    pub weapon: Option<u32>,
    pub aim: u32,
    pub behaviour: u32,
    pub target: OwnerId,
    pub spawn: u64,
    pub health: f32,
    pub due: u64,
}

/// Idle things a bot does when it goofs, each an ordinary player action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Flavour {
    Stare,
    Emote,
    Hop,
    Circle,
    Detour,
    Look,
    Crouch,
    Spray,
    Tool,
    Drop,
    Light,
}
impl Flavour {
    const ALL: [Flavour; 11] = [
        Self::Stare,
        Self::Emote,
        Self::Hop,
        Self::Circle,
        Self::Detour,
        Self::Look,
        Self::Crouch,
        Self::Spray,
        Self::Tool,
        Self::Drop,
        Self::Light,
    ];
    pub(super) fn name(self) -> &'static str {
        FLAVOURS[self as usize]
    }
}
/// Emotes a bot strikes when it goofs: those a player strikes that are
/// only a look (and the alarm's harmless flare).
pub(super) const EMOTES: [&str; 4] = ["love", "hate", "confusion", "alarm"];

#[derive(Clone, Copy, Debug)]
pub(super) struct Interrupt {
    pub flavour: Flavour,
    pub since: u64,
    pub until: u64,
    /// A player it looks or sprays toward.
    pub target: Option<OwnerId>,
    /// Random 0..1 fixed at the start (which emote, which way round).
    pub roll: f32,
    /// The slot in hand before it took out another tool.
    pub restore: Option<Option<usize>>,
}
/// What the bot can do idly now.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Pause {
    /// It may goof: playing an objective it does not carry, or with
    /// nothing to do; no enemy in sight, on its feet.
    pub natural: bool,
    /// Nothing to do at all (no objective either): playing bores faster.
    pub idle: bool,
    pub gate: Gate,
    /// A player in sight it might look or spray toward.
    pub player: Option<OwnerId>,
    /// It has another tool to take out, or a second attack so it can drop
    /// the one in hand.
    pub other_tool: bool,
    pub spare_weapon: bool,
}
/// A goof this tick.
#[derive(Clone, Copy, Debug)]
pub(super) enum Moment {
    None,
    Begin(Interrupt),
    Continue(Interrupt),
    End(Interrupt),
}

/// A bot's surprise state: its own random stream, its drives, what each
/// choice point holds, and its last decision at each.
#[derive(Clone, Debug, Default)]
pub(super) struct Mind {
    rng: u64,
    drives: Vec<Drive>,
    holds: [Hold; 5],
    decisions: [Option<Decision>; 5],
    last_pick: [u64; 5],
    /// This tick's guard (the weapon choice, made earlier in the tick,
    /// uses the last one).
    pub gate: Gate,
    interrupt: Option<Interrupt>,
    shots: Vec<Shot>,
}
impl Mind {
    pub(super) fn new(bot: OwnerId) -> Self {
        Self {
            rng: 0x5851_F42D_4C95_7F2D ^ bot.wrapping_mul(0xD1B5_4A32_D192_ED03),
            ..Default::default()
        }
    }
    /// A new life: no choice, goof or shot carries over; drives (how it has
    /// come to like its options) do.
    pub(super) fn new_life(&mut self) {
        self.holds = Default::default();
        self.interrupt = None;
        self.shots.clear();
    }
    fn random(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }
    /// The drive of `option`, brought up to `tick`: boredom and lost
    /// effectiveness fade, and the drift takes a step each second.
    fn drive(&mut self, strength: f32, domain: Domain, option: u32, tick: u64) -> usize {
        let at = match self
            .drives
            .iter()
            .position(|d| d.domain == domain && d.option == option)
        {
            Some(at) => at,
            None => {
                self.drives.push(Drive {
                    domain,
                    option,
                    drift: 0.0,
                    boredom: 0.0,
                    effectiveness: 1.0,
                    updated: tick,
                });
                return self.drives.len() - 1;
            }
        };
        let d = self.drives[at];
        let seconds = tick.saturating_sub(d.updated) as f32 / TICKS;
        let mut drive = d;
        drive.boredom *= 0.5f32.powf(seconds / BOREDOM_SECONDS);
        drive.effectiveness =
            1.0 - (1.0 - drive.effectiveness) * 0.5f32.powf(seconds / EFFECTIVENESS_SECONDS);
        // An Ornstein-Uhlenbeck walk, a step a second: it reverts over
        // `DRIFT_SECONDS` and spreads to about half its limit either way.
        let limit = DRIFT * strength;
        let steps = (tick / 120).saturating_sub(d.updated / 120).min(120);
        let sigma = limit * 0.5 * (2.0 / DRIFT_SECONDS).sqrt() * 3f32.sqrt();
        for _ in 0..steps {
            let noise = self.random() * 2.0 - 1.0;
            drive.drift =
                (drive.drift * (1.0 - 1.0 / DRIFT_SECONDS) + noise * sigma).clamp(-limit, limit);
        }
        drive.updated = tick;
        self.drives[at] = drive;
        at
    }
    /// Choose at one choice point: each option's score scaled by its
    /// terms, then the hold rule. Returns the option chosen.
    pub(super) fn pick(
        &mut self,
        cfg: &BotSurprise,
        rule: BotHold,
        choice: Choice,
        gate: Gate,
        tick: u64,
    ) -> u32 {
        let domain = choice.domain;
        let di = domain as usize;
        let strength = cfg.strength.clamp(0.0, 1.0);
        let on = strength > 0.0 && gate.closed().is_none();
        let since = tick.saturating_sub(self.last_pick[di]).min(120) as f32 / TICKS;
        self.last_pick[di] = tick;
        let clean = |s: f32| if s.is_finite() { s.max(0.0) } else { 0.0 };
        let mut terms = Vec::with_capacity(choice.options.len());
        for (option, score) in choice.options {
            let score = clean(*score);
            let mut term = Term {
                option: *option,
                score,
                adjusted: score,
                drift: 0.0,
                boredom: 0.0,
                effectiveness: 1.0,
            };
            if on && score > 0.0 && !choice.fixed.contains(option) {
                let at = self.drive(strength, domain, *option, tick);
                let d = self.drives[at];
                let band = BAND * strength;
                let mut lean = d.drift.exp() / (1.0 + d.boredom);
                // Goofing is driven by boredom alone, past any band.
                if domain != Domain::Flavour {
                    lean = lean.clamp(1.0 - band, 1.0 + band);
                }
                term.adjusted = score * (1.0 - strength * (1.0 - d.effectiveness)) * lean;
                term.drift = d.drift;
                term.boredom = d.boredom;
                term.effectiveness = d.effectiveness;
            }
            terms.push(term);
        }
        let first = |by: &dyn Fn(&Term) -> f32| {
            let mut best: Option<(u32, f32)> = None;
            for t in &terms {
                if by(t) > 0.0 && best.is_none_or(|(_, b)| by(t) > b) {
                    best = Some((t.option, by(t)));
                }
            }
            best.map(|b| b.0)
        };
        let plain = first(&|t| t.score);
        let varied = plain != first(&|t| t.adjusted);
        let adjusted: Vec<(u32, f32)> = terms.iter().map(|t| (t.option, t.adjusted)).collect();
        let ask = Ask {
            options: &adjusted,
            interrupt: choice.interrupt,
            paused: choice.paused,
            must: choice.must,
        };
        let (chosen, why) = self.holds[di].choose(rule, &ask, tick);
        if on && !choice.fixed.contains(&chosen) {
            self.accrue(strength, domain, chosen, since, tick);
        }
        self.decisions[di] = Some(Decision {
            domain,
            tick,
            plain: plain.unwrap_or(chosen),
            chosen,
            varied,
            reason: why.name(),
            terms,
        });
        chosen
    }
    /// The option in use grows boring.
    fn accrue(&mut self, strength: f32, domain: Domain, option: u32, seconds: f32, tick: u64) {
        let rate = match (domain, option) {
            (Domain::Flavour, GOOF) => 0.0,
            (Domain::Flavour, _) => PLAY_BOREDOM,
            _ => BOREDOM,
        };
        let at = self.drive(strength, domain, option, tick);
        self.drives[at].boredom += rate * strength * seconds;
    }
    /// How an option worked out: a hit, progress; or a dodge, no damage,
    /// stuck. Failures cost effectiveness, so the bot adapts.
    pub(super) fn outcome(
        &mut self,
        cfg: &BotSurprise,
        domain: Domain,
        option: u32,
        success: bool,
        tick: u64,
    ) {
        let strength = cfg.strength.clamp(0.0, 1.0);
        if strength <= 0.0 {
            return;
        }
        let at = self.drive(strength, domain, option, tick);
        let d = &mut self.drives[at];
        d.effectiveness = if success {
            d.effectiveness + (1.0 - d.effectiveness) * SUCCESS
        } else {
            d.effectiveness * (1.0 - FAILURE * strength)
        }
        .clamp(0.05, 1.0);
    }
    /// The option chosen at `domain`.
    pub(super) fn chosen(&self, domain: Domain) -> Option<u32> {
        self.holds[domain as usize].option
    }
    pub(super) fn fired(&mut self, shot: Shot) {
        if self.shots.len() >= 8 {
            self.shots.remove(0);
        }
        self.shots.push(shot);
    }
    /// Shots whose outcome is due by `tick`.
    pub(super) fn due(&mut self, tick: u64) -> Vec<Shot> {
        let (due, later) = self.shots.iter().partition(|s| s.due <= tick);
        self.shots = later;
        due
    }
    /// The goof this tick: one under way goes on until its time is up or
    /// it may no longer goof; a new one starts when goofing wins the
    /// choice against playing.
    pub(super) fn goof(
        &mut self,
        cfg: &BotSurprise,
        rule: BotHold,
        pause: &Pause,
        tick: u64,
    ) -> Moment {
        if let Some(i) = self.interrupt {
            if !pause.natural || pause.gate.closed().is_some() || tick >= i.until {
                self.interrupt = None;
                return Moment::End(i);
            }
            return Moment::Continue(i);
        }
        let strength = cfg.strength.clamp(0.0, 1.0);
        if strength <= 0.0 || !pause.natural || pause.gate.closed().is_some() {
            return Moment::None;
        }
        let possible = |f: Flavour| match f {
            Flavour::Stare | Flavour::Spray => pause.player.is_some(),
            Flavour::Tool => pause.other_tool,
            Flavour::Drop => pause.spare_weapon,
            _ => true,
        };
        let weights: Vec<(Flavour, f32)> = Flavour::ALL
            .into_iter()
            .filter(|f| possible(*f))
            .map(|f| (f, cfg.flavour_weight(f.name())))
            .filter(|(_, w)| *w > 0.0)
            .collect();
        let total: f32 = weights.iter().map(|(_, w)| w).sum();
        let goof = if total > 0.0 { GOOF_SCORE } else { 0.0 };
        // With nothing to do, playing (strolling) bores twice as fast.
        if pause.idle {
            let at = self.drive(strength, Domain::Flavour, PLAY, tick);
            let seconds = tick
                .saturating_sub(self.last_pick[Domain::Flavour as usize])
                .min(120) as f32
                / TICKS;
            self.drives[at].boredom += (IDLE_BOREDOM - PLAY_BOREDOM) * strength * seconds;
        }
        let choice = Choice {
            domain: Domain::Flavour,
            options: &[(PLAY, 1.0), (GOOF, goof)],
            interrupt: false,
            paused: false,
            must: &[],
            fixed: &[],
        };
        if self.pick(cfg, rule, choice, pause.gate, tick) != GOOF {
            return Moment::None;
        }
        let mut roll = self.random() * total;
        let mut flavour = weights[0].0;
        for (f, w) in &weights {
            flavour = *f;
            if roll < *w {
                break;
            }
            roll -= w;
        }
        // A goof relieves playing's boredom.
        let at = self.drive(strength, Domain::Flavour, PLAY, tick);
        self.drives[at].boredom = 0.0;
        let seconds = GOOF_SECONDS * (1.0 + 0.5 * self.random());
        let i = Interrupt {
            flavour,
            since: tick,
            until: tick + (seconds * TICKS).max(1.0) as u64,
            target: pause.player.filter(|_| possible(flavour)),
            roll: self.random(),
            restore: None,
        };
        self.interrupt = Some(i);
        Moment::Begin(i)
    }
    /// Keep what a goof under way remembers (the tool to put back).
    pub(super) fn set_restore(&mut self, restore: Option<usize>) {
        if let Some(i) = self.interrupt.as_mut() {
            i.restore = Some(restore);
        }
    }
    pub(super) fn flavour(&self) -> Option<Flavour> {
        self.interrupt.map(|i| i.flavour)
    }
    pub(super) fn view(&self, cfg: &BotSurprise) -> BotSurpriseView {
        BotSurpriseView {
            strength: cfg.strength,
            gate: self.gate.closed(),
            interrupt: self.flavour().map(Flavour::name),
            drives: self
                .drives
                .iter()
                .map(|d| BotDrive {
                    domain: d.domain.name(),
                    option: d.domain.label(d.option),
                    drift: d.drift,
                    boredom: d.boredom,
                    effectiveness: d.effectiveness,
                })
                .collect(),
            decisions: Domain::ALL
                .iter()
                .filter_map(|d| self.decisions[*d as usize].as_ref())
                .map(|d| BotDecision {
                    domain: d.domain.name(),
                    tick: d.tick,
                    plain: d.domain.label(d.plain),
                    chosen: d.domain.label(d.chosen),
                    varied: d.varied,
                    reason: d.reason,
                    candidates: d
                        .terms
                        .iter()
                        .map(|t| BotCandidate {
                            option: d.domain.label(t.option),
                            score: t.score,
                            adjusted: t.adjusted,
                            drift: t.drift,
                            boredom: t.boredom,
                            effectiveness: t.effectiveness,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// The behaviour to follow, by the hold rule over `scores`
/// (`behaviour::scores`) and their terms. Only behaviours in [`VARIED`]
/// vary; carrying a catch and arming take over at once when they win.
#[allow(clippy::too_many_arguments)]
pub(super) fn behaviour(
    mind: &mut Mind,
    cfg: &BotSurprise,
    rule: BotHold,
    scores: &[f32; 10],
    interrupt: bool,
    paused: bool,
    gate: Gate,
    tick: u64,
) -> Behaviour {
    let options: Vec<(u32, f32)> = Behaviour::ALL
        .into_iter()
        .map(|b| (b as u32, scores[b as usize]))
        .collect();
    let fixed: Vec<u32> = Behaviour::ALL
        .into_iter()
        .filter(|b| !VARIED.contains(b))
        .map(|b| b as u32)
        .collect();
    let choice = Choice {
        domain: Domain::Behaviour,
        options: &options,
        interrupt,
        paused,
        must: &behaviour::MUST,
        fixed: &fixed,
    };
    Behaviour::ALL[mind.pick(cfg, rule, choice, gate, tick) as usize]
}

/// Which way round a chase goes ([`ROUTES`]): straight at the enemy, or
/// wide to one side (`flanks`, the points a side route aims for where
/// there is floor to stand on). The offset from the enemy to aim for.
pub(super) fn route(
    mind: &mut Mind,
    cfg: &BotSurprise,
    rule: BotHold,
    enemy: Vec3,
    flanks: [Option<Vec3>; 2],
    gate: Gate,
    tick: u64,
) -> Vec3 {
    let mut options = vec![(0, 1.0)];
    for (side, flank) in flanks.iter().enumerate() {
        if flank.is_some() {
            options.push((side as u32 + 1, FLANK_SCORE));
        }
    }
    let choice = Choice {
        domain: Domain::Route,
        options: &options,
        interrupt: false,
        paused: false,
        must: &[],
        fixed: &[],
    };
    match mind.pick(cfg, rule, choice, gate, tick) {
        side @ (1 | 2) => flanks[side as usize - 1].map_or(Vec3::ZERO, |at| at - enemy),
        _ => Vec3::ZERO,
    }
}

/// What a flavour interrupt does to this tick's controls.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Act {
    /// Yaw and pitch to look along.
    pub aim: Option<(f32, f32)>,
    /// Where to move (zero stands still); none leaves the walk alone.
    pub direction: Option<Vec3>,
    pub jump: bool,
    pub crouch: bool,
}

impl Session {
    /// Nothing varies while a bot carries an objective, nor while it is
    /// urgent: low on health, or just hurt by an enemy close by.
    pub(super) fn surprise_gate(
        &self,
        bot: OwnerId,
        feet: Vec3,
        carrying: bool,
        threat: Option<Knowledge>,
        tick: u64,
    ) -> Gate {
        if !self.bots.brains.contains_key(&bot) {
            return Gate::default();
        }
        let health = self
            .peers
            .get(&bot)
            .map_or(1.0, |p| p.combat.health / self.max_health(bot).max(1.0));
        let hurt_close = threat.is_some_and(|k| {
            tick.saturating_sub(k.observed) as f32 <= URGENT_SECONDS * TICKS
                && k.at.distance(feet) <= URGENT_RANGE
        });
        Gate {
            carrying,
            urgent: health < URGENT_HEALTH || hurt_close,
        }
    }
    /// Whether a bot carries an objective: a body its tool holds, a mount
    /// it delivers, a loose body it pushes, or a picked-up item on its way
    /// to a destination.
    pub(super) fn surprise_carrying(
        &self,
        bot: OwnerId,
        objective: Option<&objectives::View>,
    ) -> bool {
        self.held_by(bot).is_some()
            || objective.is_some_and(|v| {
                v.held.is_some()
                    || v.drive.is_some()
                    || v.board.is_some()
                    || matches!(v.resource, Some(claims::Resource::Body { .. }))
            })
            || self.bots.brains.get(&bot).is_some_and(|b| {
                b.objective.step.as_ref().is_some_and(|s| {
                    matches!(
                        s.executor,
                        objectives::Executor::Package(package_objectives::Action::Visit { .. })
                    )
                })
            })
    }
    /// A surface beside the body at `centre` (a brick, vehicle or the
    /// ground's edge) that a blast of `radius` there still reaches, on the
    /// side `from` sees.
    pub(super) fn surprise_surface(
        &self,
        enemy: OwnerId,
        centre: Vec3,
        from: Vec3,
        radius: f32,
    ) -> Option<Vec3> {
        use bri_weapons::{Filter, Query, TargetId};
        let shapes = self.tutorial_shape_targets();
        let mut q = crate::weapon_query::WeaponQuery {
            simulation: &self.simulation,
            affect: &|_, _| true,
            affect_radius: &|_, _| true,
            ally: &|_, _| false,
            catch: &|_, _| false,
            responses: &self.events.projectile_responses,
            truncated_targets: 0,
            shapes: &shapes,
        };
        let toward = flat(from - centre).normalize_or_zero();
        let mut best: Option<(f32, Vec3)> = None;
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let out = Vec3::new(a.sin(), 0.0, a.cos());
            let filter = Filter {
                projectile_age_ticks: None,
                source: ActorId(enemy),
                players: false,
                world_only: false,
            };
            let Some(hit) = q.sweep(centre, centre + out * radius * 0.7, filter) else {
                continue;
            };
            if !matches!(
                hit.target,
                TargetId::Brick(_) | TargetId::Vehicle(_) | TargetId::Map(_)
            ) || hit.normal.dot(toward) < 0.2
            {
                continue;
            }
            let d = hit.position.distance(centre);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, hit.position + hit.normal * 0.1));
            }
        }
        best.map(|(_, at)| at)
    }
    /// Points to either side of `enemy` a flanking chase may aim for.
    pub(super) fn surprise_flanks(
        &self,
        bot: OwnerId,
        feet: Vec3,
        enemy: Vec3,
    ) -> [Option<Vec3>; 2] {
        if !self
            .bots
            .brains
            .get(&bot)
            .is_some_and(|b| b.kind.surprise.strength > 0.0)
        {
            return [None, None];
        }
        let across = flat(enemy - feet);
        if across.length() < FLANK_DISTANCE * 2.5 {
            return [None, None];
        }
        let side = Vec3::new(-across.z, 0.0, across.x).normalize_or_zero() * FLANK_DISTANCE;
        [enemy + side, enemy - side].map(|at| {
            self.world_ray(at + Vec3::Y * 3.0, Vec3::NEG_Y, 6.0)
                .is_some()
                .then_some(at)
        })
    }
    /// Outcomes of shots now due: a hit (it lost health or died) works, a
    /// miss does not, for the weapon, aim and behaviour that fired it.
    pub(super) fn surprise_settle(&mut self, bot: OwnerId, tick: u64) {
        let Some(brain) = self.bots.brains.get_mut(&bot) else {
            return;
        };
        if brain.kind.surprise.strength <= 0.0 {
            return;
        }
        for shot in brain.surprise.due(tick) {
            let hit = self.peers.get(&shot.target).is_none_or(|p| {
                !p.combat.alive
                    || p.combat.spawn_tick != shot.spawn
                    || p.combat.health < shot.health - 0.01
            });
            let cfg = &brain.kind.surprise;
            if let Some(slot) = shot.weapon {
                brain.surprise.outcome(cfg, Domain::Weapon, slot, hit, tick);
            }
            brain
                .surprise
                .outcome(cfg, Domain::Aim, shot.aim, hit, tick);
            brain
                .surprise
                .outcome(cfg, Domain::Behaviour, shot.behaviour, hit, tick);
        }
    }
    /// A shot to judge once it has had time to land.
    pub(super) fn surprise_fired(
        &mut self,
        bot: OwnerId,
        choice: Option<hand_combat::Choice>,
        target: Option<OwnerId>,
        behaviour: Behaviour,
        tick: u64,
    ) {
        let Some(peer) = target.and_then(|t| self.peers.get(&t)) else {
            return;
        };
        let (spawn, health) = (peer.combat.spawn_tick, peer.combat.health);
        let Some(brain) = self
            .bots
            .brains
            .get_mut(&bot)
            .filter(|b| b.kind.surprise.strength > 0.0)
        else {
            return;
        };
        let flight = choice
            .and_then(|c| c.aim)
            .map_or(0.0, |a| a.time_seconds as f32);
        let aim = if choice.is_some() {
            brain.surprise.chosen(Domain::Aim).unwrap_or(AIM_TORSO)
        } else {
            AIM_TORSO
        };
        brain.surprise.fired(Shot {
            weapon: choice.map(|c| c.slot as u32),
            aim,
            behaviour: behaviour as u32,
            target: target.unwrap_or_default(),
            spawn,
            health,
            due: tick + ((flight + 0.6) * TICKS) as u64,
        });
    }
    /// What the bot could do idly now.
    pub(super) fn surprise_pause(
        &self,
        bot: OwnerId,
        natural: bool,
        idle: bool,
        gate: Gate,
        eye: Vec3,
    ) -> Pause {
        let on = self
            .bots
            .brains
            .get(&bot)
            .is_some_and(|b| b.kind.surprise.strength > 0.0);
        if !natural || !on {
            return Pause {
                gate,
                ..Default::default()
            };
        }
        let player = self
            .peers
            .iter()
            .filter(|(o, p)| **o != bot && p.combat.alive)
            .map(|(o, p)| (*o, p.player.eye()))
            .filter(|(_, at)| at.distance(eye) < 24.0)
            .filter(|(_, at)| {
                let d = *at - eye;
                self.world_ray(eye, d.normalize_or_zero(), d.length())
                    .is_none()
            })
            .min_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)))
            .map(|(o, _)| o);
        let (other_tool, spare_weapon) =
            self.weapons
                .actor(ActorId(bot))
                .map_or((false, false), |a| {
                    let scale = self.peers.get(&bot).map_or(1.0, |p| p.player.state().scale);
                    let attacks = |item: &Option<String>| {
                        item.as_deref()
                            .is_some_and(|i| hand_combat::item_attacks(self, i, scale))
                    };
                    let other = a
                        .inventory
                        .iter()
                        .enumerate()
                        .any(|(s, i)| i.is_some() && Some(s) != a.selected);
                    let spare = a.selected.is_some_and(|s| attacks(&a.inventory[s]))
                        && a.inventory.iter().filter(|i| attacks(i)).count() >= 2;
                    (other, spare)
                });
        Pause {
            natural,
            idle,
            gate,
            player,
            other_tool,
            spare_weapon,
        }
    }
    /// Carry out a flavour interrupt: the commands a player would give as
    /// it begins and ends, and its controls while it lasts.
    pub(super) fn surprise_act(
        &mut self,
        bot: OwnerId,
        moment: Moment,
        feet: Vec3,
        eye: Vec3,
        tick: u64,
    ) -> Result<Act> {
        let command = |s: &mut Session, c: Command| {
            let sequence = s.peers.get(&bot).map_or(1, |p| p.last_sequence + 1);
            // Refused like a player's would be (rules, spam limits): the
            // interrupt just shows less.
            let _ = s.command(bot, sequence, c);
        };
        let selected = self.weapons.actor(ActorId(bot)).and_then(|a| a.selected);
        let spray_held = |s: &Session| {
            s.weapons
                .image_state(ActorId(bot), 0)
                .is_some_and(|(image, _)| image.id == super::super::tools::SPRAY_CAN_IMAGE)
        };
        let i = match moment {
            Moment::None => return Ok(Act::default()),
            Moment::Begin(i) => {
                // It stops strolling for the moment (a detour sets its own
                // goal below; the light leaves the walk alone).
                if !matches!(i.flavour, Flavour::Detour | Flavour::Light)
                    && let Some(b) = self.bots.brains.get_mut(&bot)
                {
                    b.set_goal(None);
                    b.next_wander = b.next_wander.max(i.until);
                }
                match i.flavour {
                    Flavour::Emote => {
                        let name = EMOTES[(i.roll * EMOTES.len() as f32) as usize % EMOTES.len()];
                        command(self, Command::Emote(name.into()));
                    }
                    Flavour::Spray => {
                        let colours = self.simulation.state().palette.len().max(1);
                        let colour = ((i.roll * colours as f32) as usize % colours) as u8;
                        command(self, Command::UseSprayCan { color: colour });
                        if let Some(b) = self.bots.brains.get_mut(&bot) {
                            b.surprise.set_restore(selected);
                        }
                    }
                    Flavour::Tool => {
                        let others: Vec<usize> = self
                            .weapons
                            .actor(ActorId(bot))
                            .map(|a| {
                                (0..a.inventory.len())
                                    .filter(|s| a.inventory[*s].is_some() && Some(*s) != selected)
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !others.is_empty() {
                            let slot =
                                others[(i.roll * others.len() as f32) as usize % others.len()];
                            self.abort_bot_hand_charge(bot)?;
                            let _ = self.equip_tool(bot, Some(slot));
                            if let Some(b) = self.bots.brains.get_mut(&bot) {
                                b.surprise.set_restore(selected);
                            }
                        }
                    }
                    Flavour::Drop => {
                        if let Some(slot) = selected {
                            self.abort_bot_hand_charge(bot)?;
                            command(self, Command::DropTool { slot });
                        }
                    }
                    Flavour::Light => command(self, Command::ToggleLight),
                    Flavour::Detour => {
                        if let Some(b) = self.bots.brains.get_mut(&bot) {
                            let a = i.roll * std::f32::consts::TAU;
                            let far = 4.0 + 4.0 * i.roll;
                            b.set_goal(Some(Goal::Wander(
                                feet + Vec3::new(a.sin(), 0.0, a.cos()) * far,
                            )));
                        }
                    }
                    _ => {}
                }
                i
            }
            Moment::Continue(i) => i,
            Moment::End(i) => {
                match i.flavour {
                    Flavour::Spray | Flavour::Tool => {
                        if spray_held(self) {
                            let (y, p) = self
                                .bots
                                .brains
                                .get(&bot)
                                .map_or((0.0, 0.0), |b| (b.yaw, b.pitch));
                            let look = Vec3::new(y.sin() * p.cos(), p.sin(), -y.cos() * p.cos());
                            let _ = self.weapon_trigger(bot, false, look, false);
                        }
                        if let Some(restore) = i.restore {
                            let _ = self.equip_tool(bot, restore);
                        }
                    }
                    Flavour::Light => command(self, Command::ToggleLight),
                    _ => {}
                }
                return Ok(Act::default());
            }
        };
        let t = tick.saturating_sub(i.since) as f32 / TICKS;
        let yaw = self.bots.brains.get(&bot).map_or(0.0, |b| b.yaw);
        let look = |at: Vec3| {
            let d = at - eye;
            (yaw_to(d), d.y.atan2(flat(d).length()).clamp(-1.5, 1.5))
        };
        let target = i
            .target
            .and_then(|t| self.peers.get(&t))
            .filter(|p| p.combat.alive)
            .map(|p| (p.player.eye(), Vec3::from(p.player.state().feet)));
        let still = Some(Vec3::ZERO);
        Ok(match i.flavour {
            Flavour::Stare => Act {
                aim: target.map(|(eye, _)| look(eye)),
                direction: still,
                ..Default::default()
            },
            Flavour::Spray => {
                // Paint the floor between them, toward the player: only with
                // room between, so the stream lands well short of them.
                let aim = target.map(|(_, at)| look(feet.lerp(at, 0.4)));
                let room = target.is_some_and(|(_, at)| flat(at - feet).length() > 6.0);
                // Once the can is in hand, paint toward them.
                if tick == i.since + 2
                    && room
                    && spray_held(self)
                    && let Some((y, p)) = aim
                {
                    let direction = Vec3::new(y.sin() * p.cos(), p.sin(), -y.cos() * p.cos());
                    let _ = self.weapon_trigger(bot, true, direction, false);
                }
                Act {
                    aim,
                    direction: still,
                    ..Default::default()
                }
            }
            Flavour::Hop => Act {
                direction: still,
                jump: (tick - i.since) % 60 < 6,
                ..Default::default()
            },
            Flavour::Circle => {
                let turn = if i.roll < 0.5 { 1.0 } else { -1.0 };
                let a = i.roll * std::f32::consts::TAU + turn * t * std::f32::consts::PI;
                Act {
                    direction: Some(Vec3::new(a.sin(), 0.0, -a.cos())),
                    ..Default::default()
                }
            }
            Flavour::Look => Act {
                aim: Some((wrap(i.roll * std::f32::consts::TAU + t * 1.2), 0.1)),
                direction: still,
                ..Default::default()
            },
            Flavour::Crouch => Act {
                direction: still,
                crouch: true,
                ..Default::default()
            },
            Flavour::Emote | Flavour::Tool | Flavour::Drop => Act {
                aim: Some((yaw, -0.3)),
                direction: still,
                ..Default::default()
            },
            Flavour::Detour | Flavour::Light => Act::default(),
        })
    }
}

/// Readout of a bot's surprise state for diagnostics (`BotThought`).
#[derive(Clone, Debug)]
pub struct BotSurpriseView {
    /// The kind's strength; 0 is the plain brain.
    pub strength: f32,
    /// Why nothing varies now (carrying, urgent), if so.
    pub gate: Option<&'static str>,
    /// A goof under way.
    pub interrupt: Option<&'static str>,
    pub drives: Vec<BotDrive>,
    /// The last decision at each choice point.
    pub decisions: Vec<BotDecision>,
}
#[derive(Clone, Debug)]
pub struct BotDrive {
    pub domain: &'static str,
    pub option: String,
    pub drift: f32,
    pub boredom: f32,
    pub effectiveness: f32,
}
#[derive(Clone, Debug)]
pub struct BotDecision {
    pub domain: &'static str,
    pub tick: u64,
    /// The best by the plain scores.
    pub plain: String,
    pub chosen: String,
    /// The surprise terms put another option first.
    pub varied: bool,
    /// What the hold rule did: first, best, committed, margin, paused,
    /// beaten, interrupt or impossible.
    pub reason: &'static str,
    pub candidates: Vec<BotCandidate>,
}
/// One option of a decision and each term that weighed on it.
#[derive(Clone, Debug)]
pub struct BotCandidate {
    pub option: String,
    pub score: f32,
    /// The score the hold rule saw, after every term.
    pub adjusted: f32,
    pub drift: f32,
    pub boredom: f32,
    pub effectiveness: f32,
}
