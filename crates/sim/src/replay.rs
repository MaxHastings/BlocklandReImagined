//! Match recording and replay.
//!
//! A session is deterministic: the same session, fed the same calls in the
//! same order, plays out the same way. A recording is therefore that list
//! of calls (joins, commands, movement, steps, what the host takes out of
//! the session) plus every value the session read from outside the game
//! while handling them (the wall clock, the save-load time budget, files),
//! with a digest of the match state after every tick.
//!
//! Replaying feeds the same calls to an identically built session, serves
//! the outside reads from the recording instead, and compares each call's
//! outcome and each tick's digest. The first difference is where the two
//! runs parted, so a recording of a bug becomes a reproducible case.
//!
//! What is recorded stays at the authoritative level the session's own API
//! has: no bot decisions, no gameplay events. Bots, physics and Add-On
//! scripts recreate their behaviour from those inputs. Replays are exact
//! for the same build on the same machine; floating point may differ on
//! another platform.
use crate::{
    blueprint::SavedCopy,
    bot_kind::{BotKind, tuning::Overrides},
    player::MoveInput,
    session::{
        ActionAim, CameraView, Clan, Command, LoadedCopy, Reply, Saved, SeatSince, Session,
        StoreDone,
    },
};
use anyhow::{Context, Result, ensure};
use bri_world::{Brick, BrickId, Bricks, OwnerId, TICKS_PER_SECOND};
use glam::Vec3;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, VecDeque},
    hash::Hasher,
    io::Write,
    sync::{Arc, Mutex},
};

/// The frame format this build writes and reads.
pub const SCHEMA_VERSION: u32 = 1;
/// Ticks between full checks: each part of the state digested on its own,
/// and every bot's thinking written out. One second of play.
pub const FULL_CHECK_TICKS: u64 = TICKS_PER_SECOND;
/// Largest frame a reader accepts. A frame is one call (a command carries
/// at most a saved build) or one tick's digest.
pub const MAX_FRAME_BYTES: usize = 256 << 20;

// ---------------------------------------------------------------- reads ---

/// An error as the session saw it: each context in its chain, outermost
/// first. Rebuilt, it prints exactly as the original did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorText(pub Vec<String>);
impl ErrorText {
    fn of(error: &anyhow::Error) -> Self {
        Self(error.chain().map(ToString::to_string).collect())
    }
    fn error(&self) -> anyhow::Error {
        let mut chain = self.0.iter().rev();
        let root = chain.next().map_or("Unknown error", String::as_str);
        chain.fold(anyhow::anyhow!("{root}"), |error, context| {
            error.context(context.clone())
        })
    }
}
fn kept<T>(result: anyhow::Result<T>) -> Result<T, ErrorText> {
    result.map_err(|e| ErrorText::of(&e))
}
fn restored<T>(result: Result<T, ErrorText>) -> anyhow::Result<T> {
    result.map_err(|e| e.error())
}

/// A value the session read from outside the game while handling a call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Read {
    /// Seconds since the Unix epoch (bans, administration).
    WallClock(u64),
    /// Whether a save load still had time left in this tick.
    LoadSpare(bool),
    /// The duplicator copy store's finished requests.
    CopyPoll(Vec<(u64, CopyDone)>),
    /// One Add-On's host-kept data.
    HostData(BTreeMap<String, serde_json::Value>),
    /// `/botreload`'s bot kinds read again.
    BotReload(Result<Vec<BotKind>, ErrorText>),
    /// `/botreload`'s override dials read again.
    Overrides(Result<Overrides, ErrorText>),
    /// Whether a file the session wrote (`/botsave`) was written.
    Written(Result<(), ErrorText>),
    /// Whether the host stored administration changes (bans, ranks).
    Persisted(Result<(), ErrorText>),
}
impl Read {
    fn kind(&self) -> &'static str {
        match self {
            Self::WallClock(_) => "the clock",
            Self::LoadSpare(_) => "the load budget",
            Self::CopyPoll(_) => "the copy store",
            Self::HostData(_) => "Add-On data",
            Self::BotReload(_) => "bot kinds",
            Self::Overrides(_) => "bot overrides",
            Self::Written(_) => "a file write",
            Self::Persisted(_) => "administration storage",
        }
    }
}

/// A duplicator copy store's answer, as recorded ([`StoreDone`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CopyDone {
    Saved(Result<bool, ErrorText>),
    Loaded(Result<Option<CopyLoaded>, ErrorText>),
    Listed(Result<Vec<String>, ErrorText>),
}
/// A copy the store found ([`LoadedCopy`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CopyLoaded {
    Saved(SavedCopy),
    Loose {
        bricks: Vec<Brick>,
        palette: Vec<[f32; 4]>,
    },
}
impl CopyDone {
    fn of(done: StoreDone) -> Self {
        match done {
            StoreDone::Saved(r) => Self::Saved(kept(r).map(|s| s == Saved::Written)),
            StoreDone::Loaded(r) => Self::Loaded(kept(r).map(|copy| {
                copy.map(|copy| match copy {
                    LoadedCopy::Saved(copy) => CopyLoaded::Saved(copy),
                    LoadedCopy::Loose { bricks, palette } => CopyLoaded::Loose { bricks, palette },
                })
            })),
            StoreDone::Listed(r) => Self::Listed(kept(r)),
        }
    }
    fn done(self) -> StoreDone {
        match self {
            Self::Saved(r) => StoreDone::Saved(restored(r).map(|written| {
                if written {
                    Saved::Written
                } else {
                    Saved::Exists
                }
            })),
            Self::Loaded(r) => StoreDone::Loaded(restored(r).map(|copy| {
                copy.map(|copy| match copy {
                    CopyLoaded::Saved(copy) => LoadedCopy::Saved(copy),
                    CopyLoaded::Loose { bricks, palette } => LoadedCopy::Loose { bricks, palette },
                })
            })),
            Self::Listed(r) => StoreDone::Listed(restored(r)),
        }
    }
}

