# Running a dedicated server

`bri-server` hosts a game with no window, like a Blockland v20 dedicated
server. It ships in the Windows (`bri-server.exe`) and Linux (`bri-server`)
downloads, next to the game, and uses the game's own `content` folder.
Players should use the same release. macOS releases do not include a dedicated
server executable.

## On Windows

Extract the whole Windows release. Open PowerShell in that folder and run:

```powershell
.\bri-server.exe content slate server-state 0.0.0.0:28000
```

Allow the firewall prompt for the network you intend to host on. Internet
players need reachable UDP 28000; LAN discovery uses UDP 28050. The settings,
shutdown, resume and Add-On instructions below also apply on Windows.

## On a Linux VPS

Use Linux **x86-64** with glibc 2.35 or newer, such as Ubuntu 22.04 or Debian
12. No graphics card, sound or desktop is needed.

1. Download and unzip the newest Linux release on the VPS:
   ```sh
   wget https://github.com/MaxHastings/BlocklandReImagined/releases/latest/download/BlocklandReImagined-linux.zip
   unzip BlocklandReImagined-linux.zip
   cd BlocklandReImagined-<version>-linux
   ```
   Replace `<version>` with the folder name in the ZIP. If the extractor
   dropped executable permissions, run `chmod +x bri-server`.
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
4. Stop it with Ctrl+C. It saves the world, with the mini-game running
   in it (teams and settings), into `server-state`. Stopping it with
   `kill` or a service manager (SIGTERM), closing its terminal (SIGHUP) or
   closing its window on Windows saves the same way. Next time, start it
   with `resume` instead of the map name to carry on building; the
   mini-game comes back for the player who ran it when they rejoin:
   ```sh
   ./bri-server content resume server-state 0.0.0.0:28000
   ```
5. An admin changes the map from the Admin menu's Change Map, as in
   a game hosted from the menu. The new map starts empty.
6. Players join with **Join Game → Connect to IP** and the VPS address,
   for example `203.0.113.7:28000`.

To keep it running after you log out, start it inside `tmux` or `screen`
(Ctrl+C there still saves).

While it runs it keeps one recovery file, `server-state/recovery.json`,
rewritten about once a minute while the world changes and removed when the
server stops normally. If the server crashes or is killed outright, the next
start uses that file to prepare the world `resume` carries on from. Recovery
usually trails changes by about a minute; it is not a backup guarantee.
Back up `server-state` while the server is stopped before updating. Starting
with a map name creates a new world; use `resume` to continue the saved one.

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

The server runs the Add-Ons turned on in `content/packages.json`. Keep the
release's base package selection. To configure custom Add-Ons, use the matching
client release's **Start Game → Add-Ons** screen on a content copy and transfer
that selection together with the required Add-On folders. Stop the server
before replacing content; back up its state and custom Add-Ons first.
Players download missing shared Add-Ons on join. This does not update their
game binary or make incompatible alpha Add-On formats load.

## Not there yet

- No public server list: share the address.
