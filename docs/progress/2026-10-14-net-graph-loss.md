# 2026-10-14 Net graph packet loss: jitter no longer reads as loss

Max's friend, joining a v0.3.0 host from far away, saw high packet loss on
the net graph (Ctrl+N). The graph's loss is the transport's count of the
client's own packets declared lost over the last 4 s.

## What was wrong

QUIC declares a packet lost when a later one is acknowledged and it is still
unacknowledged after 9/8 of a round trip. On a jittery link packets and
acknowledgements overtake each other, so packets that arrived were counted
lost (and reliable data in them retransmitted). Measured over
`bri_net::impair` with the client sending input as the game does:

| Link | Real loss | Shown, 9/8 | Shown, 1.5 | Shown, 2.0 |
|---|---:|---:|---:|---:|
| 30 ms, 20 ms jitter | 0% | 0.8-1.4% | 0.5% | 0% |
| 60 ms, 40 ms jitter | 0% | 8.7-11.1% | 0% | 0% |
| 60 ms, 40 ms jitter, 5% loss each way (`BAD_WIFI`) | 5% | 12.9-14.5% | 4.2% | 3.2% |

Raising the packet-reordering threshold (3, 6, 12) changed nothing; the time
threshold was the cause. `server::transport()` (both ends) now uses 1.5. The
cost is that a truly lost reliable packet the reordering threshold does not
catch is resent half a round trip later than before.

## Traffic check against the September audit

`cargo test -p bri-net --release --test bandwidth -- --ignored --nocapture`:
host rates are at or below the audit. Two things grew in v0.3.0:

- Each player uploads 24 KB/s, up from 13 KB/s, since movement datagrams
  repeat 12 moves instead of 6 (a5057d4). A new budget in
  `an_idle_server_stays_under_its_byte_budget` holds it under v20's 32 KB/s
  Broadband.
- A driving four-wheel vehicle's pose is 276 B (129 B in the audit), 11 KB/s
  to every player at 40 Hz. It carries the driver's prediction state (tyre
  state, spin, mouse steering, steering flags, the actor motor) to players
  who only draw it, as `Pose` did before `RemotePose`. Not changed here.

## Tests

- `crates/net/tests/link_loss.rs`: a jittery link without loss shows under
  2%; `BAD_WIFI` shows 1.5-9%.
- `cargo test -p bri-net --release --test link_loss --test bandwidth`: pass.
