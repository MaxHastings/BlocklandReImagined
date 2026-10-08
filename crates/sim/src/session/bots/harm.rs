//! What a shot would do to each side: one reading of harm for every weapon
//! (`docs/architecture/bots.md`, "Harm").
//!
//! Before it fires, a bot predicts where the shot goes and what it does
//! there, from the weapon's data and the host's own rules: the way the shot
//! flies (the shot chooser's own chords, `combat::clear_path` and
//! `combat::burst`, turned by the bot's aim error, so the shot it will
//! really fire), the body that way meets first (its direct hit), the bodies
//! its pellets spread over (each by the share of v20's spread box it
//! fills), and its blast where it ends (the host's falloff). Each body's
//! harm counts only where the host's damage rules let this shooter hurt it,
//! and at most the health it has left. The sum for each side is the
//! shot's [`Harm`]; `tactics::worth` trades it one for one.
//!
//! What a shot sweeps ([`Shape`]) is kept with the plan, so each tick the
//! trigger only asks whether a body has stepped into it since.
use super::tactics::{Capability, Harm};
use super::*;

/// Who a body is to the shooter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Own,
    Ally,
    Enemy,
    /// Neither side's (a bystander): it stops a shot, its harm counts for
    /// nothing.
    Other,
}

/// A living body a shot may meet, as one decision sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Body {
    pub owner: OwnerId,
    pub side: Side,
    /// It rides the shooter's vehicle: shots leave from among the crew.
    pub crew: bool,
    pub centre: Vec3,
    /// Half its height and half its width.
    pub half: f32,
    pub width: f32,
    pub velocity: Vec3,
    pub health: f32,
    /// The host lets the shooter's hit, and its blast, hurt it.
    pub direct: bool,
    pub blast: bool,
}
impl Body {
    /// Where it is `seconds` from now, as it moves.
    fn at(&self, seconds: f32) -> Vec3 {
        self.centre + self.velocity * seconds
    }
}

/// Every living body about, looked up once a decision (`bot_allies` and
/// the damage rules are the dear part).
#[derive(Clone, Debug, Default)]
pub(super) struct Bodies(pub Vec<Body>);
impl Bodies {
    /// The bodies `bot` would shoot among, its own moved by `shift` (to a
    /// place it weighs standing on).
    pub(super) fn of(session: &Session, bot: OwnerId, shift: Vec3) -> Self {
        let mount = session.mounted(bot).map(|(v, _)| v);
        let kind = session.bots.brains.get(&bot).map(|b| &b.kind);
        let mut bodies = Vec::new();
        for (owner, peer) in &session.peers {
            if !peer.combat.alive {
                continue;
            }
            let side = if *owner == bot {
                Side::Own
            } else if session.bot_allies(bot, *owner) {
                Side::Ally
            } else if kind.is_some_and(|k| session.bot_enemy(bot, k, *owner)) {
                Side::Enemy
            } else {
                Side::Other
            };
            // Spawn protection spares a body the shot passes. Its own shot
            // ends its own; an enemy's the fire gate waits out
            // (`combat::validate_intent`), so the shot lands after it.
            let protected =
                !matches!(side, Side::Own | Side::Enemy) && session.spawn_protected(*owner);
            // The tuning a player keeps has its scale in it.
            let tuning = peer.player.tuning();
            let half = tuning.stand_height * 0.5;
            let mut centre = Vec3::from(peer.player.state().feet) + Vec3::Y * half;
            if side == Side::Own {
                centre += shift;
            }
            bodies.push(Body {
                owner: *owner,
                side,
                crew: *owner != bot
                    && mount.is_some()
                    && mount == session.mounted(*owner).map(|(v, _)| v),
                centre,
                half,
                width: tuning.width * 0.5,
                velocity: Vec3::from(peer.player.state().velocity),
                health: peer.combat.health,
                direct: !protected && session.can_damage_player(bot, *owner, false),
                blast: !protected && session.can_damage_player(bot, *owner, true),
            });
        }
        Self(bodies)
    }
}

