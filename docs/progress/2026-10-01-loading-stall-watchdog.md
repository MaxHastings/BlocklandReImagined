# 2026-10-01 Loading fails on a stall, not a fixed total

`client/src/network.rs` failed every host or join that took longer than
120 s in total ("Connection/content preparation timed out"). That included
a host still loading its own map, so a big build on a slow PC failed while
it was working.

`Worker::start` now takes the load's `bri_progress::Progress`. The load
fails only when progress stops advancing for 60 s (`PEER_STALL`) in a stage
that waits on the server (`Stage::waits_on_peer`: starting, connecting,
downloading packages, waiting for the server, receiving the world,
spawning). The computer's own work never times out: checking content,
loading the map, starting the server, building bricks and loading
graphics. The player can still cancel. A dead or unreachable server still
fails at QUIC's 15 s idle timeout, and a live server that stops sending
fails 60 s after its last progress.

Guard tests, on a paused tokio clock, in `network.rs`. Both fail on the old
code:
- `a_host_preparing_a_big_build_is_never_timed_out` covers 600 s in
  LoadingMap.
- `a_server_that_stops_answering_fails_once_nothing_advances` covers a slow
  download that keeps arriving and then stops. It fails exactly one stall
  after the last byte.
