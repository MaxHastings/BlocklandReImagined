# Red team: Add-On client code, hosting and joining

2026-09-28, from main 47dcf2a, branch `claude/red-team-o8nvo2`.

Scope: the Add-On client sandbox (PR #11: `crates/client-sandbox`,
`crates/client/src/client_code.rs`), hosting and joining (PR #13:
`crates/net`, the join code in `crates/client`), and a read of PR #1's
package download (`crates/net/src/packages.rs`, `crates/package/src/sync.rs`
on `claude/stress-campaign-73g1eu`). Every attack is a test:
`crates/client-sandbox/tests/red_team.rs`, `crates/net/tests/red_team.rs`,
one test in `crates/net/tests/loopback.rs`, and unit tests in
`crates/client/src/servers.rs` and `client_code.rs`.

## Fixed on this branch (one commit each, with its test)

Ranked by harm.

1. **Idle or spoofed connections could lock every player out of a host.**
   A connection held one of the server's 80 slots from its first packet
   until it joined or its 10 s timeouts ran out. One address opening 80
   connections and idling refused every real player; spoofed QUIC Initial
   packets (about eight a second) could do the same without ever finishing
   a handshake. Now: a QUIC retry proves the address first, and each
   address may hold four connections that have not joined. Joined players
   do not count. (`52aeaa4`, `server.rs`)
2. **Endless recursion in an Add-On could crash the whole game.** Wasmtime
   runs Add-On code on the caller's stack; the game calls it from its main
   thread (1 MiB on Windows). With a 512 KiB wasm stack, recursion started
   480 KiB deep (release) or 256 KiB deep (debug) aborted the process with
   a native stack overflow instead of trapping. Now 256 KiB. (`efbd055`)
3. **A trust grant could cover capabilities the prompt never showed.** The
   prompt is built from what the server says about its code before it
   downloads, and a grant was checked against hash and tier only. A server
   naming the real hash but listing fewer capabilities got the code run
   with the unlisted ones. A grant now needs every declared capability to
   be one it showed. (`dbbb272`, `trust.rs`)
4. **Add-On trust was keyed by the typed address, not the host.** After an
   "identity changed" failure the saved pin is forgotten and the next join
   trusts whoever answers, so a new host at the same address inherited
   every code grant. Code now runs under `host-key:<hex>` of the
   certificate the host presented. (`ffd70db`)
5. **A LAN listing overrode the saved host pin.** LAN listings are unsigned
   broadcast replies anyone on the network can send for any address. A join
   preferred the listing's certificate to the saved pin (a working
   man-in-the-middle with ARP spoofing), and a mismatch deleted the real
   pin. Order is now invite key, saved pin, LAN listing, first use; only a
   join that used the saved pin may forget it. Starring a LAN row follows
   the same order, so a spoofed row cannot pin its sender via the star.
   (`c347f64`, `1f98f11`)
6. **A hostile host could hang a join forever.** After the handshake the
   client opened its stream and sent JoinBegin and Hello with no deadline;
   a host allowing zero streams left the player on "Connecting" (a test
   still hung after 20 s). All steps now share the caller's wait.
   (`a0302d7`)
7. **Shader cost undercounted large values about 100 times.** Cost counted
   each expression once whatever its size; copying a 16 KiB local array is
   one expression but a thousand vec4 moves, and the loop cap (cost x
   iterations) was set for the smaller number. Each expression now costs
   one per 16 bytes of its value. Scalar and vector code, and the scalar
   calibration shader, cost what they did. (`6af925c`)

Also `d59c017`: `bri-client-sandbox` sat under the Windows-only dependency
table in `crates/client/Cargo.toml`, so `bri-client` did not build on
Linux (cloud threads). No Windows effect.

## Checked and holding (tests added where none existed)

- Frames that never return: stopped by fuel, and by the wall clock when
  fuel is huge (existing tests). A loop of 64 MiB `memory.fill` /
  `memory.copy` (one unit of fuel each) is stopped by the clock within a
  frame (new).