/// One stretch of a shot's way: from, to, and the seconds from firing to
/// `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Chord {
    pub from: Vec3,
    pub to: Vec3,
    pub seconds: f32,
}

/// What a planned shot sweeps, kept with the plan: its way and the sphere
/// its blast reaches where it ends, and the bodies whose harm the plan
/// already counted.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Shape {
    pub chords: Vec<Chord>,
    pub burst: Option<(Vec3, f32)>,
    pub priced: Vec<OwnerId>,
}
impl Shape {
    /// Whether a body of half-size `margin` centred at `point` is in it,
    /// the way widened by `spread` per unit from where the shot leaves.
    pub(super) fn holds(&self, point: Vec3, margin: f32, spread: f32) -> bool {
        let origin = self.chords.first().map(|c| c.from);
        self.chords.iter().any(|c| {
            let line = c.to - c.from;
            let length = line.length().max(1e-6);
            let along = ((point - c.from).dot(line) / length).clamp(0.0, length);
            let at = c.from + line / length * along;
            let out = origin.map_or(0.0, |o| o.distance(at));
            point.distance(at) < margin + out * spread
        }) || self
            .burst
            .is_some_and(|(at, radius)| point.distance(at) < radius + margin)
    }
    /// Where the shot ends.
    pub(super) fn end(&self) -> Option<Vec3> {
        self.chords.last().map(|c| c.to)
    }
}

/// What one attack does, from its data: its hit per projectile, how many
/// it fires and how they spread, its blast, and for a swing the body it
/// strikes.
#[derive(Clone, Copy, Debug)]
pub(super) struct Strike {
    pub direct: f32,
    pub pellets: u32,
    /// Each pellet turns by up to this about each cross axis, in radians
    /// (`fire::image_spread`).
    pub spread: f32,
    /// Its blast: damage, damage radius and the reach of its fragments
    /// past it (`Capability::danger`), all scaled.
    pub blast: Option<(f32, f32, f32)>,
    /// A swing: the body it strikes, if any.
    pub contact: Option<Option<OwnerId>>,
    /// It bounces off a body it meets (a timed throw), rather than going
    /// off there.
    pub rebounds: bool,
}
impl Strike {
    /// The strike of `cap`, whose projectile `def` its image fires `pellets`
    /// of with `spread`, at `scale`.
    pub(super) fn of(
        cap: Capability,
        def: Option<&bri_weapons::ProjectileDef>,
        pellets: u32,
        spread: f32,
        scale: f32,
    ) -> Self {
        let blast = def
            .filter(|_| cap.splash_damage > 0.0)
            .map(|d| (cap.splash_damage, d.explosion.radius * scale, cap.danger));
        Self {
            direct: cap.direct_damage,
            pellets: pellets.max(1),
            spread,
            blast,
            contact: None,
            rebounds: matches!(cap.delivery, super::tactics::Delivery::Timed { .. }),
        }
    }
}

/// The way a shot fired along `aimed` from `origin` really goes: each
/// chord turned about `origin` by the aim `error` (yaw, pitch), as the bot
/// will fire it.
pub(super) fn turned(origin: Vec3, chords: &[Chord], error: (f32, f32)) -> Vec<Chord> {
    let turn = |p: Vec3| {
        let v = p - origin;
        let length = v.length();
        if length < 1e-6 {
            return p;
        }
        let d = v / length;
        let yaw = d.x.atan2(-d.z) + error.0;
        let pitch = d.y.clamp(-1.0, 1.0).asin() + error.1;
        origin
            + Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            ) * length
    };
    chords
        .iter()
        .map(|c| Chord {
            from: turn(c.from),
            to: turn(c.to),
            ..*c
        })
        .collect()
}