/// A value the session reads from outside the game, so a recording can
/// keep it and a replay serve it.
pub trait Outside: Sized {
    fn into_read(self) -> Read;
    /// The value back, or the read unchanged when it is another kind.
    fn from_read(read: Read) -> Result<Self, Read>;
}
macro_rules! outside {
    ($(#[$doc:meta])* $name:ident($inner:ty) => $variant:ident, $to:expr, $from:expr) => {
        $(#[$doc])*
        pub struct $name(pub $inner);
        impl Outside for $name {
            fn into_read(self) -> Read {
                Read::$variant(($to)(self.0))
            }
            fn from_read(read: Read) -> Result<Self, Read> {
                match read {
                    Read::$variant(value) => Ok(Self(($from)(value))),
                    other => Err(other),
                }
            }
        }
    };
}
outside!(
    /// Seconds since the Unix epoch.
    WallClock(u64) => WallClock, |v| v, |v| v
);
impl WallClock {
    pub fn now() -> Self {
        Self(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        )
    }
}
outside!(
    /// Whether a save load has time left in this tick.
    LoadSpare(bool) => LoadSpare, |v| v, |v| v
);
outside!(
    /// The copy store's finished requests.
    CopyPoll(Vec<(u64, StoreDone)>) => CopyPoll,
    |v: Vec<(u64, StoreDone)>| v.into_iter().map(|(id, d)| (id, CopyDone::of(d))).collect(),
    |v: Vec<(u64, CopyDone)>| v.into_iter().map(|(id, d)| (id, d.done())).collect()
);
outside!(
    /// One Add-On's host-kept data.
    HostData(BTreeMap<String, serde_json::Value>) => HostData, |v| v, |v| v
);
outside!(
    /// Bot kinds read again.
    BotReload(anyhow::Result<Vec<BotKind>>) => BotReload, kept, restored
);
outside!(
    /// Bot override dials read again.
    OverridesRead(anyhow::Result<Overrides>) => Overrides, kept, restored
);
outside!(
    /// A file write's result.
    Written(anyhow::Result<()>) => Written, kept, restored
);
outside!(
    /// The host's storing of administration changes.
    Persisted(anyhow::Result<()>) => Persisted, kept, restored
);

/// Where a recorded or replayed session's outside reads go and come from.
/// Shared with the session it is installed in ([`Session::set_tape`]).
#[derive(Clone, Debug)]
pub struct Tape(Arc<Mutex<TapeState>>);
#[derive(Debug)]
struct TapeState {
    replaying: bool,
    /// Recording: reads kept during the current call. Replaying: the
    /// current call's recorded reads not served yet.
    reads: VecDeque<Read>,
    /// Replaying: the first read the recording could not serve.
    fault: Option<String>,
}
impl Tape {
    fn new(replaying: bool) -> Self {
        Self(Arc::new(Mutex::new(TapeState {
            replaying,
            reads: VecDeque::new(),
            fault: None,
        })))
    }
    fn state(&self) -> std::sync::MutexGuard<'_, TapeState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// The next value of `T`: recorded live, or served from the recording.
    /// A replay that runs out of reads, or finds another kind, reads live
    /// and notes the fault; the replay reports it as a divergence.
    fn read<T: Outside>(&self, live: impl FnOnce() -> T) -> T {
        let replaying = self.state().replaying;
        if !replaying {
            // Nothing is held across `live`: it may take its time.
            let read = live().into_read();
            self.state().reads.push_back(read.clone());
            return T::from_read(read).unwrap_or_else(|_| unreachable!("a read is its own kind"));
        }
        let next = self.state().reads.pop_front();
        let fault = match next.map(T::from_read) {
            Some(Ok(value)) => return value,
            Some(Err(other)) => format!(
                "the recording read {} where this run reads {}",
                other.kind(),
                live_kind::<T>()
            ),
            None => format!(
                "this run reads {} that the recording never did",
                live_kind::<T>()
            ),
        };
        self.state().fault.get_or_insert(fault);
        live()
    }
    fn take_reads(&self) -> Vec<Read> {
        self.state().reads.drain(..).collect()
    }
    fn serve(&self, reads: Vec<Read>) {
        let mut state = self.state();
        state.reads = reads.into();
        state.fault = None;
    }
    /// Replaying: what went wrong with the reads of the call just made.
    fn served(&self) -> Option<String> {
        let mut state = self.state();
        state.fault.take().or_else(|| {
            state
                .reads
                .front()
                .map(|read| format!("the recording read {} that this run never did", read.kind()))
        })
    }
}
/// What kind of read `T` is, for messages.
fn live_kind<T: Outside>() -> &'static str {
    // Each kind's name is on its read; build one from a value-free stand-in.
    match std::any::type_name::<T>().rsplit("::").next() {
        Some("WallClock") => "the clock",
        Some("LoadSpare") => "the load budget",
        Some("CopyPoll") => "the copy store",
        Some("HostData") => "Add-On data",
        Some("BotReload") => "bot kinds",
        Some("OverridesRead") => "bot overrides",
        Some("Written") => "a file write",
        Some("Persisted") => "administration storage",
        _ => "an outside value",
    }
}
/// [`Tape::read`] when there is a tape, else `live` directly.
pub fn outside<T: Outside>(tape: Option<&Tape>, live: impl FnOnce() -> T) -> T {
    match tape {
        Some(tape) => tape.read(live),
        None => live(),
    }
}

// ---------------------------------------------------------------- calls ---

/// One call into the session, as the host made it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Call {
    Join {
        name: String,
        spawn: [f32; 3],
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    },
    Resume {
        owner: OwnerId,
        spawn: [f32; 3],
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    },
    Disconnect {
        owner: OwnerId,
    },
    SetClan {
        owner: OwnerId,
        clan: Clan,
    },
    Command {
        owner: OwnerId,
        sequence: u64,
        command: Command,
        aim: Option<ActionAim>,
    },
    Movement {
        owner: OwnerId,
        sequence: u64,
        input: MoveInput,
    },
    SeatReport {
        owner: OwnerId,
        newest: u64,
        seat: Option<SeatSince>,
    },
    CameraReport {
        owner: OwnerId,
        view: CameraView,
    },
    PrivateChat {
        owner: OwnerId,
        text: String,
    },
    MapChangeFailed {
        admin: OwnerId,
        reason: String,
    },
    Step,
    /// The host took something the session holds for it. Taking can
    /// matter to play (the changed-brick set is read by brick events until
    /// replication takes it), so it is replayed at the same moment.
    Take(Take),
}
/// What the host takes out of the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Take {
    Dirty,
    Cues,
    Notices,
    PrivateNotices,
    MapChange,
    AdminDisconnects,
    AdminDisconnectMessage(OwnerId),
    EventOverload,
    EventDiagnostics,
    SlowEventTicks,
    PackageProblems,
    PackageScriptTime,
}
impl Take {
    fn run(self, s: &mut Session) {
        match self {
            Self::Dirty => drop(s.take_dirty()),
            Self::Cues => drop(s.take_cues()),
            Self::Notices => drop(s.take_notices()),
            Self::PrivateNotices => drop(s.take_private_notices()),
            Self::MapChange => drop(s.take_map_change()),
            Self::AdminDisconnects => drop(s.take_admin_disconnects()),
            Self::AdminDisconnectMessage(owner) => drop(s.take_admin_disconnect_message(owner)),
            Self::EventOverload => drop(s.take_event_overload()),
            Self::EventDiagnostics => drop(s.take_event_diagnostics()),
            Self::SlowEventTicks => drop(s.take_slow_event_ticks()),
            Self::PackageProblems => drop(s.take_package_problems()),
            Self::PackageScriptTime => drop(s.take_package_script_time()),
        }
    }
}

