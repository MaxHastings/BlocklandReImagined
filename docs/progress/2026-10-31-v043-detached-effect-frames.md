# 2026-10-31 Follow cosmetic brick sources through visible debris frames

The client previously attached lights/emitters at static brick coordinates and
hardcoded `fake_dead: false`. Debris already owns the actual rendered motion.
Expose a read-only delta from each parent's birth pose, stamped with its death
cue. Whole bodies and tool-kill motion retain their source frame; a cracked
brick assigns the shard containing its centre from the actual crack half-spaces
as parent and preserves the original
offset via the inverse birth pose. Capacity ghosts carry no live source.
Section members use the same member-pose function as rendering, including
chunk assignments. Recovered members have no fabricated death-cue ID; a later
matching cue updates their sequence without restarting motion. New deaths of
the same brick supersede old still-visible debris by actual cue sequence.

`WorldEffects` captures active source metadata before replicated permanent
removal, bounded by its existing source budget. Capture precedes world sync;
resolution follows debris advancement. Source selection uses the actual moving
position and one source per brick/resource kind. Destruction without a visible
parent is suppressed, including a late observer without historical debris.
Restoration takes precedence over old debris. Source expiry/culling removes
lights immediately and drains emitted particles through the existing runtime;
already-emitted world particles are not carried or teleported. Default static
sources and unrelated map emitters retain their established owners. No gameplay
body, authority, network/save format or content asset is changed in this increment.

The first scoped Clippy pass found an unused import and a collapsible condition;
both are corrected. The fixture then used unsupported `OrdMap::iter_mut`; use
known-key mutation instead. The next Clippy pass succeeds. The integrated native
FX/debris lifecycle check passes all four combinations of temporary/permanent
removal and blast/tool-kill motion, with a fixed reference light. It verifies
published light positions, unchanged emission handle, old world-particle positions
and axes, newly rotated emission, restoration while old debris remains, replacement
death sequence, expiry/culling, late-observer suppression and map teardown.
Private logs are `debris-attachments-{first-clippy,clippy-2,clippy-3,lifecycle}.txt`.
Expanded native/effects checks and final Clippy remain pending below.

This is not completion of the destruction requirement. A separate authoritative
partition case is now explicit: `fracture::Cell::brick` copies geometry/paint,
not the original light/emitter/sound properties. The hidden original emits a
`BrickDeath::Partition` cue, while its generated parts have different IDs.
Suppressing its old-position sources is correct but does not yet bind them to
one actual partition member. Do not substitute this suppression for following.
The next correction needs an authoritative source-to-visual-parent relationship
through partition/repartition, retained for late joining and cleared on original
restoration/removal. Preserve the original authored resource identity/settings;
do not duplicate attachments onto every generated part or guess ancestry from
nearby geometry. Read `break_one` and the existing damage-family/restore owners
before choosing the wire/view representation. Section motion already has stable
member identities; it is distinct from this original-to-part relationship.
Adjacent bot/vehicle production and static brick sound still require lifecycle
accounting, as recorded in the item-source receipt. Final pixels, combined
acceptance and release/platform gates remain open.

Expanded checks on the completed source:

- `cargo test --locked --profile playtest -p bri-client --lib brick_debris::`:
  all 62 pass, including one-parent shard offsets, complete section readiness,
  rendered member/source-frame agreement through motion/fade and explicit
  section retirement. Existing collision, capacity, continuous-edit and foley
  checks remain passing; this command includes those established bounded stress
  cases but is not a full workspace/release/performance gate.
- `cargo test --locked --profile playtest -p bri-client --lib effects::tests`:
  4 pass, with the existing timing benchmark and converted-content case ignored
  by their established default harness. The converted-content case is run
  explicitly next; no ignore or assertion was added to conceal a failure.
- `cargo clippy --locked --profile playtest -p bri-client --all-targets --
  -D warnings`: passes after all Rust/test changes.

These are native pose/snapshot checks, not final-pixel or interactive acceptance.

The explicit converted-content lifecycle case also passes:
`cargo test --locked --profile playtest -p bri-client --lib
effects::tests::replicated_effect_lifecycle_late_join_and_capacity::content --
--ignored --exact` (one pass). A final carrier refinement replaces the initial
nearest-centroid selection with exact crack-half-space membership. Its retained
shard case passes with the original source offset, followed by warning-free
client all-target Clippy. An independent convex-hull containment assertion is
added to challenge that geometric contract; its completed-source check follows.

Partition follow-up owner: `simulation/damage.rs::DamageFamilies` owns original
families and restores/removes them at the common mutation boundary; the
`Destruction` scheduler explicitly owns only work. Put authoritative ancestry
there rather than creating a second family tracker. An original's resource
settings remain on its hidden brick. Generated part settings are intentionally
not copied by `fracture::Cell::brick`. A transient replicated presentation-parent
view can preserve that authored identity without altering canonical source assets
or copying effects onto every part; its bounds, restore/edit lifecycle and exact
wire representation still need implementation and validation.

The final independent convex-hull query confirms that the selected shard really
contains the original source point; the same case retains its source offset
after motion. `a_struck_brick_cracks_into_shards` passes and final client
all-target Clippy passes on that completed source (`debris-attachments-carrier-
hull{,-clippy}.txt`). No test failed only on retry. All earlier compile/fixture
corrections above are retained as distinct failures, not classified as flakiness.
