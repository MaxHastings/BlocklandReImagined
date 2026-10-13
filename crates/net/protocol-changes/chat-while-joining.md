`Purpose::Lobby`: a connection that is accepted before the player downloads
the server's Add-Ons and loads the map. Its Hello is answered with
`Message::Arrived` (owner, name, arrival ticket, chat cursor); then the client
sends `LobbyRequest::Chat` and the host sends `Message::LobbyChat` and chat and
sound `Message::Notice`s. `Hello::arrival` names that ticket on the game join
(and is signed into the identity transcript), which spawns the announced player.