/// How a call ended, compared between the recording and the replay.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Done,
    Joined(OwnerId),
    /// A command's reply, digested.
    Replied(u64),
    Failed(ErrorText),
    /// The call panicked, with this message. Replaying it panics the same
    /// way, which is how a recorded crash is reproduced.
    Panicked(String),
}

/// What a panic was raised with.
pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".into())
}
/// `work`, with a panic caught as its message.
fn caught<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .map_err(|panic| panic_message(&*panic))
}

/// One record in a recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Frame {
    Call {
        call: Call,
        reads: Vec<Read>,
        outcome: Outcome,
    },
    /// The host swapped in a session it loaded for another map (Change
    /// Map) and moved everyone over.
    MapChanged {
        admin: OwnerId,
        map: String,
        /// The Add-On state the new map's session was set up with.
        package_save: Option<Vec<u8>>,
        reads: Vec<Read>,
        outcome: Outcome,
    },
    /// The state after a tick (and once as recording starts).
    Tick {
        tick: u64,
        digest: u64,
        /// Every [`FULL_CHECK_TICKS`], and at the start.
        full: Option<Box<FullCheck>>,
    },
}

/// What one call returned, typed for whoever made it.
enum Returned {
    Unit(Result<()>),
    Owner(Result<OwnerId>),
    Reply(Result<Reply>),
}
impl Returned {
    fn outcome(&self, secrets: &Secrets) -> Outcome {
        match self {
            Self::Unit(Ok(())) => Outcome::Done,
            Self::Owner(Ok(owner)) => Outcome::Joined(*owner),
            Self::Reply(Ok(reply)) => Outcome::Replied(reply_digest(reply, secrets)),
            Self::Unit(Err(e)) | Self::Owner(Err(e)) | Self::Reply(Err(e)) => {
                Outcome::Failed(secrets.redact_error(e))
            }
        }
    }
    fn unit(self) -> Result<()> {
        match self {
            Self::Unit(r) => r,
            _ => unreachable!("a unit call"),
        }
    }
}

/// Make `call` on `s`: the one path a live recorded call and its replay
/// both take. `persist` stores administration changes (live only).
fn apply(
    s: &mut Session,
    call: Call,
    tape: Option<&Tape>,
    persist: &mut dyn FnMut(&bri_admin::DurableState) -> Result<()>,
) -> Returned {
    let vec = |p: [f32; 3]| Vec3::from_array(p);
    match call {
        Call::Join {
            name,
            spawn,
            trusted_host,
            principal,
        } => Returned::Owner(s.join_verified(name, vec(spawn), trusted_host, principal)),
        Call::Resume {
            owner,
            spawn,
            trusted_host,
            principal,
        } => Returned::Unit(s.resume_verified(owner, vec(spawn), trusted_host, principal)),
        Call::Disconnect { owner } => Returned::Unit(s.disconnect(owner)),
        Call::SetClan { owner, clan } => Returned::Unit(s.set_clan(owner, &clan)),
        Call::Command {
            owner,
            sequence,
            command,
            aim,
        } => Returned::Reply(s.command_with_aim_and_admin_persistence(
            owner,
            sequence,
            command,
            aim,
            |state| outside(tape, || Persisted(persist(state))).0,
        )),
        Call::Movement {
            owner,
            sequence,
            input,
        } => Returned::Unit(s.movement(owner, sequence, input)),
        Call::SeatReport {
            owner,
            newest,
            seat,
        } => Returned::Unit(s.seat_report(owner, newest, seat)),
        Call::CameraReport { owner, view } => Returned::Unit(s.camera_report(owner, view)),
        Call::PrivateChat { owner, text } => {
            s.private_chat(owner, text);
            Returned::Unit(Ok(()))
        }
        Call::MapChangeFailed { admin, reason } => {
            s.map_change_failed(admin, &reason);
            Returned::Unit(Ok(()))
        }
        // The host's own step function: a test may make it fail.
        Call::Step => Returned::Unit(s.step()),
        Call::Take(take) => {
            take.run(s);
            Returned::Unit(Ok(()))
        }
    }
}

// -------------------------------------------------------------- secrets ---

