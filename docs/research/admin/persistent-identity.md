# Persistent client identity and bans

## Identity model

`bri-identity::ClientIdentity` creates or loads an Ed25519 key pair from an
absolute path selected by the caller. The key is a local pseudonym. The server
derives the administration principal as SHA-256 of the verified public key.
There is no public account service, machine fingerprint, IP-derived identity,
or native BL_ID mapping. A user who deletes or replaces the key gets a new
principal and can avoid a ban; this is not an account-strength permanent ban.
Names are display-only.

The private PKCS#8 key never appears in protocol messages or Debug output. On
Windows it is encrypted with current-user DPAPI before disk storage; the
application chooses a per-user state directory. On Unix-like systems the file
is created mode 0600 and existing files with group/other permissions reject.
Creation uses a same-directory temporary file and no-clobber hard link so two
first-run processes do not replace each other's identity. Existing symlinks,
non-files, oversized/corrupt files, and unsupported file formats fail closed.
Moving the protected Windows file to a different OS user does not preserve the
identity. Unix storage relies on the caller choosing that user's private app
data directory and the OS enforcing owner-only mode.

The `bri-server` production binary stores `administration.json` under its
explicit state directory; it does not need a client signing key. App clients
store `client.identity` under the caller-supplied per-user state directory;
headless tests use fresh temporary directories. `Client::connect_with_identity`
accepts a loaded identity.
`Client::connect` remains an anonymous compatibility path for the ephemeral
test/probe server, while `start_with_admin_store_and_limit` refuses missing
proofs. Do not use the compatibility path for an identity-required server.

## Wire proof and reconnect binding

Protocol v10 starts the QUIC bidirectional stream with a small versioned
`JoinBegin` frame. The server then creates a fresh 256-bit random nonce and
sends a `Challenge`. The client signs a canonical transcript containing a
domain/version label, protocol version, nonce, SHA-256 fingerprint of the
already pinned server certificate, length-prefixed UTF-8 player/content IDs,
and presence-tagged resume and host tokens. The server validates request
bounds before signature verification. The signature is checked with ring's
Ed25519 verifier; the public key is accepted only after proof succeeds.

The connection handler consumes one challenge and one Hello for that stream.
A replay on another connection fails because it has a different nonce. The
resume token's server-side record binds owner, host capability, and the
verified principal. Resuming requires the same principal; possessing another
player's ticket or copying their name is insufficient. The transcript also
binds a proof to the pinned server and exact join/resume/host context.
Challenge wait, Hello size, connection task count, and signature length are
bounded. Persistent servers require proofs and do not downgrade when proof is
omitted. Legacy ephemeral servers allow proofless clients but still verify any
proof they receive.

The public key is a stable pseudonym a host can correlate across servers.
Per-server identity derivation is not used because host certificate identity is
currently generated per server run; making a stable per-host pseudonym requires
a separate persistent host identity design. There is no identity reset button
in the UI yet; manual removal of `client.identity` resets it.

## Durable ban transaction

The host supplies the absolute `administration.json` path to
`server::start_with_admin_store_and_limit`. Startup creates the containing
directory, reads at most `bri_admin::MAX_SAVE_BYTES`, rejects symlinks and
non-regular targets, parses and validates the complete `DurableState`, and
initializes a missing file before the socket opens. A corrupt store or initial
write failure prevents the host from starting.

For a ban or unban, the Session clones the authority state and applies the
request to the candidate. The candidate is schema/budget validated and written
to a same-directory `tempfile` staging file. The file is flushed before atomic
replacement. Only after persistence succeeds does the authority publish the
candidate and allow disconnect effects, success replies, and snapshots. A
pre-replacement I/O error discards the candidate: no ban/unban memory change,
disconnect, or successful acknowledgement occurs. If replacement succeeds but
the Unix parent-directory sync fails, the commit is uncertain; the store is
poisoned and the host loop terminates rather than admit peers using stale
memory. Other unsupported host menu actions still reject.

On admission, a verified principal is checked against active UTC-expiring bans
before its Session player is created. Banning a connected target persists the
ban first, then disconnects every live connection for that principal. Owner,
local, and SuperAdmin protections still run before persistence. Ban IDs are
stable and unban targets the ID, not a display name. Expired records do not
block joins or appear in the active list; they are safely compacted on a later
ban. There is no offline lookup by display name or claimed number, and the UI
must label the identity as a local-key pseudonym.

## Evidence

The integration tests exercise good and bad proofs over real QUIC, challenge
replay, same-key reconnect, foreign-key resume-ticket theft, same-name/different
key behavior, persistent ban across a host restart, expired bans, unban across
restart, protected host targets, proofless admission rejection, corrupt-state
startup, precommit write failure, and uncertain post-replacement shutdown.
Session tests inject a failing storage callback and verify no in-memory ban or
disconnect intent becomes visible; a successful callback publishes the ban,
reply and disconnect intent together. No public service or original content is
used.