/// Whether `body`, grown by how far it may move by `seconds`, stands on the
/// chord from `a` to `b`; how far along it (0 to 1) if so.
fn meets(body: &Body, a: Vec3, b: Vec3, seconds: f32) -> Option<f32> {
    let grow = body.velocity.length() * seconds;
    let line = b - a;
    let t = ((body.centre - a).dot(line) / line.length_squared().max(1e-9)).clamp(0.0, 1.0);
    let at = a + line * t;
    let flat = Vec3::new(at.x - body.centre.x, 0.0, at.z - body.centre.z).length();
    (flat <= body.width + grow && (at.y - body.centre.y).abs() <= body.half + grow).then_some(t)
}

/// The share of v20's spread box (±`spread` about each cross axis) that
/// `body` fills seen from `origin` along `direction`, less what nearer
/// bodies in `shade` (their clipped angular boxes) already cover; its own
/// clipped box is added to `shade`.
fn spread_share(
    origin: Vec3,
    direction: Vec3,
    spread: f32,
    body: &Body,
    shade: &mut Vec<[f32; 4]>,
) -> f32 {
    let right = direction.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = right.cross(direction);
    let rel = body.centre - origin;
    let along = rel.dot(direction);
    if along <= 0.0 || spread <= 0.0 {
        return 0.0;
    }
    let (x, y) = (rel.dot(right).atan2(along), rel.dot(up).atan2(along));
    let (ax, ay) = (body.width.atan2(along), body.half.atan2(along));
    let clip = |lo: f32, hi: f32| (lo.max(-spread), hi.min(spread));
    let (x0, x1) = clip(x - ax, x + ax);
    let (y0, y1) = clip(y - ay, y + ay);
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }
    let area = |b: [f32; 4]| (b[1] - b[0]).max(0.0) * (b[3] - b[2]).max(0.0);
    let mine = [x0, x1, y0, y1];
    let hidden: f32 = shade
        .iter()
        .map(|s| area([s[0].max(x0), s[1].min(x1), s[2].max(y0), s[3].min(y1)]))
        .sum();
    shade.push(mine);
    ((area(mine) - hidden).max(0.0) / (4.0 * spread * spread)).min(1.0)
}