- Memory past 64 MiB, and endless `table.grow`: stopped (new for tables).
- WASI, `env` or any non-`bri` import, imported memories, shared memory,
  threads, garbage and Windows executables: refused at load (existing).
- Code swapped on disk after the prompt: the hashed bytes are what runs;
  loading again sees new code the old grant does not cover (new).
- Paths outside the Add-On and links: refused (existing).
- Shaders: compute, storage, textures, overrides, huge types, fan-out calls
  and endless loops are refused or bounded (existing); pipeline errors are
  caught by error scopes, so a shader naga accepts and wgpu refuses stops
  the Add-On instead of panicking.
- Invites: every prefix of a real invite, characters swapped for `/ : [ é`
  NUL and space, 100 KB hosts and 10 KB keys all refuse without a panic,
  and whatever parses round-trips (new).
- Hosts that go silent after accepting a stream, answer garbage, or a port
  that echoes garbage datagrams: refused within the wait (new).
- Wire decode: frames are length-bounded before allocation, zstd output is
  capped at 128 MiB, trailing bytes are refused, datagrams are rate-limited
  per peer. QUIC's packet protection makes replayed or truncated packets a
  transport matter; duplicated movement inputs are ignored by sequence.
- PR #1 downloads: listings are validated (paths, case folding, sizes,
  package hash) before fetching; every object is hashed while written and
  refused on a wrong size or hash; installs are re-hashed before publishing.
  A download that lies about size or hash is refused.

## Not fixed: ranked, with a proposed fix

1. **One joined player can stall everyone's commands for 10 s at a time.**
   `read_budgeted_request` takes the length header's worth (up to 64 MiB)
   from the shared 128 MiB request budget before reading the body, and the
   body read has 10 s. Two players sending a 64 MiB header and then
   trickling bytes hold the whole budget; every other player's building
   and chat waits, and they can repeat. Fix: a per-peer share of the budget
   (for example `REQUEST_BODY_BUDGET / 4`), or a body deadline that scales
   with length and a minimum rate. Needs a decision on the largest
   legitimate request (converted stock builds).
2. **"Identity changed" forgets the pin with no question.** The first
   failed join deletes the saved pin, and the next join silently trusts
   whatever answers. That is one retry for a man-in-the-middle. SSH's
   answer is a screen: "This server's identity changed. Only continue if
   its host told you they reinstalled", with Continue / Cancel, and the
   pin replaced only on Continue. Small UI change; Max's call on wording.
3. **PR #1: 4 GB of downloads per join with no question.** `MAX_FETCH_BYTES`
   is 4 GiB and the cache 8 GiB, so a hostile server can make every join
   download 4 GiB and evict other servers' packages. Suggest asking the
   player above something like 200 MB ("This server's Add-Ons need 1.3 GB.
   Download and join?").
4. **PR #1: the download connection has the same untimed `open_bi`** as
   the join had (`fetch_missing_pinned`). When PR #1 lands on top of this
   branch, its `connect_quic` split needs the same shared wait; expect a
   small conflict in `client.rs::open`.
5. **A host's advertised name is unverified.** The LAN listing and the
   challenge carry any name the host picks (checked for length and control
   characters only), so a host can call itself "Max's Server". With pins
   and host keys this cannot redirect trust, but the join list and the
   trust prompt title show it. Showing the host key's first characters
   next to the name on the trust prompt would make impersonation visible.
6. **Slow-drip worlds.** A host can send one brick every 29 s and keep a
   join "Receiving world" for as long as `MAX_BRICKS` allows. The player
   can cancel; a whole-transfer deadline (or a minimum rate) would end it
   on its own.
7. **UPnP and NAT-PMP trust the local network.** Any device answering the
   SSDP search can claim to be the router and report any outside address,
   which then appears in the host's invite. Local-network attacker only;
   noted, no change proposed.
