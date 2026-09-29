# Hosting and joining

Players expect to click Host, send a friend something, and have the friend
join, with no router settings. This document covers how the game gets as
close to that as it can with direct connections alone.

Scope: direct connections only, with no service of ours and no third-party
service (Max, 2026-09-28: "stick to direct ip", "I do not want to have to host
any relay service or rely on 3rd party crap"). The game talks only to the
host's own router and to the other player. A public server list or master
server is out of scope.

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

### Probe over the game port (protocol 34)

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
2. Take the public address from the router: the outside address it reports
   through UPnP (`GetExternalIPAddress`) or NAT-PMP. Nothing outside the home
   network is asked.
3. Probe `public address:port` with the host's own key. A router that
   forwards the port and loops traffic back proves the path end to end.

When the router gives no public address, the host still gets an invite at
this PC's home network address to copy (`/invite`), for players on the same
network; the verdict says what to forward first for friends outside.

The host player then reads one verdict in chat:

| Verdict | Evidence | What the player is told |
| --- | --- | --- |
| Reachable | the self-probe answered | friends can join; invite on the clipboard |
| Likely | the router opened the port, no loopback | friends should be able to join; invite on the clipboard (or, if the router kept its address to itself, where to find it) |
| Shared address | the router's outside address is private or carrier-grade (100.64/10) | a provider or second router shares the address; ask for a public IP or use a virtual LAN tool |
| Needs forward | no UPnP or NAT-PMP | turn on UPnP, or forward UDP *port* to *this PC's LAN address*; then share the address |
| Unknown | this PC has no network route | nothing could be checked |

Without an outside observer the check cannot see a second NAT whose router
reports a public-looking address, nor a provider firewall; those hosts get
"should be able to join" and learn otherwise when a friend tries.

`/invite` copies the host's invite again. LAN hosts get an invite with their
LAN address. The dedicated server prints the same lines.

### Windows Firewall

The first time a program listens, Windows asks whether to allow it. Cancel, or
allowing only private networks while on a public one, leaves block rules that
stop every friend. LAN and Internet hosts read the inbound rules for the game
executable and for the game's UDP port through PowerShell's NetSecurity module
(no administrator rights, enum names are not localized) and decide per active
network profile (`firewall::decide`). When friends would be blocked, a yes/no
box offers the fix: the game starts itself elevated (`bri-client
--allow-firewall <port>`, one Windows permission prompt), deletes this
program's inbound rules (a block rule beats any allow rule) and replaces the
rule named "Blockland ReImagined" with one allowing UDP on the game port and
LAN discovery (28050) on every profile. The rule names ports, not the
program, so a new build in another folder is let through without asking
again; the cost is that any program listening on those ports is let through
too, which is acceptable for game ports the player chose to host on.
Declining changes nothing.

### Join list

Opening Join Server searches the LAN and probes every saved server at once
(two-second timeout each). Saved servers live in `servers.json` in the client
state folder: favourites (up to 64, kept) and the last ten joins. A saved
server remembers its invite, so joining from the list is verified even
without a pin. The v20 "Query Internet" button, which had no master server to
query, is now Favorite/Unfavorite for the selected server. Favourites are
listed first and marked `*`. A saved server that does not answer shows why in
the Map column: no answer, host changed, other version, unknown name.

## Not planned: join codes and a relay

Short join codes, hole punching and a relay fallback would need a service
that someone runs (ours, Steam or Epic). Max decided against any such service
on 2026-09-28, so none is planned. A built and tested version was shelved
outside the repository; everything that joins still goes through
`JoinTarget::parse` and `resolve`, which is where one would plug in.

## Later

- Register `bri://` as a Windows URL handler, so clicking an invite starts the
  game and joins.
- PCP (RFC 6887) mapping for routers that dropped NAT-PMP.
- Dual-stack hosting (IPv6).
- Join passwords (first-impressions item 14) travel inside the QUIC session
  and need no change here.
