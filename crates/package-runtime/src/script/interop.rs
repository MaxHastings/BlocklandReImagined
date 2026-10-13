//! Add-Ons building on each other: one Add-On's rules call functions
//! another exports (`call_addon`) and hear the events it sends (`emit`,
//! `listens`), so a community framework (an economy, an inventory, a
//! roleplay core) is an Add-On others extend, not an engine feature.
//!
//! A call runs the exported function at once, in the exporting Add-On's own
//! sandbox: its state, its entity variables, its capabilities. What it
//! writes and asks for joins the calling call's transaction, so a failing
//! caller keeps nothing of either, and the host commits each Add-On's part
//! under that Add-On's own name and capabilities
//! ([`Outcome::nested`](super::Outcome)). The calling call's operation
//! budget covers the exported function's work too.
use super::*;

/// Most Add-Ons one chain of calls passes through, the first caller
/// included: `a` calls `b`, which calls `c`. Each call evaluates on the
/// native stack, so the chain stays short.
pub const MAX_CALL_CHAIN: usize = 3;
/// Most events one call may send.
pub const MAX_EVENTS_PER_CALL: usize = 64;

/// What each package offers the others, read from its behaviour.
#[derive(Debug, Clone, Default)]
pub(super) struct Interop {
    /// Exported functions and how many arguments each takes from callers.
    pub(super) exports: BTreeMap<String, u8>,
    pub(super) events: BTreeSet<String>,
    /// The packages whose exports it may call: its dependencies and the
    /// enabled optional ones.
    pub(super) uses: BTreeSet<String>,
}

/// An event a call sent with `emit`.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitted {
    pub name: String,
    pub data: serde_json::Value,
}

/// What another Add-On's exported functions did during one call: its part
/// of the transaction, committed under its own name.
#[derive(Debug, Clone)]
pub struct Nested {
    pub package: String,
    /// Its state before the first call reached it.
    pub input: Namespace,
    pub state: Namespace,
    pub ops: Vec<Op>,
    pub required: BTreeSet<usize>,
    pub entity_vars: EntityVars,
    pub output: Vec<String>,
    pub events: Vec<Emitted>,
}

thread_local! {
    /// The runtime of the running call, for `call_addon` to reach other
    /// packages' scripts (see [`Runtime::call`]).
    pub(super) static RUNTIME: std::cell::Cell<Option<*const Runtime>> =
        const { std::cell::Cell::new(None) };
    /// The packages whose functions are running, outermost first.
    pub(super) static CHAIN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Every other Add-On's part of the running outermost call, in the
    /// order each was first called.
    pub(super) static TRANSACTION: RefCell<Vec<Nested>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("call_addon", |addon: &str, function: &str| {
        call_addon(addon, function, Vec::new())
    });
    engine.register_fn("call_addon", |addon: &str, function: &str, a: Dynamic| {
        call_addon(addon, function, vec![a])
    });
    engine.register_fn(
        "call_addon",
        |addon: &str, function: &str, a: Dynamic, b: Dynamic| {
            call_addon(addon, function, vec![a, b])
        },
    );
    engine.register_fn(
        "call_addon",
        |addon: &str, function: &str, a: Dynamic, b: Dynamic, c: Dynamic| {
            call_addon(addon, function, vec![a, b, c])
        },
    );
    engine.register_fn(
        "call_addon",
        |addon: &str, function: &str, a: Dynamic, b: Dynamic, c: Dynamic, d: Dynamic| {
            call_addon(addon, function, vec![a, b, c, d])
        },
    );
    engine.register_fn(
        "call_addon",
        |addon: &str,
         function: &str,
         a: Dynamic,
         b: Dynamic,
         c: Dynamic,
         d: Dynamic,
         e: Dynamic| { call_addon(addon, function, vec![a, b, c, d, e]) },
    );
    engine.register_fn(
        "call_addon",
        |addon: &str,
         function: &str,
         a: Dynamic,
         b: Dynamic,
         c: Dynamic,
         d: Dynamic,
         e: Dynamic,
         f: Dynamic| { call_addon(addon, function, vec![a, b, c, d, e, f]) },
    );
    engine.register_fn("emit", |name: &str| emit(name, Dynamic::UNIT));
    engine.register_fn("emit", emit);
    engine.register_fn("exports", |addon: &str| {
        with(|i| {
            let runtime = runtime()?;
            let Some(interop) = runtime.interop.get(&i.package) else {
                return Ok(Dynamic::UNIT);
            };
            if addon != i.package && !interop.uses.contains(addon) {
                return Ok(Dynamic::UNIT);
            }
            Ok(runtime.interop.get(addon).map_or(Dynamic::UNIT, |o| {
                Dynamic::from_map(
                    o.exports
                        .iter()
                        .map(|(name, args)| (name.as_str().into(), Dynamic::from_int(i64::from(*args))))
                        .collect(),
                )
            }))
        })
    });
}

fn runtime() -> Fallible<&'static Runtime> {
    match RUNTIME.with(std::cell::Cell::get) {
        // SAFETY: `Runtime::call` stores this pointer from `&self` for the
        // whole outermost call and clears it before returning, and script
        // functions only run inside such a call.
        Some(runtime) => Ok(unsafe { &*runtime }),
        None => fail("no script call is active"),
    }
}

