# Running a dedicated server

`bri-server` hosts a game with no window, like a Blockland v20 dedicated
server. It ships in the Windows (`bri-server.exe`) and Linux (`bri-server`)
downloads, next to the game, and uses the game's own `content` folder, so
everyone who plays with the same download joins without extra steps.

## On a Linux VPS

Any 64-bit Linux from Ubuntu 22.04 or Debian 12 on works. No graphics card,
sound or desktop is needed.

1. On https://github.com/MaxHastings/BlocklandReImagined/releases/latest
   copy the link of the zip ending in `-linux.zip`, then on the VPS:
   ```sh
   wget <that link>
   unzip BlocklandReImagined-*-linux.zip && cd BlocklandReImagined-*-linux
   ```
2. Open the game port for UDP: `sudo ufw allow 28000/udp` (and the same
   port in your VPS provider's firewall, if it has one).
3. Start the server on a map:
   ```sh
   ./bri-server content slate server-state 0.0.0.0:28000
   ```
   Maps: `slate`, `bedroom`, `kitchen`, `slopes`, `construct`, `destruct`,
   `bedroomdark`, `kitchendark`, `skylands`, `slatedesert`,
   `halloweenslate`, `slatesearevised`, `slatestormrevised`. A wrong name
   prints the full list.
4. Stop it with Ctrl+C. It saves the world into `server-state`. Next time,
   start it with `resume` instead of the map name to carry on building:
   ```sh
   ./bri-server content resume server-state 0.0.0.0:28000
   ```
5. Players join with **Join Game → Connect to IP** and the VPS address,
   for example `203.0.113.7:28000`.

To keep it running after you log out, start it inside `tmux` or `screen`
(Ctrl+C there still saves).

## Settings and admin passwords

The first start writes `server-state/server.json`. Stop the server, edit
it, and start it again:

- `super_admin_password` and `admin_password`: type one in the Player List
  in game to become Super Admin or Admin. Empty turns that login off.
- `settings.name`: the server's name in Connect to IP and the LAN list.
- `settings.max_players`: how many can join (1 to 32 is usual).
- The rest are Start Game's Advanced Config (brick limit, falling damage,
  vehicles and so on). `settings.port` is not used here; the port is the
  one in the start command.

Ranks given in game (`/admin <name>`, `/superAdmin <name>`) and bans are
kept in `server-state/administration.json`.

## Add-Ons

The server runs the Add-Ons turned on in `content/packages.json`, the same
ones the game ships with. Players who are missing one download it from the
server when they join.

## Not there yet

- Change Map from the Admin menu: restart the server with another map.
- No autosave while it runs: it saves when stopped with Ctrl+C.
- No public server list: share the address.