/// Passwords never go into a recording. Each distinct secret becomes a
/// stand-in, the same one wherever it appears, so a replay with the
/// stand-ins as its passwords logs in and fails exactly as the match did.
#[derive(Debug, Default)]
pub struct Secrets {
    /// Real secret to its stand-in.
    stand_ins: BTreeMap<String, String>,
}
impl Secrets {
    /// The stand-in for `secret`. An empty secret (that login off) stays
    /// empty.
    pub fn redact(&mut self, secret: &bri_admin::Secret) -> bri_admin::Secret {
        let real = secret.expose();
        if real.is_empty() {
            return secret.clone();
        }
        let next = self.stand_ins.len() + 1;
        let stand_in = self
            .stand_ins
            .entry(real.to_owned())
            .or_insert_with(|| format!("replay-secret-{next}"))
            .clone();
        bri_admin::Secret::new(stand_in).expect("a short plain stand-in")
    }
    /// `call` as recorded: its passwords replaced by stand-ins.
    fn redact_call(&mut self, call: &Call) -> Call {
        let mut call = call.clone();
        if let Call::Command {
            command: Command::Admin(request),
            ..
        } = &mut call
        {
            use bri_admin::Action;
            match &mut request.action {
                Action::Login { password }
                | Action::SetAdminPassword { password }
                | Action::HostSetPassword { password, .. } => *password = self.redact(password),
                _ => {}
            }
        }
        call
    }
    /// `text` with every secret replaced by its stand-in.
    fn redact_text(&self, text: &str) -> String {
        self.stand_ins
            .iter()
            .fold(text.to_owned(), |text, (real, stand_in)| {
                text.replace(real, stand_in)
            })
    }
    fn redact_error(&self, error: &anyhow::Error) -> ErrorText {
        ErrorText(
            error
                .chain()
                .map(|c| self.redact_text(&c.to_string()))
                .collect(),
        )
    }
}
/// A reply's digest. An administration reply can echo a password the
/// player set; it is digested with its secrets as stand-ins.
fn reply_digest(reply: &Reply, secrets: &Secrets) -> u64 {
    match reply {
        // Digested the same way whether or not there are secrets to
        // replace: a replay has none, its passwords are the stand-ins.
        Reply::Admin(_) => {
            let mut value = serde_json::to_value(reply).unwrap_or_default();
            redact_value(&mut value, secrets);
            digest_of(&value)
        }
        _ => digest_of(reply),
    }
}
fn redact_value(value: &mut serde_json::Value, secrets: &Secrets) {
    match value {
        serde_json::Value::String(text) => *text = secrets.redact_text(text),
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| redact_value(v, secrets)),
        serde_json::Value::Object(map) => map.values_mut().for_each(|v| redact_value(v, secrets)),
        _ => {}
    }
}

// --------------------------------------------------------------- digest ---

/// Feeds what is written to it into a hasher.
struct HashWriter(std::hash::DefaultHasher);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// A digest of `value`'s every field. `DefaultHasher::new` uses fixed
/// keys, so it is the same in every run of one build, which is all a
/// replay compares.
pub fn digest_of<T: Serialize + ?Sized>(value: &T) -> u64 {
    let mut out = HashWriter(std::hash::DefaultHasher::new());
    if let Err(error) = rmp_serde::encode::write(&mut out, value) {
        // Never expected from plain data; folded in so it still differs.
        out.0.write(error.to_string().as_bytes());
    }
    out.0.finish()
}

/// The match state after a tick, each part on its own, and what every bot
/// was thinking.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FullCheck {
    pub parts: Parts,
    /// Each bot's readout (the F3 overlay's lines).
    pub bots: Vec<(OwnerId, Vec<String>)>,
}
/// Digests of each part of the match state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parts {
    /// Every brick, the palette and the brick owners.
    pub world: u64,
    /// Players' and bots' bodies, health, names, looks and tools.
    pub players: u64,
    /// Projectiles, held and dropped items.
    pub weapons: u64,
    pub vehicles: u64,
    pub minigames: u64,
    /// Add-On entities and state.
    pub packages: u64,
    /// Every chat line so far.
    pub chat: u64,
}
impl Parts {
    pub const NAMES: [&str; 7] = [
        "bricks",
        "players",
        "weapons",
        "vehicles",
        "minigames",
        "Add-Ons",
        "chat",
    ];
    fn values(&self) -> [u64; 7] {
        [
            self.world,
            self.players,
            self.weapons,
            self.vehicles,
            self.minigames,
            self.packages,
            self.chat,
        ]
    }
    fn digest(&self) -> u64 {
        digest_of(&self.values())
    }
    /// The names of the parts that differ from `other`'s.
    pub fn differing(&self, other: &Self) -> Vec<&'static str> {
        self.values()
            .into_iter()
            .zip(other.values())
            .zip(Self::NAMES)
            .filter(|((a, b), _)| a != b)
            .map(|(_, name)| name)
            .collect()
    }
}

/// Digests a session's state tick after tick. Bricks are kept as a running
/// sum over each brick's digest, updated from what changed since the last
/// look (the world's maps share untouched structure, so finding the
/// changes costs only the changes).
pub struct Digester {
    bricks: Bricks,
    brick_sum: u64,
    chat: u64,
    chat_seen: u64,
}
fn brick_digest(id: BrickId, brick: &Brick) -> u64 {
    digest_of(&(id, brick))
}
impl Digester {
    pub fn new(s: &Session) -> Self {
        let bricks = s.simulation().state().bricks.clone();
        let brick_sum = bricks
            .iter()
            .fold(0u64, |sum, (id, b)| sum.wrapping_add(brick_digest(*id, b)));
        let mut digester = Self {
            bricks,
            brick_sum,
            chat: 0,
            chat_seen: 0,
        };
        digester.follow_chat(s);
        digester
    }
    fn follow_bricks(&mut self, s: &Session) {
        let now = &s.simulation().state().bricks;
        for change in self.bricks.diff(now) {
            use imbl::ordmap::DiffItem;
            match change {
                DiffItem::Add(id, b) => {
                    self.brick_sum = self.brick_sum.wrapping_add(brick_digest(*id, b))
                }
                DiffItem::Remove(id, b) => {
                    self.brick_sum = self.brick_sum.wrapping_sub(brick_digest(*id, b))
                }
                DiffItem::Update { old, new } => {
                    self.brick_sum = self
                        .brick_sum
                        .wrapping_sub(brick_digest(*old.0, old.1))
                        .wrapping_add(brick_digest(*new.0, new.1));
                }
            }
        }
        self.bricks = now.clone();
    }
    fn follow_chat(&mut self, s: &Session) {
        for line in s.chat_after(self.chat_seen) {
            self.chat_seen = line.id;
            self.chat = digest_of(&(self.chat, &line));
        }
    }
    /// The state now, part by part.
    pub fn parts(&mut self, s: &Session) -> Parts {
        self.follow_bricks(s);
        self.follow_chat(s);
        let world = s.simulation().state();
        Parts {
            world: digest_of(&(
                self.brick_sum,
                world.tick,
                world.next_brick_id,
                &world.palette,
                &world.owners,
            )),
            players: digest_of(&(
                s.motion_states(),
                s.vitals(),
                s.names(),
                s.avatars(),
                s.tool_inventories(),
            )),
            weapons: digest_of(&s.weapon_view()),
            vehicles: digest_of(&(s.vehicle_poses(), s.vehicle_infos())),
            minigames: digest_of(&s.minigame_views()),
            packages: digest_of(&(s.package_entities(), s.package_state_revision())),
            chat: self.chat,
        }
    }
    /// The tick's frame: its digest, and every [`FULL_CHECK_TICKS`] (or
    /// when `full`) its parts and the bots' thinking.
    fn tick(&mut self, s: &Session, full: bool) -> Frame {
        let parts = self.parts(s);
        let tick = s.simulation().state().tick;
        let full = (full || tick.is_multiple_of(FULL_CHECK_TICKS)).then(|| {
            Box::new(FullCheck {
                parts,
                bots: s.bot_why(),
            })
        });
        Frame::Tick {
            tick,
            digest: parts.digest(),
            full,
        }
    }
}