fn emit(name: &str, data: Dynamic) -> Fallible<()> {
    let runtime = runtime()?;
    with(|i| {
        permit_write(i)?;
        let declared = runtime
            .interop
            .get(&i.package)
            .is_some_and(|o| o.events.contains(name));
        if !declared {
            return fail(format!(
                "emit: `{name}` is not one of the events this Add-On's behaviour declares"
            ));
        }
        if i.events.len() >= MAX_EVENTS_PER_CALL {
            return fail(format!(
                "more than {MAX_EVENTS_PER_CALL} events in one call"
            ));
        }
        let data = to_json(&data)?;
        i.events.push(Emitted {
            name: name.to_owned(),
            data,
        });
        Ok(())
    })
}

/// `call_addon(addon, function, args...)`.
fn call_addon(addon: &str, function: &str, args: Vec<Dynamic>) -> Fallible<Dynamic> {
    let runtime = runtime()?;
    // What the nested call needs from the running one; the running one is
    // set aside while the other Add-On's function runs.
    let (from, snapshot, caller, world, read_only) = with(|i| {
        let Some(interop) = runtime.interop.get(&i.package) else {
            return fail("call_addon: this Add-On has no rules");
        };
        if addon == i.package {
            return fail("call_addon: call your own functions directly");
        }
        if !interop.uses.contains(addon) {
            return fail(format!(
                "call_addon: `{addon}` is not running, or this Add-On does not list it in \
                 dependencies or optional_dependencies"
            ));
        }
        let Some(arity) = runtime.interop.get(addon).and_then(|o| o.exports.get(function))
        else {
            return fail(format!("call_addon: `{addon}` exports no `{function}`"));
        };
        if usize::from(*arity) != args.len() {
            return fail(format!(
                "call_addon: `{addon}:{function}` takes {arity} argument(s), not {}",
                args.len()
            ));
        }
        Ok((
            i.package.clone(),
            i.snapshot.clone(),
            i.caller,
            i.world,
            i.read_only,
        ))
    })?;
    CHAIN.with(|c| {
        let chain = c.borrow();
        if chain.iter().any(|p| p == addon) {
            fail(format!(
                "call_addon: `{addon}` is already running in this chain ({} → {addon}); an \
                 Add-On's functions are never re-entered",
                chain.join(" → ")
            ))
        } else if chain.len() >= MAX_CALL_CHAIN {
            fail(format!(
                "call_addon: a chain of calls passes through at most {MAX_CALL_CHAIN} Add-Ons \
                 ({} → {addon})",
                chain.join(" → ")
            ))
        } else {
            Ok(())
        }
    })?;
    // The transaction as it was, to put back if this call fails: nothing
    // of a failed call is kept, down the whole chain.
    let before: Vec<Nested> = TRANSACTION.with(|t| t.borrow().clone());
    let prior = TRANSACTION.with(|t| {
        let mut t = t.borrow_mut();
        t.iter()
            .position(|n| n.package == addon)
            .map(|at| t.remove(at))
    });
    // The state and entity variables it starts from: what this transaction
    // already made of them, else the host's.
    // SAFETY: as `with_world`: the pointer outlives the outermost call.
    let world_ref = world.map(|w| unsafe { &*w });
    let mut entry = prior.unwrap_or_else(|| {
        let state = world_ref
            .and_then(|w| w.addon_state(addon))
            .unwrap_or_default();
        Nested {
            package: addon.to_owned(),
            input: state.clone(),
            state,
            ops: Vec::new(),
            required: BTreeSet::new(),
            entity_vars: BTreeMap::new(),
            output: Vec::new(),
            events: Vec::new(),
        }
    });
    let mut entity_vars: EntityVars = world_ref
        .map(|w| (*w.addon_vars(addon)).clone())
        .unwrap_or_default();
    for (id, v) in &entry.entity_vars {
        entity_vars.insert(*id, v.clone());
    }
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(Dynamic::from(from));
    // Views are the calling call's; the other Add-On reads plain values.
    call_args.extend(args.iter().map(view::plain));
    let api = format!("api_{function}");
    let call = Call {
        function: &api,
        args: call_args,
        // Unused: a nested call spends the outermost call's budget.
        budget: Budget::Command,
        snapshot,
        caller,
        aim: None,
        entity: None,
        state: entry.state.clone(),
        entity_vars: Arc::new(entity_vars),
        world: None,
    };
    match runtime.run(addon, call, read_only, world) {
        Ok(outcome) => {
            entry.state = outcome.state;
            let base = entry.ops.len();
            entry.ops.extend(outcome.ops);
            entry
                .required
                .extend(outcome.required.into_iter().map(|r| r + base));
            entry.entity_vars.extend(outcome.entity_vars);
            entry.output.extend(outcome.output);
            entry.events.extend(outcome.events);
            TRANSACTION.with(|t| t.borrow_mut().push(entry));
            Ok(outcome.returned)
        }
        Err(problem) => {
            TRANSACTION.with(|t| *t.borrow_mut() = before);
            fail(format!("{addon}:{function} failed: {}", problem.message))
        }
    }
}
