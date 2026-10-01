//! The engine side of the script-operation boundary.
//!
//! A script asks for typed operations (`bri_package_runtime::ops`); the
//! runtime's `ops::authorize` checks their shape and the package's
//! capabilities. What the engine then does for each one is its
//! [`Perform`] impl, in the file named like the operation's own module
//! (`ops/player.rs` here performs `ops/player.rs` there). [`perform`]
//! dispatches through the runtime's `for_each_op!` list, so an operation
//! added there without an impl here does not compile, and nothing else in
//! the session matches on `Op`. See docs/architecture/script-operations.md.
use super::*;

mod bots;
mod brick_events;
mod build;
mod chat;
mod damage;
mod effects;
mod entity;
mod environment;
mod lighting;
mod minigame;
mod physics;
mod player;
mod storage;
mod world_edit;

/// Who asked for an operation, and when.
#[derive(Clone, Copy)]
pub(in crate::session) struct OpCall<'a> {
    /// The package whose script asked.
    pub package: &'a str,
    /// The player whose command asked for it: brick changes then need that
    /// player's trust, or a build their minigame plays with
    /// ([`Session::rule_may_edit`]), so a package cannot be used to reach
    /// another player's build (stress campaign W9). Without a caller
    /// (hooks, think, generation) a package acts only on world-owned
    /// bricks, such as its generated world.
    pub caller: Option<OwnerId>,
    /// The simulation tick the operation lands on.
    pub tick: u64,
}

/// What the engine does for one authorized operation. Its impl lives in
/// `perform/<capability>.rs`, unless the state it changes is private to
/// one session system: then in that system's own `perform` module
/// (`movables/perform.rs` for the physics operations that move things).
pub(in crate::session) trait Perform: Sized {
    /// The package entity the operation acts on, which the calling package
    /// must own (`op.not_owner`). Checked with every other operation of the
    /// call before any is performed.
    fn entity(&self) -> Option<u64> {
        None
    }
    /// Carry it out. An error is reported as `op.failed` and does not undo
    /// the call's other operations.
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()>;
}

macro_rules! dispatch {
    ($($op:ident = $module:ident,)*) => {
        /// The package entity `op` acts on ([`Perform::entity`]).
        pub(super) fn entity(op: &Op) -> Option<u64> {
            match op {
                $(Op::$op(op) => Perform::entity(op),)*
            }
        }
        /// Carry out an authorized operation ([`Perform::perform`]).
        pub(super) fn perform(session: &mut Session, op: Op, cx: OpCall<'_>) -> Result<()> {
            match op {
                $(Op::$op(op) => Perform::perform(op, session, cx),)*
            }
        }
    };
}
bri_package_runtime::for_each_op!(dispatch);
