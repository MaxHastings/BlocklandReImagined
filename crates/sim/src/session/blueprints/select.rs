//! A selection as a copy job: the stack or box found a slice at a time,
//! then each brick taken into the copy a slice at a time.
use super::*;
use crate::blueprint::CopyBuilder;
use crate::session::copy_jobs::{CopyWork, Ending, Progress};
use crate::simulation::{BoxScan, Selection, StackScan, spend, work};

/// A brick taken into a copy: read under its highlight and stored small.
const CAPTURE: u32 = work::EDIT * 2;

enum Finding {
    Stack(StackScan),
    Box { scan: BoxScan, limited: bool },
    Found,
}

/// Copying the stack at a brick or the bricks in a box for a player.
pub(in crate::session) struct SelectWork {
    finding: Finding,
    actor: Actor,
    rule: CopyRule,
    limit: usize,
    hold: CopyHold,
    package: String,
    selection: Selection,
    /// The bricks to take, in order: those held already first when adding.
    ids: Vec<BrickId>,
    next: usize,
    /// Taken already, when adding to a held copy.
    taken: Option<crate::id_map::IdSet>,
    builder: CopyBuilder,
    sources: Vec<BrickId>,
    area: Option<(Vec3, Vec3)>,
    /// Why it took nothing, once found.
    refusal: Option<Refusal>,
}

