//! Bot surprise: variation among a bot's choices that changes over time,
//! so bots do not always do the one predictable thing
//! (`docs/architecture/bots.md`, "Surprise"). No personality, mood or
//! script: plain mechanisms over choices the brain already scores.
//!
//! One chooser ([`Mind::pick`]) sits at each choice point the brain has
//! (which behaviour, which weapon, which aim point, which way round a
//! chase). The brain still scores its options as before and names its
//! plain pick; the chooser then picks at random, by weight, among the
//! options scoring near the best:
//!
//! - an option's score counts with its *effectiveness*: one that is not
//!   working (shots dodged, no damage, stuck) loses it, and an option that
//!   was best may fall out of the band so another takes over;
//! - each eligible option weighs by how near the best it scores, by a
//!   per-bot *drift* that wanders slowly, and by *boredom*, which grows
//!   while an option is in use and fades once it is not.
//!
//! Guards keep it sane: a pick is held for a while (commitment); nothing
//! varies while it carries an objective or is urgent (low health, hurt at
//! close range); only options that work now are offered (a zero score is
//! no option); and a switch the variation causes is preceded by a short
//! pause (the tell).
//!
//! At natural pauses a bot sometimes does something idle a player could
//! do (a flavour [`Interrupt`]): looks at a player, emotes, hops, runs a
//! little circle, walks a detour, looks round, crouches, sprays paint
//! toward a player, takes out another tool or drops its weapon, or
//! flicks its light. Never while it carries an objective.
//!
//! `strength` 0 (the default) changes nothing: every pick is the plain
//! one and no random number is drawn. The mind still records the plain
//! decisions for the readout.
use super::*;
use crate::bot_kind::{BotSurprise, INTERRUPTS};

/// The choice points the chooser covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Domain {
    Behaviour,
    Weapon,
    Aim,
    Route,
}
impl Domain {
    const ALL: [Domain; 4] = [Self::Behaviour, Self::Weapon, Self::Aim, Self::Route];
    fn name(self) -> &'static str {
        match self {
            Self::Behaviour => "behaviour",
            Self::Weapon => "weapon",
            Self::Aim => "aim",
            Self::Route => "route",
        }
    }
    /// A switch here shows (another weapon in hand, another activity):
    /// the variation pauses a moment first.
    fn tells(self) -> bool {
        matches!(self, Self::Behaviour | Self::Weapon)
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
/// Behaviours the chooser may trade for each other. The rest (carrying a
/// catch, arming, walking home) are done when they apply.
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
    /// Its weight is multiplied by e^drift.
    drift: f32,
    boredom: f32,
    /// 1 works; lower when it has not been working.
    effectiveness: f32,
    updated: u64,
    /// Seen it work for a teammate (`team` copy): a bonus on its score that
    /// fades over `effectiveness_seconds`, as of `seen_at`.
    seen: f32,
    seen_at: u64,
}

/// One option's terms in a decision.
#[derive(Clone, Copy, Debug)]
pub(super) struct Term {
    pub option: u32,
    pub score: f32,
    /// The score as the band sees it, after effectiveness.
    pub adjusted: f32,
    pub eligible: bool,
    pub drift: f32,
    pub boredom: f32,
    pub effectiveness: f32,
    /// Its share in the random pick (0 if not eligible).
    pub weight: f32,
}
#[derive(Clone, Debug)]
pub(super) struct Decision {
    pub domain: Domain,
    pub tick: u64,
    pub plain: u32,
    pub chosen: u32,
    /// off, gated (carrying/urgent), plain, committed, picked, telling,
    /// switched.
    pub reason: &'static str,
    pub terms: Vec<Term>,
}
#[derive(Clone, Copy, Debug)]
struct Commit {
    option: u32,
    until: u64,
}
#[derive(Clone, Copy, Debug)]
struct Tell {
    domain: Domain,
    from: u32,
    to: u32,
    until: u64,
}
/// What the chooser says to do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pick {
    pub option: u32,
    /// Holding the old option a moment before switching: pause.
    pub telling: bool,
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

/// Idle things a bot does at a pause, each an ordinary player action.
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
        INTERRUPTS[self as usize]
    }
}
/// Emotes a bot strikes at a pause: those a player strikes that are only
/// a look (and the alarm's harmless flare).
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
    /// Nothing to do but stroll: no enemy, no objective, on its feet.
    pub natural: bool,
    pub gate: Gate,
    /// A player in sight it might look or spray toward.
    pub player: Option<OwnerId>,
    /// It has another tool to take out, or a second attack so it can drop
    /// the one in hand.
    pub other_tool: bool,
    pub spare_weapon: bool,
    /// Mood (`team::mood`): how much likelier any flavour is, and each one,
    /// from bots nearby doing one.
    pub pull: f32,
    pub copy: [f32; 11],
}
/// An interrupt this tick.
#[derive(Clone, Copy, Debug)]
pub(super) enum Moment {
    None,
    Begin(Interrupt),
    Continue(Interrupt),
    End(Interrupt),
}

