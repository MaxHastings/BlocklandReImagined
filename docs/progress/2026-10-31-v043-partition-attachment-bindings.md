# 2026-10-31 Partition attachment identity survives replication

Follow-up to the detached-effect-frames receipt. The existing damage-family
owner now selects one carrier before partition mutation, using exact half-open
integer cells at the authored source point. Repartition only rebinds when it
replaces that carrier. The transient `Brick.damage_attachment` pointer preserves
original settings/identity, including effects added after damage; generated parts
do not receive copies. Restoration and canonical saving clear it. Public deltas,
compressed checkpoints and world streaming retain it through packed encoding.
The protocol change has its own marker. This is a presentation relationship,
not a moving gameplay body or a new family scheduler.

Client effects resolve that carrier against either a surviving part or its
actual visible debris frame. Missing historical debris suppresses a late
observer's source. Metadata remains bounded and survives cue-before-removal
frames; original restoration or a newer original death supersedes the old
carrier. Existing emitted world particles remain independent.

The initial world unit run caught a real memory regression: adding the optional
pointer raised the inline brick to 264 bytes, beyond its 256-byte guard. Keep
the guard. Immutable optional authored names now use `Box<str>` rather than
`String`, saving its unused capacity word with the same serialized text/null
representation and no additional allocation layer. Command/script/UI APIs keep
Strings and convert explicitly. Existing adapters and test fixtures were checked
across the workspace. Early typechecks exposed those required conversions;
subsequent completed all-target workspace typecheck passes. These compile
corrections are not flaky tests. Formatting five packages at once hit Windows
command-length error 206; separate package groups succeed.

Completed focused checks so far:

- World library: 18 pass, including the unchanged inline-size guard, packed
  records and saved representation.
- Simulation destruction module: 22 pass. The new actual Session case covers
  partition/repartition, settings added after damage, no per-part duplicates,
  independent collision-shape containment, canonical save and timed restoration.
- Client FX module: 5 pass, two established ignored cases unchanged. Native
  lifecycle cases cover cue/removal delivery order, actual carrier motion,
  absent-carrier late observers, superseding original death, culling and restore.
- Network replication module: 23 pass. Checkpoint/streaming retain the pointer;
  restoration revokes it. The final negative case challenges both codec
  rejection and replica atomicity while retaining an earlier valid mutation.

The original wire size guard failed: 17,364 packed bytes versus 66,389 named
bytes exceeded the established fourfold reduction. Promoting a plain source to
a whole record merely for its new pointer was the cause. Keep that assertion;
encode the rare pointer in an ordered sparse entry-indexed column, alongside the
existing section metadata. Reject duplicate/out-of-order, invalid-entry, zero
and non-hidden bindings. World library checks again pass all 18 cases including
hostile column inputs. The strengthened replica atomicity case also passes with
a valid earlier mutation retained before invalid metadata. Both wire cases pass
against that corrected representation, including the unchanged size guard. The first new replica run then failed in its test setup:
it unwrapped decoding of deliberately invalid metadata, which the new sparse
reader now rejects earlier. Keep that stronger decoder validation and test its
refusal explicitly; separately submit the original invalid delta to the replica
to challenge atomicity behind the codec. The earlier valid mutation and unchanged
world/cursor assertions remain. All 23 replication cases pass on this corrected
fixture (`partition-attachments-replication-final.txt`).

Final checks on the completed source:

- `cargo test --locked --profile playtest -p bri-net --lib wire::tests`: 2 pass,
  including the unchanged compression guard; full replication integration: 23 pass.
- `cargo test --locked --profile playtest -p bri-sim --test duplicator --test
  advanced_duplicator`: 8/21 pass, including authored properties/name adapters.
- `cargo clippy --locked --profile playtest -p bri-world -p bri-sim -p bri-net
  -p bri-client -p bri-chaos --all-targets -- -D warnings`: passes after the
  sparse representation and final negative-fixture correction. Grouped formatting
  and `git diff --check` pass. No failing test was accepted by an unchanged retry.

Private diagnostics are `partition-attachments-*` under the release evidence
folder. These are scoped native checks, not final pixel,
interactive or full release acceptance. Bot/vehicle production while destroyed,
brick looping audio, combined acceptance and publication gates remain open.
