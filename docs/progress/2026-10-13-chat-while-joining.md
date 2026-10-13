# 2026-10-13 Chat while joining

Maxwell reported that joining announced "connected" only after the map had
loaded and every Add-On had downloaded, and that the joiner could not chat
until then. v20 is the other way round: `GameConnection::onConnect` runs as
the connection is accepted, so everyone sees "X connected." while X is still
on the loading screen, X reads and sends chat the whole time (LoadingGui
pushes the move map, and NewChatHud draws over it), and "X spawned." comes
from the first `spawnPlayer` once they enter the game.

## What changed

- **Session** (`bri-sim`): a player can be *arriving*: admitted (name,
  owner number, administration role, ban check) with no body.
  `Session::arrive` says the welcome line to them and "connected." to
  everyone; `Session::enter_arrival` embodies them, says "spawned." and the
  auto-admin line. While arriving, `Session::command` accepts only
  `Command::Chat` (same pace, length, repeat and E-Tard checks) and refuses
  everything else with "You are still loading"; movement and reports find no
  player. Leaving while arriving says only "has left the game.". Names stay
  unique across arriving and spawned players, arrivals survive a map change,
  and both calls are recorded for match replays (`Call::Arrive`,
  `Call::EnterArrival`). A join without a loading stage (hosting, bots,
  rejoins after a dropped connection) is unchanged.
- **Wire** (`bri-net`, protocol change `chat-while-joining.md`):
  `Purpose::Lobby` connection, answered with `Message::Arrived`; the client
  sends `LobbyRequest::Chat`, the host sends `Message::LobbyChat` and chat and
  sound notices. The game join carries `Hello::arrival`, signed into the
  identity transcript and checked against the loading connection's principal;
  it spawns that player and closes the loading connection without a
  departure. Loading players count toward the player limit and the listing.
  `client::Arrival` is the client side, and `Client::connect_arriving` joins
  with its ticket, running a `ready` future between the Add-On load and the
  join.
- **Client app**: a remote join opens the loading connection first, keeps it
  across the Add-On reload continuation (so the player is announced once),
  loads the map before the game join (so "spawned" means in the world), and
  drops it on entry, failure or cancel. Chat typed on the loading screen goes
  over it, and its lines show in the chat HUD; the game's chat picks up after
  the last line shown.
- **UI**: `UiUpdate::LoadingChat`; with it the loading screen draws the chat
  and the chat key opens the chat box there. Nothing else reaches gameplay.

## Evidence

- `cargo test -p bri-net --test chat_while_joining`: Ann is in the game; Bob's
  loading connection makes Ann see "Bob connected." with no body for Bob; Bob
  and Ann chat both ways, then again while Bob downloads the server's Add-On
  (his join waits until they have); Bob's join spawns the announced owner
  number, Ann sees "Bob spawned." once and "Bob connected." once. Also: a
  joiner who leaves while loading only "has left the game."; another player
  presenting Bob's ticket is refused.
- `cargo test -p bri-sim --test session a_loading_player`: the session-level
  announcements, chat-only commands, name numbering and spawn.
- `cargo test -p bri-ui --test runtime_input a_player_the_host_accepted`:
  the chat key on the loading screen, only once the host has accepted.
- `python tools/check.py focused` over bri-client, bri-net, bri-sim, bri-ui.

## Next

Maxwell's playtest: join a server with Add-Ons from a second PC, chat from
the loading screen, and check the connected/spawned lines and order.