// --------------------------------------------------------------- frames ---

/// Writes frames: each a little-endian length, then its MessagePack
/// (fields by name).
pub struct FrameWriter<W: Write> {
    out: W,
    buffer: Vec<u8>,
}
impl<W: Write> FrameWriter<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            buffer: Vec::new(),
        }
    }
    pub fn write(&mut self, frame: &Frame) -> Result<()> {
        self.buffer.clear();
        // Fields by name: the game's types read back only from maps.
        rmp_serde::encode::write_named(&mut self.buffer, frame)?;
        let len = u32::try_from(self.buffer.len()).context("Frame too large")?;
        self.out.write_all(&len.to_le_bytes())?;
        self.out.write_all(&self.buffer)?;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        Ok(self.out.flush()?)
    }
    pub fn into_inner(self) -> W {
        self.out
    }
}
/// Reads frames written by [`FrameWriter`].
pub struct FrameReader<R: std::io::Read> {
    input: R,
    buffer: Vec<u8>,
    /// Why the recording ends early: the host stopped mid-write (a crash,
    /// a killed process), or the file is damaged from there on.
    pub cut_off: Option<String>,
}
impl<R: std::io::Read> FrameReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            buffer: Vec::new(),
            cut_off: None,
        }
    }
    /// The next frame; `None` at the end (or where the recording was cut
    /// off).
    pub fn next_frame(&mut self) -> Result<Option<Frame>> {
        if self.cut_off.is_some() {
            return Ok(None);
        }
        let mut len = [0; 4];
        match read_all(&mut self.input, &mut len) {
            Ok(0) => return Ok(None),
            Ok(4) => {}
            Ok(_) => return self.cut("the last frame is cut off"),
            Err(error) => return self.cut(&format!("{error}")),
        }
        let len = u32::from_le_bytes(len) as usize;
        ensure!(len <= MAX_FRAME_BYTES, "Recording frame too large");
        self.buffer.resize(len, 0);
        match read_all(&mut self.input, &mut self.buffer) {
            Ok(read) if read == len => {}
            Ok(_) => return self.cut("the last frame is cut off"),
            Err(error) => return self.cut(&format!("{error}")),
        }
        Ok(Some(
            rmp_serde::from_slice(&self.buffer).context("Reading a recording frame")?,
        ))
    }
    fn cut(&mut self, why: &str) -> Result<Option<Frame>> {
        self.cut_off = Some(why.to_owned());
        Ok(None)
    }
}
/// Fill `buffer`, short only at the end of `input`; how much was read.
fn read_all(input: &mut impl std::io::Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match input.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}
/// A small value as MessagePack, for a recording's header.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(rmp_serde::to_vec_named(value)?)
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(rmp_serde::from_slice(bytes)?)
}

// ------------------------------------------------------------- recorder ---