/// A bot's surprise state: its own random stream, its drives, what it
/// holds, and its last decision at each choice point.
#[derive(Clone, Debug, Default)]
pub(super) struct Mind {
    rng: u64,
    drives: Vec<Drive>,
    commits: [Option<Commit>; 4],
    tell: Option<Tell>,
    decisions: [Option<Decision>; 4],
    last_pick: [u64; 4],
    /// This tick's guard (the weapon choice, made earlier in the tick,
    /// uses the last one).
    pub gate: Gate,
    interrupt: Option<Interrupt>,
    next_interrupt: u64,
    /// Seconds spent in flavours, halving every `boredom_seconds` (as of
    /// the tick beside it): the whole kind grows stale, not one flavour.
    stale: (f32, u64),
    shots: Vec<Shot>,
}
impl Mind {
    pub(super) fn new(bot: OwnerId) -> Self {
        Self {
            rng: 0x5851_F42D_4C95_7F2D ^ bot.wrapping_mul(0xD1B5_4A32_D192_ED03),
            ..Default::default()
        }
    }
    /// A new life: no pick, tell, interrupt or shot carries over; drives
    /// (how it has come to like its options) do.
    pub(super) fn new_life(&mut self) {
        self.commits = Default::default();
        self.tell = None;
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
    fn drive(&mut self, cfg: &BotSurprise, domain: Domain, option: u32, tick: u64) -> usize {
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
                    seen: 0.0,
                    seen_at: tick,
                });
                return self.drives.len() - 1;
            }
        };
        let d = self.drives[at];
        let seconds = tick.saturating_sub(d.updated) as f32 / TICKS;
        let mut drive = d;
        drive.boredom *= 0.5f32.powf(seconds / cfg.boredom_seconds);
        drive.effectiveness =
            1.0 - (1.0 - drive.effectiveness) * 0.5f32.powf(seconds / cfg.effectiveness_seconds);
        // An Ornstein-Uhlenbeck walk, a step a second: it reverts over
        // `drift_seconds` and spreads to about half `drift` either way.
        let limit = cfg.drift * cfg.strength;
        let steps = (tick / 120).saturating_sub(d.updated / 120).min(120);
        let sigma = limit * 0.5 * (2.0 / cfg.drift_seconds).sqrt() * 3f32.sqrt();
        for _ in 0..steps {
            let noise = self.random() * 2.0 - 1.0;
            drive.drift = (drive.drift * (1.0 - 1.0 / cfg.drift_seconds) + noise * sigma)
                .clamp(-limit, limit);
        }
        drive.updated = tick;
        self.drives[at] = drive;
        at
    }
    /// Choose at one choice point. `options` are those the brain scored
    /// (a zero or non-finite score is not an option), `plain` the brain's
    /// own pick and `current` what is in effect now.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pick(
        &mut self,
        cfg: &BotSurprise,
        domain: Domain,
        options: &[(u32, f32)],
        plain: u32,
        current: Option<u32>,
        gate: Gate,
        tick: u64,
    ) -> Pick {
        let di = domain as usize;
        let viable: Vec<(u32, f32)> = options
            .iter()
            .copied()
            .filter(|(_, s)| s.is_finite() && *s > 0.0)
            .collect();
        let plain_terms = |viable: &[(u32, f32)]| {
            viable
                .iter()
                .map(|(option, score)| Term {
                    option: *option,
                    score: *score,
                    adjusted: *score,
                    eligible: *option == plain,
                    drift: 0.0,
                    boredom: 0.0,
                    effectiveness: 1.0,
                    weight: if *option == plain { 1.0 } else { 0.0 },
                })
                .collect::<Vec<_>>()
        };
        let reason = if cfg.strength <= 0.0 {
            Some("off")
        } else if let Some(gated) = gate.closed() {
            Some(gated)
        } else if !viable.iter().any(|(o, _)| *o == plain) {
            Some("plain")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.commits[di] = None;
            if self.tell.is_some_and(|t| t.domain == domain) {
                self.tell = None;
            }
            self.record(domain, tick, plain, plain, reason, plain_terms(&viable));
            return Pick {
                option: plain,
                telling: false,
            };
        }
        // Terms: effectiveness counts in the score, drift and boredom in
        // the weight.
        let since = tick.saturating_sub(self.last_pick[di]).min(120) as f32 / TICKS;
        self.last_pick[di] = tick;
        let mut terms = Vec::with_capacity(viable.len());
        for (option, score) in &viable {
            let at = self.drive(cfg, domain, *option, tick);
            let d = self.drives[at];
            terms.push(Term {
                option: *option,
                score: *score,
                adjusted: score
                    * (1.0 - cfg.strength * (1.0 - d.effectiveness))
                    * (1.0 + Self::faded(cfg, &d, tick)),
                eligible: false,
                drift: d.drift,
                boredom: d.boredom,
                effectiveness: d.effectiveness,
                weight: 0.0,
            });
        }
        let best = terms.iter().map(|t| t.adjusted).fold(0.0, f32::max);
        let floor = best * (1.0 - cfg.band * cfg.strength);
        for t in &mut terms {
            t.eligible = t.adjusted >= floor && t.adjusted > 0.0;
            if t.eligible {
                t.weight = (t.adjusted / best).powi(4) * t.drift.exp() / (1.0 + t.boredom);
            }
        }
        let eligible = |terms: &[Term], o: u32| terms.iter().any(|t| t.option == o && t.eligible);
        let viable_now = |o: u32| viable.iter().any(|(v, _)| *v == o);
        let mut reason = "picked";
        let mut chosen = None;
        // A tell under way: hold what it had until the switch.
        if let Some(t) = self.tell.filter(|t| t.domain == domain) {
            if tick < t.until && viable_now(t.from) && eligible(&terms, t.to) {
                self.accrue(cfg, domain, t.from, since, tick);
                self.record(domain, tick, plain, t.from, "telling", terms);
                return Pick {
                    option: t.from,
                    telling: true,
                };
            }
            self.tell = None;
            if eligible(&terms, t.to) {
                chosen = Some(t.to);
                reason = "switched";
                self.commit(cfg, domain, t.to, tick);
            }
        }
        if chosen.is_none()
            && let Some(c) = self.commits[di]
            && tick < c.until
            && eligible(&terms, c.option)
        {
            chosen = Some(c.option);
            reason = "committed";
        }
        let chosen = match chosen {
            Some(c) => c,
            None => {
                let total: f32 = terms.iter().map(|t| t.weight).sum();
                let c = if total > 0.0 {
                    let mut roll = self.random() * total;
                    let mut pick = plain;
                    for t in terms.iter().filter(|t| t.weight > 0.0) {
                        pick = t.option;
                        if roll < t.weight {
                            break;
                        }
                        roll -= t.weight;
                    }
                    pick
                } else {
                    plain
                };
                self.commit(cfg, domain, c, tick);
                // A switch the variation causes shows first.
                if domain.tells()
                    && cfg.tell_seconds > 0.0
                    && c != plain
                    && let Some(from) = current.filter(|f| *f != c && viable_now(*f))
                {
                    self.tell = Some(Tell {
                        domain,
                        from,
                        to: c,
                        until: tick + (cfg.tell_seconds * TICKS).round().max(1.0) as u64,
                    });
                    self.accrue(cfg, domain, from, since, tick);
                    self.record(domain, tick, plain, from, "telling", terms);
                    return Pick {
                        option: from,
                        telling: true,
                    };
                }
                c
            }
        };
        self.accrue(cfg, domain, chosen, since, tick);
        self.record(domain, tick, plain, chosen, reason, terms);
        Pick {
            option: chosen,
            telling: false,
        }
    }
    fn commit(&mut self, cfg: &BotSurprise, domain: Domain, option: u32, tick: u64) {
        let hold = cfg.commit_seconds * (1.0 + 0.5 * self.random());
        self.commits[domain as usize] = Some(Commit {
            option,
            until: tick + (hold * TICKS) as u64,
        });
    }
    /// The option in use grows boring.
    fn accrue(&mut self, cfg: &BotSurprise, domain: Domain, option: u32, seconds: f32, tick: u64) {
        let at = self.drive(cfg, domain, option, tick);
        self.drives[at].boredom += cfg.boredom * cfg.strength * seconds;
    }
    fn record(
        &mut self,
        domain: Domain,
        tick: u64,
        plain: u32,
        chosen: u32,
        reason: &'static str,
        terms: Vec<Term>,
    ) {
        self.decisions[domain as usize] = Some(Decision {
            domain,
            tick,
            plain,
            chosen,
            reason,
            terms,
        });
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
        if cfg.strength <= 0.0 {
            return;
        }
        let at = self.drive(cfg, domain, option, tick);
        let d = &mut self.drives[at];
        d.effectiveness = if success {
            d.effectiveness + (1.0 - d.effectiveness) * cfg.success
        } else {
            d.effectiveness * (1.0 - cfg.failure * cfg.strength)
        }
        .clamp(0.05, 1.0);
    }
    /// What seeing `option` work for a teammate is still worth, at `tick`.
    fn faded(cfg: &BotSurprise, d: &Drive, tick: u64) -> f32 {
        let seconds = tick.saturating_sub(d.seen_at) as f32 / TICKS;
        let seen = d.seen * 0.5f32.powf(seconds / cfg.effectiveness_seconds);
        if seen < 1e-3 { 0.0 } else { seen }
    }
    /// The bonus on `option` from having seen it work for a teammate.
    /// Draws nothing, so the plain brain stays as it was.
    pub(super) fn seen(&self, cfg: &BotSurprise, domain: Domain, option: u32, tick: u64) -> f32 {
        self.drives
            .iter()
            .find(|d| d.domain == domain && d.option == option)
            .map_or(0.0, |d| Self::faded(cfg, d, tick))
    }
    /// A teammate's `option` worked where this bot saw it: each sighting
    /// adds half of `copy`, and the bonus never passes `copy`.
    pub(super) fn saw(
        &mut self,
        cfg: &BotSurprise,
        domain: Domain,
        option: u32,
        copy: f32,
        tick: u64,
    ) {
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
                    seen: 0.0,
                    seen_at: tick,
                });
                self.drives.len() - 1
            }
        };
        let now = Self::faded(cfg, &self.drives[at], tick);
        let d = &mut self.drives[at];
        d.seen = (now + copy * 0.5).min(copy);
        d.seen_at = tick;
    }
    /// In a tell: pausing before a switch.
    pub(super) fn telling(&self, tick: u64) -> bool {
        self.tell.is_some_and(|t| tick < t.until)
    }
    /// The option last chosen at `domain`.
    pub(super) fn chosen(&self, domain: Domain) -> Option<u32> {
        self.decisions[domain as usize].as_ref().map(|d| d.chosen)
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
    /// The flavour interrupt this tick: one under way goes on until its
    /// time is up or the pause ends; a new one starts now and then at a
    /// natural pause once the last has cooled down.
    pub(super) fn interrupt(&mut self, cfg: &BotSurprise, pause: &Pause, tick: u64) -> Moment {
        if let Some(i) = self.interrupt {
            if !pause.natural || pause.gate.closed().is_some() || tick >= i.until {
                self.interrupt = None;
                // Others about still at it cut the rest short (`team::mood`).
                self.next_interrupt = tick
                    + (cfg.interrupt_cooldown_seconds * TICKS / (1.0 + pause.pull)).round() as u64;
                self.stale = (
                    self.stale(cfg, tick) + tick.saturating_sub(i.since) as f32 / TICKS,
                    tick,
                );
                return Moment::End(i);
            }
            return Moment::Continue(i);
        }
        if cfg.strength <= 0.0
            || cfg.interrupts_per_minute <= 0.0
            || !pause.natural
            || pause.gate.closed().is_some()
            || tick < self.next_interrupt
        {
            return Moment::None;
        }
        let chance = cfg.interrupts_per_minute * cfg.strength * (1.0 + pause.pull)
            / (1.0 + cfg.boredom * self.stale(cfg, tick))
            / (60.0 * TICKS);
        if self.random() >= chance {
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
            .map(|f| {
                (
                    f,
                    cfg.interrupt_weight(f.name()) * (1.0 + pause.copy[f as usize]),
                )
            })
            .filter(|(_, w)| *w > 0.0)
            .collect();
        let total: f32 = weights.iter().map(|(_, w)| w).sum();
        if total <= 0.0 {
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
        let seconds = cfg.interrupt_seconds * (1.0 + 0.5 * self.random());
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
    fn stale(&self, cfg: &BotSurprise, tick: u64) -> f32 {
        let (seconds, at) = self.stale;
        let half_lives = tick.saturating_sub(at) as f32 / (cfg.boredom_seconds * TICKS);
        seconds * 0.5f32.powf(half_lives)
    }
    /// Keep what an interrupt under way remembers (the tool to put back).
    pub(super) fn set_restore(&mut self, restore: Option<usize>) {
        if let Some(i) = self.interrupt.as_mut() {
            i.restore = Some(restore);
        }
    }
    pub(super) fn flavour(&self) -> Option<Flavour> {
        self.interrupt.map(|i| i.flavour)
    }
    pub(super) fn view(&self, cfg: &BotSurprise, tick: u64) -> BotSurpriseView {
        BotSurpriseView {
            strength: cfg.strength,
            gate: self.gate.closed(),
            telling: self.tell.filter(|t| tick < t.until).map(|t| {
                format!(
                    "{}: {} -> {}",
                    t.domain.name(),
                    t.domain.label(t.from),
                    t.domain.label(t.to)
                )
            }),
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
                    reason: d.reason,
                    candidates: d
                        .terms
                        .iter()
                        .map(|t| BotCandidate {
                            option: d.domain.label(t.option),
                            score: t.score,
                            adjusted: t.adjusted,
                            eligible: t.eligible,
                            drift: t.drift,
                            boredom: t.boredom,
                            effectiveness: t.effectiveness,
                            weight: t.weight,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// The behaviour to follow: the plain pick (`behaviour::choose` over
/// `scores`), or a near one the chooser takes instead. Only behaviours in
/// [`VARIED`] are traded, and only when the plain pick is one.
pub(super) fn behaviour(
    mind: &mut Mind,
    cfg: &BotSurprise,
    scores: &[f32; Behaviour::COUNT],
    plain: Behaviour,
    current: Behaviour,
    gate: Gate,
    tick: u64,
) -> (Behaviour, bool) {
    let varied = VARIED.contains(&plain);
    let options: Vec<(u32, f32)> = Behaviour::ALL
        .into_iter()
        .filter(|b| VARIED.contains(b) && (varied || *b == plain))
        .map(|b| (b as u32, scores[b as usize]))
        .collect();
    let options = if varied {
        options
    } else {
        vec![(plain as u32, scores[plain as usize].max(f32::MIN_POSITIVE))]
    };
    let pick = mind.pick(
        cfg,
        Domain::Behaviour,
        &options,
        plain as u32,
        Some(current as u32),
        gate,
        tick,
    );
    (Behaviour::ALL[pick.option as usize], pick.telling)
}

/// Which way round a chase goes ([`ROUTES`]): straight at the enemy, or
/// wide to one side (`flanks`, the points a side route aims for where
/// there is floor to stand on). The offset from the enemy to aim for.
pub(super) fn route(
    mind: &mut Mind,
    cfg: &BotSurprise,
    enemy: Vec3,
    flanks: [Option<Vec3>; 2],
    gate: Gate,
    tick: u64,
) -> Vec3 {
    let mut options = vec![(0, 1.0)];
    for (side, flank) in flanks.iter().enumerate() {
        if flank.is_some() {
            options.push((side as u32 + 1, 0.9));
        }
    }
    let current = mind.chosen(Domain::Route);
    let pick = mind.pick(cfg, Domain::Route, &options, 0, current, gate, tick);
    match pick.option {
        1 | 2 => flanks[pick.option as usize - 1].map_or(Vec3::ZERO, |at| at - enemy),
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
        let Some(brain) = self.bots.brains.get(&bot) else {
            return Gate::default();
        };
        let cfg = &brain.kind.surprise;
        let health = self
            .peers
            .get(&bot)
            .map_or(1.0, |p| p.combat.health / self.max_health(bot).max(1.0));
        let hurt_close = threat.is_some_and(|k| {
            tick.saturating_sub(k.observed) as f32 <= cfg.urgent_seconds * TICKS
                && k.at.distance(feet) <= cfg.urgent_range
        });
        Gate {
            carrying,
            urgent: health < cfg.urgent_health || hurt_close,
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
        let Some(cfg) = self
            .bots
            .brains
            .get(&bot)
            .map(|b| &b.kind.surprise)
            .filter(|s| s.strength > 0.0 && s.flank_distance > 0.0)
        else {
            return [None, None];
        };
        let across = flat(enemy - feet);
        if across.length() < cfg.flank_distance * 2.5 {
            return [None, None];
        }
        let side = Vec3::new(-across.z, 0.0, across.x).normalize_or_zero() * cfg.flank_distance;
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
        if brain.kind.surprise.strength <= 0.0 && brain.kind.team.copy <= 0.0 {
            return;
        }
        let mut worked = Vec::new();
        for shot in brain.surprise.due(tick) {
            let hit = self.peers.get(&shot.target).is_none_or(|p| {
                !p.combat.alive
                    || p.combat.spawn_tick != shot.spawn
                    || p.combat.health < shot.health - 0.01
            });
            let cfg = &brain.kind.surprise;
            if let Some(slot) = shot.weapon {
                brain.surprise.outcome(cfg, Domain::Weapon, slot, hit, tick);
                if hit {
                    worked.push((Domain::Weapon, slot));
                }
            }
            brain
                .surprise
                .outcome(cfg, Domain::Aim, shot.aim, hit, tick);
            brain
                .surprise
                .outcome(cfg, Domain::Behaviour, shot.behaviour, hit, tick);
            if hit {
                worked.extend([(Domain::Aim, shot.aim), (Domain::Behaviour, shot.behaviour)]);
            }
        }
        self.team_copy(bot, &worked, tick);
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
            .filter(|b| b.kind.surprise.strength > 0.0 || b.kind.team.copy > 0.0)
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
            gate,
            player,
            other_tool,
            spare_weapon,
            ..Default::default()
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
    /// A switch it is pausing before.
    pub telling: Option<String>,
    /// A flavour interrupt under way.
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
    pub plain: String,
    pub chosen: String,
    pub reason: &'static str,
    pub candidates: Vec<BotCandidate>,
}
/// One option of a decision and each term that weighed on it.
#[derive(Clone, Debug)]
pub struct BotCandidate {
    pub option: String,
    pub score: f32,
    pub adjusted: f32,
    pub eligible: bool,
    pub drift: f32,
    pub boredom: f32,
    pub effectiveness: f32,
    pub weight: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(strength: f32) -> BotSurprise {
        BotSurprise {
            strength,
            ..Default::default()
        }
    }
    const OPEN: Gate = Gate {
        carrying: false,
        urgent: false,
    };
    /// Picks over `ticks`, re-asking every tick.
    fn run(mind: &mut Mind, cfg: &BotSurprise, options: &[(u32, f32)], ticks: u64) -> Vec<u32> {
        let mut out = Vec::new();
        let mut current = None;
        for tick in 0..ticks {
            let p = mind.pick(cfg, Domain::Aim, options, 0, current, OPEN, tick);
            current = Some(p.option);
            out.push(p.option);
        }
        out
    }

    #[test]
    fn the_chooser_is_deterministic_for_a_seed() {
        let cfg = BotSurprise {
            commit_seconds: 0.0,
            ..on(1.0)
        };
        let options = [(0, 1.0), (1, 0.95), (2, 0.9)];
        let a = run(&mut Mind::new(7), &cfg, &options, 2000);
        let b = run(&mut Mind::new(7), &cfg, &options, 2000);
        assert_eq!(a, b);
        assert_ne!(a, run(&mut Mind::new(8), &cfg, &options, 2000));
        // And it does vary.
        assert!(a.contains(&1) && a.contains(&2), "{:?}", &a[..40]);
    }

    /// Strength 0 is the plain brain: the plain pick every time, with no
    /// random number drawn.
    #[test]
    fn strength_zero_reproduces_the_plain_pick() {
        let cfg = on(0.0);
        let mut mind = Mind::new(3);
        let rng = mind.rng;
        for tick in 0..1000 {
            let plain = (tick / 100 % 3) as u32;
            let p = mind.pick(
                &cfg,
                Domain::Weapon,
                &[(0, 1.0), (1, 1.0), (2, 1.0)],
                plain,
                Some((plain + 1) % 3),
                OPEN,
                tick,
            );
            assert_eq!(
                p,
                Pick {
                    option: plain,
                    telling: false
                }
            );
        }
        assert_eq!(mind.rng, rng, "no random number drawn");
        mind.outcome(&cfg, Domain::Weapon, 0, false, 5);
        assert!(mind.drives.is_empty());
        let pause = Pause {
            natural: true,
            ..Default::default()
        };
        for tick in 0..100_000 {
            assert!(matches!(mind.interrupt(&cfg, &pause, tick), Moment::None));
        }
        assert_eq!(mind.rng, rng);
        // The behaviour hook likewise.
        let mut scores = [0.0; Behaviour::COUNT];
        scores[Behaviour::Objective as usize] = 0.65;
        scores[Behaviour::Chase as usize] = 0.6;
        scores[Behaviour::Wander as usize] = 0.1;
        for tick in 0..500 {
            assert_eq!(
                behaviour(
                    &mut mind,
                    &cfg,
                    &scores,
                    Behaviour::Objective,
                    Behaviour::Objective,
                    OPEN,
                    tick
                ),
                (Behaviour::Objective, false)
            );
        }
    }

    /// A pick is held for its commitment: no re-roll each tick.
    #[test]
    fn commitment_holds_a_pick() {
        let cfg = BotSurprise {
            commit_seconds: 2.0,
            tell_seconds: 0.0,
            ..on(1.0)
        };
        let options = [(0, 1.0), (1, 1.0), (2, 1.0)];
        let picks = run(&mut Mind::new(11), &cfg, &options, 6000);
        let mut runs = Vec::new();
        let mut length = 1;
        for w in picks.windows(2) {
            if w[0] == w[1] {
                length += 1;
            } else {
                runs.push(length);
                length = 1;
            }
        }
        assert!(runs.len() > 5, "it does change: {runs:?}");
        // Every hold but the first lasts at least the commitment.
        assert!(
            runs.iter().skip(1).all(|r| *r >= 240),
            "held at least two seconds: {runs:?}"
        );
        // Without commitment it changes far more often.
        let loose = BotSurprise {
            commit_seconds: 0.0,
            ..cfg.clone()
        };
        let changes = |p: &[u32]| p.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(changes(&run(&mut Mind::new(11), &loose, &options, 6000)) > 10 * runs.len());
    }

    /// A commitment gives way once its option is no longer near the best:
    /// a changed situation is not ignored.
    #[test]
    fn commitment_yields_when_the_option_falls_out_of_the_band() {
        let cfg = BotSurprise {
            commit_seconds: 100.0,
            tell_seconds: 0.0,
            ..on(1.0)
        };
        let mut mind = Mind::new(5);
        let mut held = None;
        for tick in 0..100 {
            held = Some(
                mind.pick(
                    &cfg,
                    Domain::Aim,
                    &[(0, 1.0), (1, 1.0)],
                    0,
                    held,
                    OPEN,
                    tick,
                )
                .option,
            );
        }
        let held = held.unwrap();
        let other = 1 - held;
        let mut scores = [(0, 0.0), (1, 0.0)];
        scores[other as usize].1 = 1.0;
        scores[held as usize].1 = 0.2;
        let p = mind.pick(&cfg, Domain::Aim, &scores, other, Some(held), OPEN, 100);
        assert_eq!(p.option, other);
    }

    /// Carrying an objective or being urgent: the plain pick, always.
    #[test]
    fn gating_keeps_the_plain_pick() {
        let cfg = BotSurprise {
            commit_seconds: 0.0,
            ..on(1.0)
        };
        for gate in [
            Gate {
                carrying: true,
                urgent: false,
            },
            Gate {
                carrying: false,
                urgent: true,
            },
        ] {
            let mut mind = Mind::new(9);
            for tick in 0..3000 {
                let p = mind.pick(
                    &cfg,
                    Domain::Weapon,
                    &[(0, 1.0), (1, 1.0), (2, 1.0)],
                    2,
                    Some(1),
                    gate,
                    tick,
                );
                assert_eq!(p.option, 2);
                assert!(!p.telling);
            }
        }
    }

    /// Options that cannot work now (no score) are never picked, however
    /// the weights fall.
    #[test]
    fn the_viability_filter_excludes_impossible_options() {
        let cfg = BotSurprise {
            commit_seconds: 0.0,
            band: 1.0,
            ..on(1.0)
        };
        let options = [(0, 1.0), (1, 0.0), (2, f32::NAN), (3, -1.0), (4, 0.9)];
        let picks = run(&mut Mind::new(2), &cfg, &options, 4000);
        assert!(picks.iter().all(|p| [0, 4].contains(p)), "{picks:?}");
        assert!(picks.contains(&4));
    }

    /// A switch the variation causes is preceded by a tell: the old option
    /// is held, flagged, for the tell's length; then it switches.
    #[test]
    fn a_tell_precedes_a_switch() {
        let cfg = BotSurprise {
            commit_seconds: 0.5,
            tell_seconds: 0.25,
            ..on(1.0)
        };
        let options = [(0, 1.0), (1, 1.0)];
        let mut mind = Mind::new(4);
        let mut current = 0;
        let mut switches = 0;
        let mut telling_since = None;
        for tick in 0..20_000 {
            let p = mind.pick(&cfg, Domain::Weapon, &options, 0, Some(current), OPEN, tick);
            if p.telling {
                assert_eq!(p.option, current, "the tell holds what it had");
                assert!(mind.telling(tick));
                telling_since.get_or_insert(tick);
            } else if p.option != current && p.option != 0 {
                // A switch away from the plain pick (back to it needs none).
                let since = telling_since.expect("a tell before the switch");
                assert!(tick - since >= 30, "the tell lasted {} ticks", tick - since);
                switches += 1;
            }
            if !p.telling {
                telling_since = None;
            }
            current = p.option;
        }
        assert!(switches > 3, "{switches} switches away from the plain pick");
        // Aim switches show nothing: no tell.
        let mut mind = Mind::new(4);
        for tick in 0..5000 {
            assert!(
                !mind
                    .pick(&cfg, Domain::Aim, &options, 0, Some(0), OPEN, tick)
                    .telling
            );
        }
    }

    /// An option that keeps failing loses out: torso shots dodged, it
    /// aims at the feet; a weapon that never lands gives way to another.
    #[test]
    fn effectiveness_decay_shifts_picks() {
        let cfg = BotSurprise {
            commit_seconds: 0.5,
            tell_seconds: 0.0,
            band: 0.1,
            ..on(1.0)
        };
        // Torso plainly best, feet a bit behind.
        let options = [(AIM_TORSO, 1.0), (AIM_FEET, 0.8)];
        let share = |fail_torso: bool| {
            let mut mind = Mind::new(21);
            let mut feet = 0;
            for tick in 0..12_000 {
                let p = mind.pick(&cfg, Domain::Aim, &options, AIM_TORSO, None, OPEN, tick);
                if p.option == AIM_FEET {
                    feet += 1;
                }
                if tick % 120 == 0 {
                    mind.outcome(
                        &cfg,
                        Domain::Aim,
                        p.option,
                        !(fail_torso && p.option == 0),
                        tick,
                    );
                }
            }
            feet
        };
        assert_eq!(share(false), 0, "out of the band: never the feet");
        assert!(share(true) > 6000, "dodged torso shots: {}", share(true));
        // A weapon twice as good on paper that never lands.
        let other = |fails: bool| {
            let mut mind = Mind::new(1);
            let weapons = [(0, 2.0), (1, 1.0)];
            let (mut last, mut other) = (0, 0);
            for tick in 0..12_000 {
                last = mind
                    .pick(&cfg, Domain::Weapon, &weapons, 0, Some(last), OPEN, tick)
                    .option;
                other += u32::from(last == 1);
                if tick % 60 == 0 && last == 0 {
                    mind.outcome(&cfg, Domain::Weapon, 0, !fails, tick);
                }
            }
            other
        };
        assert_eq!(other(false), 0, "the better weapon while it lands");
        assert!(other(true) > 8000, "the one that works: {}", other(true));
    }

    /// Boredom: an option long in use loses weight to its peers.
    #[test]
    fn boredom_and_drift_move_the_weights() {
        let cfg = BotSurprise {
            boredom: 0.1,
            ..on(1.0)
        };
        let mut mind = Mind::new(13);
        for tick in 0..2400 {
            mind.accrue(&cfg, Domain::Route, 0, 1.0 / TICKS, tick);
        }
        let at = mind.drive(&cfg, Domain::Route, 0, 2400);
        let d = mind.drives[at];
        assert!(d.boredom > 0.5, "{}", d.boredom);
        let at = mind.drive(&cfg, Domain::Route, 0, 2400 + 120 * 60);
        let later = mind.drives[at];
        assert!(later.boredom < 0.2 * d.boredom, "it fades out of use");
        assert!(later.drift != 0.0 && later.drift.abs() <= cfg.drift);
    }

    /// No flavour interrupt while carrying an objective, and one under way
    /// ends when the bot takes one up.
    #[test]
    fn no_interrupts_while_carrying() {
        let cfg = BotSurprise {
            interrupts_per_minute: 60.0,
            interrupt_cooldown_seconds: 0.0,
            interrupt_seconds: 1000.0,
            ..on(1.0)
        };
        let mut pause = Pause {
            natural: true,
            gate: Gate {
                carrying: true,
                urgent: false,
            },
            ..Default::default()
        };
        let mut mind = Mind::new(17);
        for tick in 0..60_000 {
            assert!(matches!(mind.interrupt(&cfg, &pause, tick), Moment::None));
        }
        pause.gate.carrying = false;
        let mut began = None;
        for tick in 60_000..120_000 {
            if let Moment::Begin(_) = mind.interrupt(&cfg, &pause, tick) {
                began = Some(tick);
                break;
            }
        }
        let began = began.expect("interrupts at a free pause");
        assert!(matches!(
            mind.interrupt(&cfg, &pause, began + 1),
            Moment::Continue(_)
        ));
        pause.gate.carrying = true;
        assert!(matches!(
            mind.interrupt(&cfg, &pause, began + 2),
            Moment::End(_)
        ));
        assert!(mind.flavour().is_none());
        // Not at a busy moment either, nor one needing a player it lacks.
        let busy = Pause {
            natural: false,
            ..Default::default()
        };
        for tick in 200_000..260_000 {
            assert!(matches!(mind.interrupt(&cfg, &busy, tick), Moment::None));
        }
        let mut mind = Mind::new(18);
        let free = Pause {
            natural: true,
            ..Default::default()
        };
        let short = BotSurprise {
            interrupt_seconds: 0.5,
            ..cfg
        };
        let mut began = 0;
        for tick in 0..200_000 {
            if let Moment::Begin(i) = mind.interrupt(&short, &free, tick) {
                began += 1;
                assert!(
                    !matches!(
                        i.flavour,
                        Flavour::Stare | Flavour::Spray | Flavour::Tool | Flavour::Drop
                    ),
                    "{:?} needs what it lacks",
                    i.flavour
                );
            }
        }
        assert!(began > 100, "{began} interrupts");
    }

    /// The behaviour hook trades only near, ordinary behaviours: never away
    /// from carrying a catch, arming or walking home.
    #[test]
    fn behaviour_variation_keeps_must_do_behaviours() {
        let cfg = BotSurprise {
            band: 1.0,
            commit_seconds: 0.0,
            tell_seconds: 0.0,
            ..on(1.0)
        };
        let mut scores = [0.0; Behaviour::COUNT];
        scores[Behaviour::Carry as usize] = 1.0;
        scores[Behaviour::Fight as usize] = 0.8;
        scores[Behaviour::Chase as usize] = 0.6;
        scores[Behaviour::Wander as usize] = 0.1;
        let mut mind = Mind::new(1);
        for tick in 0..2000 {
            let (b, _) = behaviour(
                &mut mind,
                &cfg,
                &scores,
                Behaviour::Carry,
                Behaviour::Carry,
                OPEN,
                tick,
            );
            assert_eq!(b, Behaviour::Carry);
        }
        let mut seen = Vec::new();
        for tick in 0..4000 {
            let (b, _) = behaviour(
                &mut mind,
                &cfg,
                &scores,
                Behaviour::Fight,
                Behaviour::Fight,
                OPEN,
                tick,
            );
            assert_ne!(b, Behaviour::Carry);
            seen.push(b);
        }
        assert!(seen.contains(&Behaviour::Chase), "{seen:?}");
    }
}