/// Why a selection took nothing: `trust`, `public`, `empty` or
/// `invalid`, and the words for it.
type Refusal = (&'static str, String);

impl SelectWork {
    #[allow(clippy::too_many_arguments)]
    fn new(
        s: &Session,
        owner: OwnerId,
        finding: Finding,
        limit: usize,
        rule: CopyRule,
        tool: &str,
        package: &str,
        hold: CopyHold,
    ) -> Result<Self> {
        let actor = s.peers.get(&owner).context("Unknown connection")?.actor.clone();
        Ok(Self {
            finding,
            actor,
            rule,
            limit,
            hold,
            package: package.into(),
            selection: Selection::default(),
            ids: Vec::new(),
            next: 0,
            taken: None,
            builder: CopyBuilder::new(tool),
            sources: Vec::new(),
            area: None,
            refusal: None,
        })
    }

    /// The stack at `brick` ([`crate::simulation::Simulation::select_stack`]).
    #[allow(clippy::too_many_arguments)]
    pub fn stack(
        s: &Session,
        owner: OwnerId,
        brick: BrickId,
        limit: usize,
        reach: StackReach,
        rule: CopyRule,
        tool: &str,
        package: &str,
        hold: CopyHold,
    ) -> Result<std::result::Result<Self, Refusal>> {
        s.check_copy(limit, tool)?;
        let actor = &s.peers.get(&owner).context("Unknown connection")?.actor;
        let first = s
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("Unknown brick")?;
        if !admits(actor, rule, first) {
            return Ok(Err(if first.owner == 0 {
                ("public", "Public bricks cannot be copied.".to_string())
            } else {
                (
                    "trust",
                    "The brick's owner does not trust you enough to do that.".to_string(),
                )
            }));
        }
        let reach = crate::simulation::StackReach {
            up: reach.up,
            limited: reach.limited,
        };
        let scan = StackScan::new(&s.simulation, brick, reach, limit)?;
        Self::new(s, owner, Finding::Stack(scan), limit, rule, tool, package, hold).map(Ok)
    }

    /// The bricks in the box `min` to `max` ([`Session::copy_box`]).
    #[allow(clippy::too_many_arguments)]
    pub fn boxed(
        s: &Session,
        owner: OwnerId,
        (min, max): ([f32; 3], [f32; 3]),
        limited: bool,
        limit: usize,
        rule: CopyRule,
        tool: &str,
        package: &str,
        hold: CopyHold,
    ) -> Result<Self> {
        s.check_copy(limit, tool)?;
        let area = grid_box(min, max)?;
        let scan = BoxScan::new(&s.simulation, area, limited, limit);
        Self::new(
            s,
            owner,
            Finding::Box { scan, limited },
            limit,
            rule,
            tool,
            package,
            hold,
        )
    }

    /// Done finding: what to take, or why nothing.
    fn found(&mut self, s: &Session, owner: OwnerId) -> std::result::Result<(), Refusal> {
        let finding = std::mem::replace(&mut self.finding, Finding::Found);
        let selection = match finding {
            Finding::Stack(scan) => scan.selection,
            Finding::Box { scan, limited } => {
                let selection = scan.selection;
                if selection.bricks.is_empty() && selection.refused > 0 {
                    return Err((
                        "trust",
                        "The bricks in that box belong to builds that do not trust you enough."
                            .to_string(),
                    ));
                }
                if selection.bricks.is_empty() {
                    return Err((
                        "empty",
                        if limited {
                            "There are no bricks wholly inside that box."
                        } else {
                            "There are no bricks in that box."
                        }
                        .to_string(),
                    ));
                }
                selection
            }
            Finding::Found => unreachable!("found once"),
        };
        self.selection = Selection {
            bricks: Vec::new(),
            ..selection
        };
        // Added to the copy held from the same Add-On: its bricks first.
        let held = s
            .copies
            .get(&owner)
            .filter(|c| self.hold.add && c.package == self.package);
        self.ids = match held {
            Some(held) => {
                self.taken = Some(Default::default());
                let mut ids = Vec::with_capacity(held.sources.len() + selection.bricks.len());
                ids.extend_from_slice(&held.sources);
                ids.extend(selection.bricks);
                ids
            }
            None => selection.bricks,
        };
        Ok(())
    }

    /// The copy taken, held by `owner`; or why there is none.
    pub fn complete(self, s: &mut Session, owner: OwnerId) -> Copied {
        let failed = |error| Copied {
            selection: Default::default(),
            error: Some(error),
        };
        if let Some(refusal) = self.refusal {
            return failed(refusal);
        }
        let blueprint = match self.builder.finish() {
            Ok(blueprint) => blueprint,
            Err(error) => return failed(("invalid", format!("{error:#}"))),
        };
        let mut held = HeldCopy::new(self.sources, &self.package, self.rule.partial);
        held.shown = !self.hold.hidden;
        held.area = self.area;
        let selection = Selection {
            bricks: held.sources.to_vec(),
            ..self.selection
        };
        s.hold_blueprint(owner, Arc::new(blueprint), held);
        Copied {
            selection,
            error: None,
        }
    }
}

impl CopyWork for SelectWork {
    fn progress(&self) -> Progress {
        match &self.finding {
            Finding::Stack(scan) => Progress {
                action: "select",
                done: scan.selection.bricks.len(),
                total: 0,
                placed: 0,
                refused: 0,
            },
            Finding::Box { scan, .. } => Progress {
                action: "select",
                done: scan.selection.bricks.len(),
                total: 0,
                placed: 0,
                refused: 0,
            },
            Finding::Found => Progress {
                action: "select",
                done: self.next,
                total: self.ids.len(),
                placed: 0,
                refused: 0,
            },
        }
    }

    fn step(&mut self, s: &mut Session, owner: OwnerId, budget: &mut u32) -> Result<bool> {
        let (actor, rule) = (&self.actor, self.rule);
        let found = match &mut self.finding {
            Finding::Stack(scan) => scan.step(&s.simulation, budget, |b| admits(actor, rule, b))?,
            Finding::Box { scan, .. } => scan.step(&s.simulation, budget, |b| admits(actor, rule, b)),
            Finding::Found => true,
        };
        if !found {
            return Ok(false);
        }
        if !matches!(self.finding, Finding::Found)
            && let Err(refusal) = self.found(s, owner)
        {
            // Nothing to take: said when the job finishes.
            self.refusal = Some(refusal);
            return Ok(true);
        }
        let world = s.simulation.state();
        let definitions = &s.simulation.definitions;
        while let Some(&id) = self.ids.get(self.next) {
            if !spend(budget, CAPTURE) {
                return Ok(false);
            }
            self.next += 1;
            // Gone since it was found.
            let Some(brick) = world.bricks.get(&id) else {
                continue;
            };
            if self.taken.as_mut().is_some_and(|taken| !taken.insert(id)) {
                continue;
            }
            if self.builder.len() >= self.limit {
                self.selection.limit_reached = true;
                break;
            }
            let brick = s.unlit(id, brick);
            self.builder.push(&brick, definitions)?;
            let (low, high) = crate::definitions::brick_box(&brick, &definitions.get(&brick)?.mesh);
            self.area = Some(match self.area {
                Some((a, b)) => (a.min(low), b.max(high)),
                None => (low, high),
            });
            self.sources.push(id);
        }
        Ok(true)
    }

    fn finish(self: Box<Self>, s: &mut Session, owner: OwnerId, ending: Ending) {
        let package = self.package.clone();
        let copied = match ending {
            Ending::Done => self.complete(s, owner),
            Ending::Left => return,
            Ending::Canceled => Copied {
                selection: Default::default(),
                error: Some(("canceled", "Selection canceled!".to_string())),
            },
            Ending::Failed(error) => Copied {
                selection: Default::default(),
                error: Some(("invalid", format!("{error:#}"))),
            },
        };
        s.report_copy(&package, owner, copied);
    }
}