/// The host's way into its session. It passes every call straight
/// through; while recording, it also writes each call, its outside reads
/// and outcome, and each tick's digest. Recording never stops the game: if
/// writing fails, the recording ends and play goes on.
pub struct Recorder {
    live: Option<Live>,
}
struct Live {
    tape: Tape,
    out: FrameWriter<Box<dyn Write + Send>>,
    digester: Digester,
    secrets: Secrets,
}
impl Recorder {
    /// Calls pass straight through; nothing is written.
    pub fn off() -> Self {
        Self { live: None }
    }
    /// Start recording `s` into `out` (after the host's own header): the
    /// state now is the first frame.
    pub fn start(s: &mut Session, secrets: Secrets, out: Box<dyn Write + Send>) -> Result<Self> {
        let tape = Tape::new(false);
        s.set_tape(Some(tape.clone()));
        let mut digester = Digester::new(s);
        let mut out = FrameWriter::new(out);
        out.write(&digester.tick(s, true))?;
        out.flush()?;
        Ok(Self {
            live: Some(Live {
                tape,
                out,
                digester,
                secrets,
            }),
        })
    }
    pub fn recording(&self) -> bool {
        self.live.is_some()
    }
    /// The recording's stand-in for a password, for the host's header.
    pub fn redact(&mut self, secret: &bri_admin::Secret) -> Option<bri_admin::Secret> {
        self.live.as_mut().map(|live| live.secrets.redact(secret))
    }
    /// Finish the file (flushing what is buffered).
    pub fn finish(&mut self) {
        self.flush();
        self.live = None;
    }
    fn write(&mut self, frame: &Frame) {
        if let Some(live) = self.live.as_mut()
            && let Err(error) = live.out.write(frame)
        {
            eprintln!("The match recording stopped; writing it failed: {error:#}");
            self.live = None;
        }
    }
    fn call(
        &mut self,
        s: &mut Session,
        call: Call,
        persist: &mut dyn FnMut(&bri_admin::DurableState) -> Result<()>,
    ) -> Returned {
        let Some(live) = self.live.as_mut() else {
            return apply(s, call, None, persist);
        };
        let recorded = live.secrets.redact_call(&call);
        let tape = live.tape.clone();
        // A panic is recorded before it goes on to the host's own guard,
        // so the recording ends with the call that crashed.
        let returned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            apply(s, call, Some(&tape), persist)
        }));
        let outcome = match &returned {
            Ok(returned) => returned.outcome(&live.secrets),
            Err(panic) => Outcome::Panicked(panic_message(&**panic)),
        };
        let frame = Frame::Call {
            call: recorded,
            reads: tape.take_reads(),
            outcome,
        };
        self.write(&frame);
        match returned {
            Ok(returned) => returned,
            Err(panic) => {
                self.flush();
                std::panic::resume_unwind(panic)
            }
        }
    }
    fn flush(&mut self) {
        if let Some(live) = self.live.as_mut()
            && let Err(error) = live.out.flush()
        {
            eprintln!("The match recording stopped; writing it failed: {error:#}");
            self.live = None;
        }
    }
    fn unit(&mut self, s: &mut Session, call: Call) -> Result<()> {
        if self.live.is_none() {
            return apply_direct(s, call);
        }
        self.call(s, call, &mut no_storage).unit()
    }
    pub fn join_verified(
        &mut self,
        s: &mut Session,
        name: String,
        spawn: Vec3,
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    ) -> Result<OwnerId> {
        if self.live.is_none() {
            return s.join_verified(name, spawn, trusted_host, principal);
        }
        let call = Call::Join {
            name,
            spawn: spawn.to_array(),
            trusted_host,
            principal,
        };
        match self.call(s, call, &mut no_storage) {
            Returned::Owner(r) => r,
            _ => unreachable!("a join"),
        }
    }
    pub fn resume_verified(
        &mut self,
        s: &mut Session,
        owner: OwnerId,
        spawn: Vec3,
        trusted_host: bool,
        principal: Option<bri_admin::Principal>,
    ) -> Result<()> {
        self.unit(
            s,
            Call::Resume {
                owner,
                spawn: spawn.to_array(),
                trusted_host,
                principal,
            },
        )
    }
    pub fn disconnect(&mut self, s: &mut Session, owner: OwnerId) -> Result<()> {
        self.unit(s, Call::Disconnect { owner })
    }
    pub fn set_clan(&mut self, s: &mut Session, owner: OwnerId, clan: &Clan) -> Result<()> {
        self.unit(
            s,
            Call::SetClan {
                owner,
                clan: clan.clone(),
            },
        )
    }
    pub fn command(
        &mut self,
        s: &mut Session,
        owner: OwnerId,
        sequence: u64,
        command: Command,
        aim: Option<ActionAim>,
        mut persist: impl FnMut(&bri_admin::DurableState) -> Result<()>,
    ) -> Result<Reply> {
        if self.live.is_none() {
            return s
                .command_with_aim_and_admin_persistence(owner, sequence, command, aim, persist);
        }
        let call = Call::Command {
            owner,
            sequence,
            command,
            aim,
        };
        match self.call(s, call, &mut persist) {
            Returned::Reply(r) => r,
            _ => unreachable!("a command"),
        }
    }
    pub fn movement(
        &mut self,
        s: &mut Session,
        owner: OwnerId,
        sequence: u64,
        input: MoveInput,
    ) -> Result<()> {
        self.unit(
            s,
            Call::Movement {
                owner,
                sequence,
                input,
            },
        )
    }
    pub fn seat_report(
        &mut self,
        s: &mut Session,
        owner: OwnerId,
        newest: u64,
        seat: Option<SeatSince>,
    ) -> Result<()> {
        self.unit(
            s,
            Call::SeatReport {
                owner,
                newest,
                seat,
            },
        )
    }
    pub fn camera_report(
        &mut self,
        s: &mut Session,
        owner: OwnerId,
        view: CameraView,
    ) -> Result<()> {
        self.unit(s, Call::CameraReport { owner, view })
    }
    pub fn private_chat(&mut self, s: &mut Session, owner: OwnerId, text: String) {
        let _ = self.unit(s, Call::PrivateChat { owner, text });
    }
    pub fn map_change_failed(&mut self, s: &mut Session, admin: OwnerId, reason: &str) {
        let _ = self.unit(
            s,
            Call::MapChangeFailed {
                admin,
                reason: reason.to_owned(),
            },
        );
    }
    /// One simulation tick, then its digest. `step` is the host's step
    /// (always [`Session::step`] outside tests that make it fail); a
    /// recording replays [`Session::step`].
    pub fn step(&mut self, s: &mut Session, step: fn(&mut Session) -> Result<()>) -> Result<()> {
        let Some(live) = self.live.as_mut() else {
            return step(s);
        };
        let tape = live.tape.clone();
        let stepped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| step(s)));
        let outcome = match &stepped {
            Ok(Ok(())) => Outcome::Done,
            Ok(Err(error)) => Outcome::Failed(live.secrets.redact_error(error)),
            Err(panic) => Outcome::Panicked(panic_message(&**panic)),
        };
        let frame = Frame::Call {
            call: Call::Step,
            reads: tape.take_reads(),
            outcome,
        };
        self.write(&frame);
        let result = match stepped {
            Ok(result) => result,
            Err(panic) => {
                self.flush();
                std::panic::resume_unwind(panic)
            }
        };
        if let Some(live) = self.live.as_mut() {
            let tick = live.digester.tick(s, false);
            let full = matches!(&tick, Frame::Tick { full: Some(_), .. });
            self.write(&tick);
            if full {
                self.flush();
            }
        }
        result
    }
    /// The host moved everyone onto `new`, a session it loaded for `map`
    /// with the Add-On state `package_save`. `new` replaces `s`.
    pub fn map_changed(
        &mut self,
        s: &mut Session,
        mut new: Session,
        admin: OwnerId,
        map: &str,
        package_save: Option<Vec<u8>>,
    ) -> Result<()> {
        let Some(live) = self.live.as_mut() else {
            let old = std::mem::replace(s, new);
            return s.adopt(old, admin);
        };
        let tape = live.tape.clone();
        new.set_tape(Some(tape.clone()));
        let old = std::mem::replace(s, new);
        let result = s.adopt(old, admin);
        let returned = Returned::Unit(result);
        let frame = Frame::MapChanged {
            admin,
            map: map.to_owned(),
            package_save,
            reads: tape.take_reads(),
            outcome: returned.outcome(&live.secrets),
        };
        self.write(&frame);
        returned.unit()
    }
    fn take<T>(&mut self, s: &mut Session, which: Take, take: impl FnOnce(&mut Session) -> T) -> T {
        let taken = take(s);
        if self.live.is_some() {
            self.write(&Frame::Call {
                call: Call::Take(which),
                reads: Vec::new(),
                outcome: Outcome::Done,
            });
        }
        taken
    }
    pub fn take_dirty(&mut self, s: &mut Session) -> std::collections::BTreeSet<BrickId> {
        self.take(s, Take::Dirty, Session::take_dirty)
    }
    pub fn take_cues(&mut self, s: &mut Session) -> Vec<crate::presentation::Cue> {
        self.take(s, Take::Cues, Session::take_cues)
    }
    pub fn take_notices(&mut self, s: &mut Session) -> Vec<String> {
        self.take(s, Take::Notices, Session::take_notices)
    }
    pub fn take_private_notices(
        &mut self,
        s: &mut Session,
    ) -> Vec<(OwnerId, crate::session::Notice)> {
        self.take(s, Take::PrivateNotices, Session::take_private_notices)
    }
    pub fn take_map_change(&mut self, s: &mut Session) -> Option<(OwnerId, String)> {
        self.take(s, Take::MapChange, Session::take_map_change)
    }
    pub fn take_admin_disconnects(&mut self, s: &mut Session) -> Vec<OwnerId> {
        self.take(s, Take::AdminDisconnects, Session::take_admin_disconnects)
    }
    pub fn take_admin_disconnect_message(&mut self, s: &mut Session, owner: OwnerId) -> String {
        self.take(s, Take::AdminDisconnectMessage(owner), |s| {
            s.take_admin_disconnect_message(owner)
        })
    }
    pub fn take_event_overload(&mut self, s: &mut Session) -> u64 {
        self.take(s, Take::EventOverload, Session::take_event_overload)
    }
    pub fn take_event_diagnostics(&mut self, s: &mut Session) -> Vec<String> {
        self.take(s, Take::EventDiagnostics, Session::take_event_diagnostics)
    }
    pub fn take_slow_event_ticks(
        &mut self,
        s: &mut Session,
    ) -> Option<crate::session::SlowEventTicks> {
        self.take(s, Take::SlowEventTicks, Session::take_slow_event_ticks)
    }
    pub fn take_package_problems(&mut self, s: &mut Session) -> Vec<bri_package::diag::Diagnostic> {
        self.take(s, Take::PackageProblems, Session::take_package_problems)
    }
    pub fn take_package_script_time(
        &mut self,
        s: &mut Session,
    ) -> BTreeMap<String, std::time::Duration> {
        self.take(
            s,
            Take::PackageScriptTime,
            Session::take_package_script_time,
        )
    }
}
/// A call made with no recording: straight to the session.
fn apply_direct(s: &mut Session, call: Call) -> Result<()> {
    apply(s, call, None, &mut no_storage).unit()
}
fn no_storage(_: &bri_admin::DurableState) -> Result<()> {
    anyhow::bail!("Persistent administration storage is not configured")
}

