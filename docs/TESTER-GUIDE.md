# Blockland ReImagined: tester guide

Thanks for testing. This page covers installing, playing together, what to
send when something breaks, and what is known not to work yet. What the
game can and can't do is in `FEATURES.md`; things to try and the default
keys are in `PLAYTEST.md`.

## Install

1. Download for Windows 10 or 11 (x86-64), an Apple Silicon Mac (M1 or newer),
   or Linux x86-64 with glibc 2.35 or newer (Ubuntu 22.04, Debian 12,
   Fedora 36, or newer). A graphics card or graphics built into the
   processor both work; with neither, the game falls back to slow software
   drawing. Linux needs a Vulkan driver: most desktop installs have one, and
   if the game says none was found, install Mesa's Vulkan drivers (AMD,
   Intel) or NVIDIA's own driver.
2. Extract the whole zip to a normal folder you can write to, such as your
   Desktop or Documents. On Windows, avoid Program Files. Keep the files together.
3. On Windows, run `Launch.cmd`; leave its console open while you play.
   On Mac, open `BlocklandReImagined.app`; `PLAYTEST-MAC.md` in the Mac release
   explains the first launch. On Linux, run `./launch.sh` from the extracted
   folder in a terminal. You don't need the original Blockland.
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

To update, extract the newer release to a new folder and start it normally:
it finds your state folder by itself. On Windows and Linux, Add-Ons you
dropped in or imported live in the game folder's `content`; copy
`content/Add-Ons` and `content/addons` across too. On Mac, each build has its
own content copy under the state folder; see `PLAYTEST-MAC.md` before moving
custom content.

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

- The version from the bottom corner of the main menu, for example
  `2026-09-28-a17 (1a2b3c4d5)`.
- The newest `session` log. After a crash, also the `crash` files with the
  same time.
- The map, what you did, what you expected (what Blockland v20 did, if
  you know), what happened, and whether it happens again.
- A screenshot, if it's visual: Ctrl+P saves one to `screenshots` in your
  state folder.
- For "it's slow" reports: the session log already records your frame
  rate every minute, so the log is enough.

Keep crashes and things that stop you playing separate from "this feels or
looks different from v20".

Don't share `client.identity` or `host-identity.bin` from your state folder:
they're your identity and your server's.

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

`KNOWN-ISSUES.md` has a few more details.
