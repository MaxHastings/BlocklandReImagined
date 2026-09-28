# Hosting and joining

Players expect to click Host, send a friend something, and have the friend
join, with no router settings. This document covers what the game does today
without any service of ours, the one design decision still open (join codes
through a small hosted service), and the seams that keep that decision cheap
either way.

Scope: direct connections only. A public server list or master server is out
of scope (Max, 2026-09-28).

## What happens today

### One port, one identity

A host listens on one UDP port (28000 by default) for QUIC. Joining needs only
that port. LAN discovery still answers broadcasts on UDP 28050, but that port
is never forwarded and never needed to join.

Every host has a certificate (`host-identity.bin` in its state folder, kept
across restarts). The client decides which certificate it accepts with a
`HostPin` (`crates/net/src/client.rs`), checked in one TLS verifier; the
handshake signature is always verified against the presented certificate:

| Pin | Where it comes from | Accepts |
| --- | --- | --- |
| `Key` | an invite | a certificate whose SHA-256 starts with the invite's 16-byte key |
| `Certificate` | a saved pin (`trusted-hosts.json`) or a LAN listing | exactly that certificate |
| `FirstUse` | a bare address typed for the first time | any certificate, which is then pinned (like SSH) |

A pin that no longer matches fails as `JoinError::IdentityChanged`; the stale
pin is forgotten so joining again trusts the new host. A join through an invite
replaces any older pin.

### Addresses and invites

`bri_net::invite::JoinTarget` parses what a player types or pastes:
`203.0.113.10`, `203.0.113.10:28001`, `[2001:db8::1]:28000`,
`play.example.com[:port]` (resolved by DNS), or an invite:

```
bri://203.0.113.10:28000/a3kq7m2x5c4vbn6ydh2fz7w4pe
```

The last part is the host key: the first 128 bits of the SHA-256 of the host's
certificate, lower-case RFC 4648 base32 without padding (26 characters, no
characters that chat or URLs mangle, case-insensitive). 128 bits puts a
second-preimage attack far out of reach. Pins and saved servers are keyed by
the address as typed (`JoinTarget::address`), so a host name keeps its pin when
its IP changes.

### Probe over the game port (protocol 35)

The first answer to a `JoinBegin` is `Message::Challenge { nonce, listing }`.
`Listing` carries the server name, map and player counts. A probe
(`bri_net::client::probe`) opens QUIC, sends `JoinBegin`, reads the challenge
and closes: the join list uses it to show saved servers live, and the host uses
it to test its own public address. A client on another protocol version gets
a refusal saying which side must update.

### Host check

`ServerHandle::open_to_internet` (called for Internet hosts and dedicated
servers not bound to loopback) runs `bri_net::reach::open_and_check`:

1. Ask the router to forward the game port: UPnP IGD first (`upnp.rs`), then
   NAT-PMP (`natpmp.rs`, RFC 6886) to the default gateway. Forwards are leased
   for an hour, renewed every 20 minutes, and removed when hosting stops.
2. Ask a public STUN server (`stun.rs`, RFC 8489 Binding request) for the
   public address. Servers used: `stun.cloudflare.com:3478`, then
   `stun.l.google.com:19302`. They see the host's IP and nothing else.
3. Probe `public address:port` with the host's own key. A router that
   forwards the port and loops traffic back proves the path end to end.

The host player then reads one verdict in chat:

| Verdict | Evidence | What the player is told |
| --- | --- | --- |
| Reachable | the self-probe answered | friends can join; invite on the clipboard |
| Likely | the router opened the port, addresses agree, no loopback | friends should be able to join; invite on the clipboard |
| Shared address | the router's outside address is private or differs from the STUN address | a provider or second router shares the address; ask for a public IP or use a virtual LAN tool |
| Needs forward | no UPnP or NAT-PMP | turn on UPnP, or forward UDP *port* to *this PC's LAN address*; the invite for afterwards |
| Unknown | no public address found | nothing could be checked; LAN still works |

`/invite` copies the host's invite again. LAN hosts get an invite with their
LAN address. The dedicated server prints the same lines.

### Windows Firewall

The first time a program listens, Windows asks whether to allow it. Cancel, or
allowing only private networks while on a public one, leaves block rules that
stop every friend. LAN and Internet hosts read the firewall rules for the game
executable through PowerShell's NetSecurity module (no administrator rights,
enum names are not localized) and decide per active network profile
(`firewall::decide`). When friends would be blocked, a yes/no box offers the
fix: the game starts itself elevated (`bri-client --allow-firewall`, one
Windows permission prompt), deletes this program's inbound rules and adds one
allow rule named "Blockland ReImagined" for UDP on every profile. Declining
changes nothing.