// ------------------------------------------------------------- replayer ---

/// Where a replay first parted from its recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Divergence {
    /// The tick the match was on.
    pub tick: u64,
    /// What differed, in plain words.
    pub what: String,
    /// The first full check at or after it.
    pub later: Option<LaterCheck>,
}
/// What a full check after a divergence found different.
#[derive(Clone, Debug, PartialEq)]
pub struct LaterCheck {
    pub tick: u64,
    /// The parts of the state that differed ([`Parts::NAMES`]).
    pub parts: Vec<&'static str>,
    /// Each bot whose thinking differed.
    pub bots: Vec<BotDifference>,
}
/// One bot's readout, as recorded and as replayed (empty when it was not
/// there).
#[derive(Clone, Debug, PartialEq)]
pub struct BotDifference {
    pub bot: OwnerId,
    pub recorded: Vec<String>,
    pub replayed: Vec<String>,
}
/// How a replay went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Calls replayed (steps included).
    pub calls: u64,
    /// Ticks checked against the recording.
    pub ticks: u64,
    /// The last tick reached.
    pub last_tick: u64,
    pub divergence: Option<Divergence>,
    /// Why the recording ends early, if it does ([`FrameReader::cut_off`]).
    pub cut_off: Option<String>,
}

/// Plays a recording's frames into a session built as the recorded one
/// was, and compares as it goes.
pub struct Replayer {
    tape: Tape,
    digester: Digester,
    /// A replay's passwords are already the stand-ins; nothing to redact.
    secrets: Secrets,
    report: Report,
    /// Seen the divergence's later full check (or there is no divergence
    /// to explain): nothing more to learn.
    explained: bool,
}
/// Loads the session for a map a recording changed to, with the Add-On
/// state it was set up with, as the recording's host would have (its
/// events prepared).
pub type LoadMap<'a> = dyn FnMut(&str, Option<&[u8]>) -> Result<Session> + 'a;

