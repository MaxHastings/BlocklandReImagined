# Blockland ReImagined: tester guide

Thanks for testing. This page covers installing, playing together, what to
send when something breaks, and what is known not to work yet. What the
game can and can't do is in [FEATURES.md](FEATURES.md); things to try and the
default keys are in [PLAYTEST.md](PLAYTEST.md). This is an alpha: back up
valuable saves and custom Add-Ons before updating.

## Install

1. Download for Windows 10 or 11 (x86-64), an Apple Silicon Mac (M1 or newer),
   or Linux x86-64 with glibc 2.35 or newer (Ubuntu 22.04, Debian 12,
   or a compatible distribution). A graphics card or graphics built into the
   processor both work; with neither, Windows and Mac fall back to slow
   software drawing. Linux needs a Vulkan driver: most desktop installs have
   one, and if the game says none was found, install Mesa's Vulkan drivers
   (AMD, Intel) or NVIDIA's own driver. Software drawing on Linux needs
   Mesa's software Vulkan driver (lavapipe) installed.
2. Extract the whole zip to a normal folder you can write to, such as your
   Desktop or Documents. On Windows, avoid Program Files. Keep the files together.
3. On Windows, run `Launch.cmd`; leave its console open while you play.
   On Mac, open `BlocklandReImagined.app`; `PLAYTEST-MAC.md` in the Mac release
   explains the first launch. On Linux, run `./launch.sh` from the extracted
   folder in a terminal; if it says permission denied (some extractors drop
   the executable bit), run `bash launch.sh` instead. You don't need the original Blockland.
4. **Windows: "Windows protected your PC"**: the game isn't signed yet. Click **More
   info**, check the name is `Launch.cmd` or `bri-client.exe`, then **Run
   anyway**. If Windows blocked the download itself, right-click the zip,
   choose **Properties**, tick **Unblock**, and extract it again.
   **Mac:** if macOS blocks the app, open System Settings → Privacy & Security
   and choose **Open Anyway** for BlocklandReImagined.

The first start asks a few things once:

- Your controls: standard if you have a numpad, laptop if you don't.
- Whether to play the Tutorial now. It teaches moving, building, tools and
  driving, and you can start it later from the main menu.
- Your name, if you're still "Blockhead".

It also picks Low, Medium or High graphics from your hardware. Change that
in Options > Graphics. If the game runs slowly, `PLAYTEST.md` has a short
list of settings to try.

Your settings, saves, screenshots and identity are kept apart from the game
in your state folder:

| System | State folder |
|---|---|
| Windows | `%LOCALAPPDATA%\BlocklandReImagined` |
| Mac | `~/Library/Application Support/BlocklandReImagined` |
| Linux | `$XDG_DATA_HOME/blockland-reimagined`, or `~/.local/share/blockland-reimagined` if unset |

## Updating and backups

Close the game, back up your state folder, then extract a newer release to a
**fresh folder** and start it normally. It finds the same settings and saves.
Keep the old release until you have checked your important builds. Alpha
formats may change; finding an old save does not guarantee it can be loaded.
There is no automatic update installation or compatibility migration.

On Windows and Linux, custom Add-Ons live in the release folder's
`content/Add-Ons` (ZIPs awaiting import/install) and `content/addons` (installed
native folders). Back up both. Move only your own additions into the new
release, then check and enable them in **Start Game → Add-Ons**. Keep the new
release's bundled content and `packages.json`; copying the old content tree
wholesale can restore outdated game files and choices. Recheck your host's
Add-On selections before starting a game.