### Join list

Opening Join Server searches the LAN and probes every saved server at once
(two-second timeout each). Saved servers live in `servers.json` in the client
state folder: favourites (up to 64, kept) and the last ten joins. A saved
server remembers its invite, so joining from the list is verified even
without a pin. The v20 "Query Internet" button, which had no master server to
query, is now Favorite/Unfavorite for the selected server. Favourites are
listed first and marked `*`. A saved server that does not answer shows why in
the Map column: no answer, host changed, other version, unknown name.

## The seam for join codes

Everything that joins goes through `JoinTarget::parse` then
`JoinTarget::resolve() -> Route { address, key }`, and connects with the
route's key as a `HostPin::Key`. A join code is another `JoinTarget` variant
whose `resolve` asks a rendezvous service; nothing after `resolve` changes. A
relayed route would add a transport choice to `Route`; the QUIC session, pins
and protocol stay as they are, because a relay only forwards encrypted packets.

## Open decision: join codes and a relay

Direct addresses work for hosts whose router opens the port, which is most home
connections but not all. Players today expect a short code instead
(Among Us, Valheim, Minecraft Bedrock, Steam games): no address to read out,
no router settings, and it works behind carrier-grade NAT. That needs a small
always-on service. Nothing here is built; this is the option for Max to decide.

### How it would work

- **Rendezvous service.** The host registers with the service over an
  outgoing connection it keeps open, and gets a code like `BRK-7Q4M`
  (Crockford base32, no ambiguous letters). It publishes its candidate
  addresses (LAN address, STUN public address, router-forwarded address) and
  its host key. The service keeps this in memory only while the host is
  connected.
- **Join.** The joiner resolves the code to the candidates and key. Both sides
  learn each other's public address from the service, and the service tells
  the host to "punch": the host's QUIC endpoint starts an outgoing connection
  attempt to the joiner's public address from its game port (quinn endpoints
  can connect and accept on one socket), which opens the host's NAT for the
  joiner's packets. The joiner's normal connect then gets through. This works
  for most home NATs (about nine in ten pairs, by industry figures for UDP hole
  punching).
- **Relay fallback.** When punching fails (symmetric NAT on both sides, strict
  corporate networks), both sides send QUIC packets to a relay that forwards
  them unchanged. The session stays end-to-end encrypted and pinned by the
  host key, so the relay cannot read or alter the game. Bandwidth is what the
  game sends: snapshots at 20 Hz and movement datagrams (roughly tens of
  kilobits per second per player) plus world transfers on join.
- **Invites keep working.** Direct addresses and `bri://` invites stay for
  players who forward ports, LAN play, and dedicated servers.

### What it costs

- A service to run: one small VPS runs both rendezvous and relay for many
  concurrent games; relay traffic is the only cost that grows with play.
- Operations: uptime, abuse limits (codes per IP, relay bandwidth caps per
  session), a privacy note (the service sees who connects to whom, never
  content), and a way to point the game at another service
  (a `$pref::Net::Rendezvous` address) so it is open source and self-hostable.
- Code: the rendezvous and relay (one small Rust binary, shared protocol
  types in `bri-net`), a UDP socket wrapper for quinn that lets the host send
  punch packets and relay-wrapped packets, and the code field in Connect to IP.

### Alternatives

- **Steam networking (Steam Datagram Relay)** gives codes, invites and relays
  for free, but needs a Steam release and the Steamworks SDK.
- **Epic Online Services P2P** is free and store-independent but adds a large
  SDK and an Epic account dependency.
- **IPv6 direct.** Many players have IPv6, where no NAT stands in the way,
  but home routers still firewall inbound connections and UPnP pinholes are
  rarely offered. Worth adding (bind dual-stack, publish both families) as a
  cheap improvement, not a replacement.
- **Stay direct-only.** The check above tells each host exactly what is
  wrong; players behind carrier-grade NAT use a virtual LAN tool.

## Later, independent of the decision

- Register `bri://` as a Windows URL handler, so clicking an invite starts the
  game and joins.
- PCP (RFC 6887) mapping for routers that dropped NAT-PMP.
- Dual-stack hosting (IPv6).
- Join passwords (first-impressions item 14) travel inside the QUIC session
  and need no change here.