impl Replayer {
    /// Replay into `s`, which must be set up as the recorded session was
    /// when recording started.
    pub fn start(s: &mut Session) -> Self {
        let tape = Tape::new(true);
        s.set_tape(Some(tape.clone()));
        Self {
            tape,
            digester: Digester::new(s),
            secrets: Secrets::default(),
            report: Report::default(),
            explained: false,
        }
    }
    /// Whether the replay has parted from the recording and found what
    /// differed after: replaying further adds nothing.
    pub fn finished(&self) -> bool {
        self.report.divergence.is_some() && self.explained
    }
    fn diverge(&mut self, tick: u64, what: String) {
        if self.report.divergence.is_none() {
            self.report.divergence = Some(Divergence {
                tick,
                what,
                later: None,
            });
        }
    }
    fn compare_call(&mut self, tick: u64, label: String, recorded: &Outcome, replayed: &Outcome) {
        if let Some(fault) = self.tape.served() {
            self.diverge(tick, format!("{label}: {fault}."));
        }
        if recorded != replayed {
            self.diverge(
                tick,
                format!(
                    "{label}: in the recording it {}, in this run it {}.",
                    describe(recorded),
                    describe(replayed)
                ),
            );
        }
    }
    /// Play one frame.
    pub fn frame(
        &mut self,
        s: &mut Session,
        frame: Frame,
        load_map: &mut LoadMap<'_>,
    ) -> Result<()> {
        let tick = s.simulation().state().tick;
        match frame {
            Frame::Call {
                call,
                reads,
                outcome,
            } => {
                self.tape.serve(reads);
                let label = label(&call);
                let tape = self.tape.clone();
                // Administration storage is answered from the recording.
                let replayed = match caught(|| apply(s, call, Some(&tape), &mut |_| Ok(()))) {
                    Ok(returned) => returned.outcome(&self.secrets),
                    Err(message) => Outcome::Panicked(message),
                };
                self.report.calls += 1;
                self.compare_call(tick, label, &outcome, &replayed);
            }
            Frame::MapChanged {
                admin,
                map,
                package_save,
                reads,
                outcome,
            } => {
                self.tape.serve(reads);
                let mut new = load_map(&map, package_save.as_deref())
                    .with_context(|| format!("Loading {map} as the recording did"))?;
                new.set_tape(Some(self.tape.clone()));
                let old = std::mem::replace(s, new);
                let replayed = match caught(|| s.adopt(old, admin)) {
                    Ok(adopted) => Returned::Unit(adopted).outcome(&self.secrets),
                    Err(message) => Outcome::Panicked(message),
                };
                self.report.calls += 1;
                self.compare_call(
                    tick,
                    format!("Changing the map to {map}"),
                    &outcome,
                    &replayed,
                );
            }
            Frame::Tick {
                tick: recorded,
                digest,
                full,
            } => {
                self.report.ticks += 1;
                self.report.last_tick = tick;
                if recorded != tick {
                    self.diverge(
                        tick,
                        format!(
                            "The recording is at tick {recorded} where this run is at tick {tick}."
                        ),
                    );
                }
                let parts = self.digester.parts(s);
                if parts.digest() != digest {
                    self.diverge(tick, "The match state differs.".into());
                }
                if let Some(full) = full {
                    let bots = bot_differences(&full.bots, &s.bot_why());
                    if !bots.is_empty() {
                        self.diverge(tick, "The bots think differently.".into());
                    }
                    if let Some(divergence) = self.report.divergence.as_mut()
                        && divergence.later.is_none()
                    {
                        divergence.later = Some(LaterCheck {
                            tick,
                            parts: full.parts.differing(&parts),
                            bots,
                        });
                        self.explained = true;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn report(self) -> Report {
        self.report
    }
}

/// Replay every frame `frames` holds into `s` (set up as the recorded
/// session was), stopping once it has parted from the recording and seen
/// the next full check.
pub fn replay<R: std::io::Read>(
    s: &mut Session,
    frames: &mut FrameReader<R>,
    load_map: &mut LoadMap<'_>,
) -> Result<Report> {
    let mut replayer = Replayer::start(s);
    while !replayer.finished()
        && let Some(frame) = frames.next_frame()?
    {
        replayer.frame(s, frame, load_map)?;
    }
    s.set_tape(None);
    let mut report = replayer.report();
    report.cut_off = frames.cut_off.clone();
    Ok(report)
}

/// The bots whose readouts differ between `recorded` and `replayed`.
fn bot_differences(
    recorded: &[(OwnerId, Vec<String>)],
    replayed: &[(OwnerId, Vec<String>)],
) -> Vec<BotDifference> {
    let recorded: BTreeMap<_, _> = recorded.iter().map(|(b, w)| (*b, w)).collect();
    let replayed: BTreeMap<_, _> = replayed.iter().map(|(b, w)| (*b, w)).collect();
    let bots: std::collections::BTreeSet<_> = recorded.keys().chain(replayed.keys()).collect();
    bots.into_iter()
        .filter(|bot| recorded.get(bot) != replayed.get(bot))
        .map(|bot| BotDifference {
            bot: *bot,
            recorded: recorded.get(bot).map(|w| w.to_vec()).unwrap_or_default(),
            replayed: replayed.get(bot).map(|w| w.to_vec()).unwrap_or_default(),
        })
        .collect()
}

/// A call, in words.
fn label(call: &Call) -> String {
    match call {
        Call::Join { name, .. } => format!("{name:?} joining"),
        Call::Resume { owner, .. } => format!("Player {owner} rejoining"),
        Call::Disconnect { owner } => format!("Player {owner} leaving"),
        Call::SetClan { owner, .. } => format!("Player {owner}'s clan tags"),
        Call::Command { owner, command, .. } => {
            let kind = serde_json::to_value(command)
                .ok()
                .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_owned))
                .unwrap_or_else(|| "unnamed".into());
            format!("Player {owner}'s {kind} command")
        }
        Call::Movement {
            owner, sequence, ..
        } => format!("Player {owner}'s move {sequence}"),
        Call::SeatReport { owner, .. } => format!("Player {owner}'s seat report"),
        Call::CameraReport { owner, .. } => format!("Player {owner}'s camera report"),
        Call::PrivateChat { owner, .. } => format!("A message to player {owner}"),
        Call::MapChangeFailed { admin, .. } => format!("Player {admin}'s failed map change"),
        Call::Step => "The tick".into(),
        Call::Take(take) => format!("The host taking {take:?}"),
    }
}
/// An outcome, in words.
fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Done => "succeeded".into(),
        Outcome::Joined(owner) => format!("joined as player {owner}"),
        Outcome::Replied(digest) => format!("replied (reply {digest:016x})"),
        Outcome::Failed(error) => format!("failed ({})", error.0.join(": ")),
        Outcome::Panicked(message) => format!("crashed ({message})"),
    }
}