On Mac, the app uses a separate writable content copy for each build. Follow
[Mac file locations](PLAYTEST-MAC.md#where-things-are) to move custom Add-Ons;
do not edit files inside the signed app.

Normal local play has no autosave: save important builds using **Save Bricks**.
The dedicated server has its own shutdown and crash-recovery behavior, described
in the dedicated server guide shipped with Windows and Linux.

**Bringing your old Blockland saves.** Open Load Bricks in a game and press
**Saves Folder**. Copy your old `.bls` saves into that folder, either whole
map folders from the old game's `saves` folder (such as `Slate`) or single
files. The game converts them in the background the next time it starts,
or when you next open Load Bricks, and lists them under their map; loose
files are under **Other**. Your original files are never changed, and the
game never looks anywhere else for saves: only what is in this folder is
listed. A save that can't be converted is skipped and noted in the log.

## Brick colorsets

In **Start Game**, press **Colorsets...**, choose a palette, and press **Use**.
Default and Trueno's are included. **Folder...** opens your `colorsets` folder;
add a named `.txt` file, or a subfolder containing `colorSet.txt`, and return to
choose it. Guests receive the hosted world's colors automatically. When loading
a brick save with different colors, use the existing color-matching choice.

## Playing together

Use the **same release on the host and all joining players**. Alpha versions
can change the network protocol. A protocol mismatch needs matching game builds;
automatic Add-On downloads do not upgrade the game.

Everything is a direct connection between players. There are no accounts
and nothing to sign up for.

**Same house or LAN.** The host picks **Start Game**, sets the server type
to **LAN**, and launches. Everyone else opens **Join Game**: LAN games show
up in the list by themselves. If Windows asks about the firewall, choose
Allow. If friends still can't see you, the game offers to fix the firewall
for you (one Windows permission prompt).

**Over the internet.**

1. The host picks **Internet** in Start Game and launches.
2. The game asks your router to open its port. Within about ten seconds
   the chat says whether friends can reach you. When they can, an invite
   is copied to your clipboard (it starts with `bri://`). Type `/invite`
   to copy it again.
3. Send the invite to your friends. They paste it into **Join Game >
   Connect to IP**. A plain address such as `203.0.113.10` or
   `203.0.113.10:28000` works too.

If the chat says the port needs forwarding, your router didn't open it
automatically. Either turn on UPnP in the router, or forward **UDP port
28000** to the host PC's address, which the chat message shows. Only
28000 matters for joining. UDP 28050 is used just to find LAN games in the
list, is never needed from outside, and doesn't need forwarding. If the
chat says your internet provider shares your address with other
customers, forwarding won't help; a virtual LAN tool is the way around it.

The first time you join a host, the game remembers who they are. If that
host's identity later changes, the game asks before letting you continue.

**Add-Ons on a server.** Joining a server that runs Add-Ons you don't have,
or have in a different version, downloads the server's copies and puts you
in the game. Your own Add-Ons the server doesn't run are left off for that
game. If an Add-On wants to run
its own code on your PC, the game asks "Trust and join" or "Leave" before
it runs. That code is kept in a sandbox, and you can take the trust back
with Forget Trust on the Add-Ons screen.

## When something breaks

Every run writes a log. A crash leaves a report, and the next start tells
you which files it wrote and offers to open the folder. Nothing is sent
anywhere automatically.

On Windows and Linux, logs normally live in the extracted game's `logs`
folder, beside the executable. If that folder cannot be written, session
and crash reports use `logs` in your state folder instead. On Mac, they
always use the state folder's `logs`. The crash prompt shows the actual folder.

| File | What it is |
|---|---|
| `session-<time>.log` | everything the game printed during that run |
| `crash-<time>.txt` | what went wrong, with the end of the session log |
| `crash-<time>.dmp` | a Windows memory snapshot, after a hard crash only |
| `client-<time>.stderr.log`, `.stdout.log` | what the Windows/Linux launcher caught, useful if the game never opened (in the release folder's `logs`) |

Please send:

- The complete version and commit from the bottom corner of the main menu,
  for example `v0.3.2 (e4d9bb05d)`.
- The newest `session` log. After a crash, also the `crash` files with the
  same time.
- The map, what you did, what you expected (what Blockland v20 did, if
  you know), what happened, and whether it happens again.
- A screenshot, if it's visual: Ctrl+P saves one to `screenshots` in your
  state folder.
- For "it's slow" reports: the session log records frame times every minute.
  Include that log, your CPU/GPU, graphics settings, map, approximate brick/bot
  count, and whether you were hosting or joining.

Keep crashes and things that stop you playing separate from "this feels or
looks different from v20".

Don't share `client.identity` or `host-identity.bin` from your state folder:
they're your identity and your server's.

## Quick troubleshooting

| Problem | What to check |
|---|---|
| No game window appears | Start with the launcher and keep its console open. Read its error and newest `logs` files; keep the whole extracted folder together. |
| Linux says permission denied | Run `bash launch.sh` from the extracted folder. |
| Linux cannot find a graphics adapter | Install a Vulkan driver for your GPU; see Install above. |
| A friend cannot join | Match release versions, check the address/port, and follow Playing together above. Internet hosting needs reachable UDP 28000. |
| A build or Add-On disappeared after updating | Check the state folder and custom content locations under Updating and backups. Avoid overwriting the new release's bundled content. |
| A legacy Add-On imported but does nothing | Read its `IMPORT-REPORT.md`. Scripts need a supported native port; importing resources alone does not recreate behavior. |
| A source build reports missing packs | Developers should rerun `python tools/bootstrap.py`; a downloaded release should be extracted again into a fresh folder. |

## Known limits

- **Unsigned Windows builds; ad-hoc signed Mac app.** SmartScreen or macOS
  may block the first start (see Install).
- **Gamepads** work while playing (move, look, jump, crouch, fire, jet).
  Menus and building still need a keyboard and mouse.
- **Vehicle handling** is rebuilt, not copied from v20's engine. Tell us
  where driving feels off.
- **Bots** (Blockhead Bot Add-On) path round builds but not round moving vehicles or players; they stop, then try another way.
- **A few v20 settings aren't there yet**, such as Render My Player.
  `FEATURES.md` lists everything still missing.
- **Old Add-On scripts don't run.** Imported v20 Add-Ons bring their
  bricks, weapons and vehicles, but not their custom behaviour (see
  `FEATURES.md`).
- **Lighting, shadows, water and sky** aren't final.
- **Knocked-out bricks** tumble differently on each player's screen. That's
  on purpose: they're only for show and never affect play.

[KNOWN-ISSUES.md](KNOWN-ISSUES.md) has the detailed limits and open reports.