/// What a shot along `chords` (already turned by the aim error) from
/// `origin` does to each side, and what it sweeps. The way ends at the
/// first body it meets; pellets spread over the bodies in their box; the
/// blast hits every body around where the way ends.
pub(super) fn shot_harm(
    bodies: &Bodies,
    origin: Vec3,
    chords: &[Chord],
    strike: &Strike,
) -> (Harm, Shape) {
    let mut harm = vec![0.0f32; bodies.0.len()];
    let mut way: Vec<Chord> = Vec::with_capacity(chords.len());
    let direction = chords
        .first()
        .map_or(Vec3::ZERO, |c| (c.to - c.from).normalize_or_zero());
    let mut end = chords.last().map_or(origin, |c| c.to);
    let mut seconds = chords.last().map_or(0.0, |c| c.seconds);
    // The bodies its way meets: priced by the plan whatever they take (a
    // push's target takes no damage, yet the shot is meant for it).
    let mut met = Vec::new();
    // A throw that bounces off a body short of where it is meant to go off
    // goes off somewhere the plan cannot say, perhaps back by the thrower.
    let mut rebound = None;
    if let Some(struck) = strike.contact {
        // A swing strikes what the host's own trace says it strikes.
        way.extend_from_slice(chords);
        if let Some(i) = struck.and_then(|o| bodies.0.iter().position(|b| b.owner == o)) {
            met.push(bodies.0[i].owner);
            end = bodies.0[i].centre;
            if bodies.0[i].direct {
                harm[i] += strike.direct;
            }
        }
    } else if strike.pellets > 1 && strike.spread > 0.0 {
        // Each pellet flies the spread box; a body takes the share of the
        // pellets that fill it, nearer bodies shading farther ones, as far
        // as the way goes before the world stops it.
        way.extend_from_slice(chords);
        let reach = origin.distance(end);
        let mut order: Vec<usize> = (0..bodies.0.len())
            .filter(|i| {
                let b = &bodies.0[*i];
                b.side != Side::Own && !b.crew
            })
            .collect();
        order.sort_by(|a, b| {
            let d = |i: &usize| (bodies.0[*i].centre - origin).dot(direction);
            d(a).total_cmp(&d(b))
        });
        let mut shade = Vec::new();
        for i in order {
            let b = &bodies.0[i];
            if (b.centre - origin).dot(direction) > reach + b.width {
                continue;
            }
            let share = spread_share(origin, direction, strike.spread, b, &mut shade);
            if share > 0.0 {
                met.push(b.owner);
            }
            if b.direct {
                harm[i] += strike.direct * strike.pellets as f32 * share;
            }
        }
    } else {
        // One projectile: the first body on its way ends it.
        'chords: for c in chords {
            let first = bodies
                .0
                .iter()
                .enumerate()
                .filter(|(_, b)| b.side != Side::Own && !b.crew)
                .filter_map(|(i, b)| meets(b, c.from, c.to, c.seconds).map(|t| (t, i)))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            match first {
                Some((t, i)) => {
                    let at = c.from + (c.to - c.from) * t;
                    let planned = chords.last().map_or(at, |c| c.to);
                    let reach = strike
                        .blast
                        .map_or(0.0, |(_, radius, danger)| radius + danger);
                    if strike.rebounds && at.distance(planned) > reach {
                        rebound = Some(origin);
                    }
                    way.push(Chord { to: at, ..*c });
                    end = at;
                    seconds = c.seconds;
                    met.push(bodies.0[i].owner);
                    if bodies.0[i].direct {
                        harm[i] += strike.direct;
                    }
                    break 'chords;
                }
                None => way.push(*c),
            }
        }
    }
    // Its blast where the way ends, by the host's falloff, to the centre
    // of each body; within its fragments' reach, all of it. Its own side
    // counts at the nearer of where it is and where it is heading, since
    // it may stop or turn back.
    let burst = strike.blast.map(|(damage, radius, danger)| {
        for (i, b) in bodies.0.iter().enumerate() {
            if !b.blast {
                continue;
            }
            let ahead = end.distance(b.at(seconds));
            // Bounced short, it is priced at the worse for its side: for
            // enemies where it would be farthest from them, for its own side
            // where nearest, back at the thrower included.
            let back = rebound.map(|o: Vec3| o.distance(b.at(seconds)).min(o.distance(b.centre)));
            let d = match b.side {
                Side::Enemy | Side::Other => ahead.max(back.unwrap_or(0.0)),
                Side::Own | Side::Ally => ahead
                    .min(end.distance(b.centre))
                    .min(back.unwrap_or(f32::INFINITY)),
            };
            let share = if danger > 0.0 && d <= radius + danger {
                1.0
            } else {
                bri_weapons::runtime::blast_falloff(d, radius)
            };
            harm[i] += damage * share;
        }
        (end, radius + danger)
    });
    let mut total = Harm::default();
    let mut priced = met;
    for (b, h) in bodies.0.iter().zip(harm) {
        let h = h.min(b.health.max(0.0));
        if h <= 0.0 {
            continue;
        }
        if !priced.contains(&b.owner) {
            priced.push(b.owner);
        }
        match b.side {
            Side::Own => total.own += h,
            Side::Ally => {
                total.ally += h;
                total.kills_ally |= h >= b.health;
            }
            Side::Enemy => total.enemy += h,
            Side::Other => {}
        }
    }
    (
        total,
        Shape {
            chords: way,
            burst,
            priced,
        },
    )
}

