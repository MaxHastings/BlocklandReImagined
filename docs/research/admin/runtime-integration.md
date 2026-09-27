# Administration runtime integration evidence

This records the current Session and QUIC boundary for the typed `bri-admin`
authority. Protocol version 10 carries authoritative `AdminSnapshot` values
after admission and whenever the roster, roles, or password configuration
changes. Clients submit Admin requests through the existing reliable command
sequence and action-rate checks. Requests never supply their actor, owner,
local status, role, or native identity.

Identity-aware connections prove possession of a persistent local Ed25519 key
over a fresh server challenge. The proof binds the pinned TLS certificate,
protocol version, player name, content identity, and optional resume/host
capabilities. Invalid signatures and replayed challenges reject before Session
admission. The server derives its principal from the verified public key; names
remain display text. Persistent host mode rejects clients without a proof.
Legacy ephemeral `server::start` remains available for tests and probes.

The server validates the host credential before assigning host authority. A
resume token is an opaque server-issued capability stored with its original
host bit and verified principal, so reconnect requires the same proved key and
restores host authority only from server-side ticket state or a separately
verified host credential. Ordinary clients cannot set their role through a
request. Password-granted roles are bound to the live transport connection and
return to Player on reconnect. Session setup can install Admin and SuperAdmin
passwords before any clients have joined; the setup operation validates both
before mutation and rejects calls after a join or disconnect. Passwords remain
in memory. Join passwords are rejected by the Admin API because they are not
wired to the transport handshake.

The operational request set is login, kick, ban/list/unban, host role
assignment, Admin and SuperAdmin password updates, brick-group listing,
clearing all bricks, and clearing one brick group. Role updates also update the
Session actor used by existing build and brick permissions, and roster
snapshots are broadcast to all clients. Kicks, bans, and the fourth failed
password attempt close affected authenticated peers and remove their Session
owners. Ban/unban state is committed atomically before memory state,
disconnects, or a successful reply become visible. Clear requests preflight
world revision capacity before removing any brick. Group IDs are authoritative
owner IDs; ID 0 is represented as the unowned group.

Ban principals are self-generated local key pseudonyms, not externally
verified accounts or permanent machine identity. Deleting the local identity
file creates a new principal, so bans do not promise resistance to deliberate
identity reset. The system does not derive identities from a name, BL_ID, IP,
or client-supplied key without a valid signature. Bans target connected,
verified principals; offline identity lookup and legacy IP/BL_ID display are
not provided. Host join-password changes and other source menu features
without a host adapter return errors instead of reporting success.

`server::start_with_admin_store_and_limit` takes an absolute caller-owned state
path, validates or initializes durable state before opening the listener, and
requires identity proofs. Corrupt state and initial write errors fail startup.
Pre-replacement write failures do not publish a candidate ban or unban. If
atomic replacement succeeds but directory durability is uncertain, the server
stops rather than continue with divergent in-memory authority.

Focused validation on 2026-09-26:

- `cargo test -p bri-admin`: 10 passed.
- `cargo test -p bri-sim --tests`: 48 passed, 3 ignored.
- `cargo test -p bri-net --test loopback`: final result recorded after the
  identity-specific additions below.
- `cargo test -p bri-sim --test session setup_admin_passwords`: setup/login
  integration passed.
- `cargo test -p bri-sim --test session
  ban_and_unban_publish_only_after_durable_commit`: injected storage failure
  left durable memory state and disconnect effects unpublished.
- `cargo test -p bri-net --lib admin_store::tests`: corrupt state, precommit
  failure and post-replacement durability uncertainty passed.
- `cargo test -p bri-net --test loopback
  identity_proofs_reject_forgery_and_replayed_challenges`: QUIC proof rejection
  passed.
- `cargo test -p bri-net --test loopback
  persistent_bans_bind_keys_survive_restart_and_unban`: key-bound resume,
  copied-name isolation, stolen-ticket rejection, host protection, restart ban,
  expired ban, ban-list and unban passed.
- `cargo test -p bri-identity --lib`: identity file protection and malformed
  or symlink path checks passed.
- `cargo check -p bri-net`: passed.

Root owns App/ban-row mapping and end-to-end startup wiring. Join-password
transport configuration and remaining host menu adapters remain integration
limitations. Full targeted Clippy is pending.
