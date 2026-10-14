# 2026-10-14 Loading players on the player list

Max reported that someone who connects does not appear on the player list
until they spawn.

Cause: the client builds the list from `Replica::names`, which the host fills
from `Session::names()`, the spawned peers only. A player who has connected
and is still downloading Add-Ons and loading the map lives in
`Session::arrivals` and was never replicated, although everyone is already
told "X connected." and can chat with them (v20's `onConnect` adds them to
the list at that point).

Change: `Checkpoint::loading` and `Delta::loading` carry the loading players
by owner with their names (`crates/net/protocol-changes/loading-players.md`).
The replica keeps them in `Replica::loading`, the client view in
`View::loading`, and the player list adds them as rows with `loading: true`,
shown as "Name (loading)" until they spawn and move to `names`.

Evidence:
- `cargo test -p bri-net --test chat_while_joining --test replication`: the
  joiner tests now wait for the loading player to appear on another player's
  replica, and to leave it on spawning or on leaving while loading.
- `cargo test -p bri-ui --lib players`: a loading row is listed as
  "Ada (loading)".

## Auto admin on connect

Max also reported that an auto admin or super admin only got their rank when
they spawned. The role was already granted on connect (`Session::admit`),
but "X has become Super Admin (Auto)" only went out with "spawned.", and the
list did not show the player at all. `Session::greet` now sends the auto
admin line right after "connected.", as v20's `autoAdminCheck` runs on
connect; spawning only says "spawned.". With the loading rows above, the
list's A/S column shows the rank while they load (it reads the admin
snapshot, which already included loading players).

- `cargo test -p bri-sim --test session`: 50 passed, including the new
  `a_loading_auto_admin_is_announced_on_connect`.