/// Puts `a` and `b` in one new mini-game, where weapons hurt (outside
/// mini-games they never do): for fixtures whose bots fight.
#[cfg(test)]
pub(super) fn one_game(s: &mut Session, a: OwnerId, b: OwnerId) {
    use crate::session::{Command, MiniGameRequest};
    // Joining a game respawns its players: each goes back where it stood.
    let stood = [a, b].map(|o| {
        let p = &s.peers[&o].player;
        (o, Vec3::from(p.state().feet), p.state().yaw)
    });
    s.command(
        a,
        1,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            // Keep what each holds: a test pack has no stock items.
            settings: bri_minigames::Settings {
                loadout: Default::default(),
                ..Default::default()
            },
        }),
    )
    .unwrap();
    let game = s.game_of(a).unwrap();
    s.command(
        b,
        1,
        Command::MiniGame(MiniGameRequest::Join { game: game.0 }),
    )
    .unwrap();
    for (owner, feet, yaw) in stood {
        let peer = s.peers.get_mut(&owner).unwrap();
        peer.player
            .teleport(&mut s.simulation.physics, feet, yaw)
            .unwrap();
        // Past its spawn protection, as if it had fired.
        peer.combat.shot_once = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(owner: OwnerId, side: Side, centre: Vec3) -> Body {
        Body {
            owner,
            side,
            crew: false,
            centre,
            half: 1.0,
            width: 0.5,
            velocity: Vec3::ZERO,
            health: 100.0,
            direct: true,
            blast: true,
        }
    }
    fn straight(to: Vec3) -> Vec<Chord> {
        vec![Chord {
            from: Vec3::ZERO,
            to,
            seconds: 0.0,
        }]
    }

    #[test]
    fn a_teammate_filling_half_the_spread_box_takes_half_the_pellets() {
        // A box of ±0.1 rad about each axis: at 10 units ±1 across. A body
        // 2 wide and 2 high, its edge on the line: half the box's width and
        // all its height.
        let spread: f32 = 0.1;
        let edge = 10.0 * spread.tan();
        let mut ally = body(2, Side::Ally, Vec3::new(edge * 0.5, 0.0, 10.0));
        (ally.width, ally.half) = (edge * 0.5, 10.0);
        let strike = Strike {
            direct: 10.0,
            pellets: 6,
            spread,
            blast: None,
            contact: None,
            rebounds: false,
        };
        let (harm, _) = shot_harm(
            &Bodies(vec![ally]),
            Vec3::ZERO,
            &straight(Vec3::Z * 20.0),
            &strike,
        );
        assert!((harm.ally - 30.0).abs() < 1.5, "{harm:?}");
        // One outside the box takes nothing.
        let wide = body(3, Side::Ally, Vec3::new(5.0, 0.0, 10.0));
        let (harm, _) = shot_harm(
            &Bodies(vec![wide]),
            Vec3::ZERO,
            &straight(Vec3::Z * 20.0),
            &strike,
        );
        assert_eq!(harm.ally, 0.0);
    }

    #[test]
    fn a_blast_hurts_as_the_host_does_at_the_same_distance() {
        let strike = Strike {
            direct: 0.0,
            pellets: 1,
            spread: 0.0,
            blast: Some((80.0, 6.0, 0.0)),
            contact: None,
            rebounds: false,
        };
        let enemy = body(2, Side::Enemy, Vec3::new(3.0, 0.0, 10.0));
        let (harm, shape) = shot_harm(
            &Bodies(vec![enemy]),
            Vec3::ZERO,
            &straight(Vec3::Z * 10.0),
            &strike,
        );
        let host = 80.0 * bri_weapons::runtime::blast_falloff(3.0, 6.0);
        assert!((harm.enemy - host).abs() < 1e-4, "{} vs {host}", harm.enemy);
        assert_eq!(shape.burst, Some((Vec3::Z * 10.0, 6.0)));
    }

    #[test]
    fn the_first_body_on_the_way_takes_the_hit() {
        let strike = Strike {
            direct: 25.0,
            pellets: 1,
            spread: 0.0,
            blast: None,
            contact: None,
            rebounds: false,
        };
        let ally = body(2, Side::Ally, Vec3::new(0.0, 0.0, 5.0));
        let enemy = body(3, Side::Enemy, Vec3::new(0.0, 0.0, 10.0));
        let (harm, shape) = shot_harm(
            &Bodies(vec![enemy, ally]),
            Vec3::ZERO,
            &straight(Vec3::Z * 20.0),
            &strike,
        );
        assert_eq!((harm.ally, harm.enemy), (25.0, 0.0));
        assert!(shape.end().unwrap().z < 6.0);
    }

    #[test]
    fn harm_counts_only_where_the_rules_let_it_hurt_and_at_most_the_health_left() {
        let strike = Strike {
            direct: 0.0,
            pellets: 1,
            spread: 0.0,
            blast: Some((80.0, 6.0, 0.0)),
            contact: None,
            rebounds: false,
        };
        let mut ally = body(2, Side::Ally, Vec3::new(0.0, 0.0, 10.0));
        ally.blast = false;
        let mut enemy = body(3, Side::Enemy, Vec3::new(0.5, 0.0, 10.0));
        enemy.health = 15.0;
        let (harm, _) = shot_harm(
            &Bodies(vec![ally, enemy]),
            Vec3::ZERO,
            &straight(Vec3::Z * 10.0),
            &strike,
        );
        assert_eq!(harm.ally, 0.0, "friendly fire off");
        assert_eq!(harm.enemy, 15.0, "no more than it has");
    }

    fn rocket() -> Strike {
        Strike {
            direct: 0.0,
            pellets: 1,
            spread: 0.0,
            blast: Some((80.0, 6.0, 0.0)),
            contact: None,
            rebounds: false,
        }
    }
    /// Whether the shot is one a full-health shooter takes (`tactics`):
    /// never a teammate's death or its own, and more harm to its enemies
    /// than to its side.
    fn taken(harm: Harm) -> bool {
        super::super::tactics::trade(harm, 100.0, 100.0).is_some_and(|net| net > 0.0)
    }

    #[test]
    fn with_friendly_fire_on_a_shot_past_a_teammate_is_taken_only_when_its_enemies_take_more() {
        // A blast going off at z = 9, an enemy two units off it.
        let enemy = body(2, Side::Enemy, Vec3::new(2.0, 0.0, 9.0));
        let shot = |ally: Body| {
            shot_harm(
                &Bodies(vec![enemy, ally]),
                Vec3::ZERO,
                &straight(Vec3::Z * 9.0),
                &rocket(),
            )
            .0
        };
        // A teammate at the blast's edge takes less than the enemy: the
        // trade is taken.
        let edge = shot(body(3, Side::Ally, Vec3::new(5.5, 0.0, 9.0)));
        assert!(
            edge.ally > 0.0 && edge.ally < edge.enemy && taken(edge),
            "{edge:?}"
        );
        // One where it goes off takes more: not taken.
        let there = body(3, Side::Ally, Vec3::new(0.0, 0.0, 9.5));
        let at = shot(there);
        assert!(at.ally > at.enemy && !taken(at), "{at:?}");
        // Friendly fire off: the teammate takes nothing, wherever it is.
        let mut off = there;
        (off.direct, off.blast) = (false, false);
        let off = shot(off);
        assert_eq!(off.ally, 0.0);
        assert!(taken(off));
    }

    #[test]
    fn a_throw_landing_on_a_teammate_is_no_option_one_arcing_over_it_is() {
        let enemy = body(2, Side::Enemy, Vec3::new(0.0, 0.0, 12.0));
        let arc = vec![
            Chord {
                from: Vec3::new(0.0, 1.0, 0.0),
                to: Vec3::new(0.0, 8.0, 6.0),
                seconds: 0.5,
            },
            Chord {
                from: Vec3::new(0.0, 8.0, 6.0),
                to: Vec3::new(0.0, 0.0, 12.0),
                seconds: 1.0,
            },
        ];
        // A teammate under the arc's top: it flies over.
        let under = body(3, Side::Ally, Vec3::new(0.0, 1.0, 6.0));
        let (harm, _) = shot_harm(&Bodies(vec![enemy, under]), Vec3::ZERO, &arc, &rocket());
        assert!(taken(harm), "{harm:?}");
        // A teammate where it lands, the enemy beside it: no option.
        let there = body(3, Side::Ally, Vec3::new(0.0, 0.0, 12.0));
        let beside = body(2, Side::Enemy, Vec3::new(2.0, 0.0, 12.0));
        let (harm, _) = shot_harm(&Bodies(vec![beside, there]), Vec3::ZERO, &arc, &rocket());
        assert!(!taken(harm), "{harm:?}");
    }

    #[test]
    fn a_throw_a_moving_body_meets_by_the_thrower_goes_off_by_its_own_side() {
        // The acceptance run's case: a throw at a far enemy, and a body
        // walking across its way close to the thrower. It goes off there,
        // by the thrower and its teammate: refused.
        let enemy = body(2, Side::Enemy, Vec3::new(0.0, 0.0, 20.0));
        let mut crossing = body(3, Side::Other, Vec3::new(-2.0, 1.0, 2.0));
        crossing.velocity = Vec3::new(4.0, 0.0, 0.0);
        let ally = body(4, Side::Ally, Vec3::new(1.5, 0.0, 1.0));
        let own = body(1, Side::Own, Vec3::new(0.0, 1.0, 0.0));
        let way = vec![
            Chord {
                from: Vec3::new(0.0, 1.5, 0.5),
                to: Vec3::new(0.0, 2.0, 4.0),
                seconds: 0.25,
            },
            Chord {
                from: Vec3::new(0.0, 2.0, 4.0),
                to: Vec3::new(0.0, 0.0, 20.0),
                seconds: 1.5,
            },
        ];
        let (harm, shape) = shot_harm(
            &Bodies(vec![enemy, crossing, ally, own]),
            Vec3::new(0.0, 1.5, 0.0),
            &way,
            &rocket(),
        );
        assert!(shape.end().unwrap().z < 4.5, "{:?}", shape.end());
        assert!(
            harm.ally > 0.0 && harm.own > 0.0 && !taken(harm),
            "{harm:?}"
        );
    }

    #[test]
    fn the_body_a_push_is_meant_for_is_priced_though_it_takes_no_damage() {
        // A push: no damage, no blast. The trigger keeps fire only while no
        // unpriced body is in the shot's way, so its target must be priced.
        let enemy = body(2, Side::Enemy, Vec3::new(0.0, 0.0, 3.0));
        let push = Strike {
            direct: 0.0,
            pellets: 1,
            spread: 0.0,
            blast: None,
            contact: None,
            rebounds: false,
        };
        let (harm, shape) = shot_harm(
            &Bodies(vec![enemy]),
            Vec3::ZERO,
            &straight(Vec3::Z * 6.0),
            &push,
        );
        assert_eq!(harm.enemy, 0.0);
        assert_eq!(shape.priced, vec![2]);
    }

    #[test]
    fn a_throw_that_bounces_off_a_body_short_of_its_burst_is_priced_back_by_the_thrower() {
        // A timed throw meant to go off at an enemy 12 out, with a body in
        // the way 4 out: it bounces off there, maybe back to the thrower.
        let enemy = body(2, Side::Enemy, Vec3::new(0.0, 0.0, 12.0));
        let blocker = body(3, Side::Other, Vec3::new(0.0, 0.0, 4.0));
        let own = body(1, Side::Own, Vec3::new(0.0, 1.0, 0.0));
        let throw = Strike {
            rebounds: true,
            ..rocket()
        };
        let way = straight(Vec3::new(0.0, 0.0, 12.0));
        let (harm, _) = shot_harm(&Bodies(vec![enemy, blocker, own]), Vec3::ZERO, &way, &throw);
        assert!(
            harm.own > 0.0 && harm.enemy == 0.0 && !taken(harm),
            "{harm:?}"
        );
        // Unblocked it goes off at the enemy.
        let (harm, _) = shot_harm(&Bodies(vec![enemy, own]), Vec3::ZERO, &way, &throw);
        assert!(
            harm.enemy > 0.0 && harm.own == 0.0 && taken(harm),
            "{harm:?}"
        );
    }
}
