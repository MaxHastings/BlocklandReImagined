# Blockland ReImagined: tester guide

Thanks for testing. This page covers installing, playing together, what to
send when something breaks, and what is known not to work yet. What the
game can and can't do is in `FEATURES.md`; things to try and the default
keys are in `PLAYTEST.md`.

## Install

1. You need Windows 10 or 11 (64-bit). Other systems are not supported. A
   graphics card or graphics built into the processor both work; with
   neither, the game falls back to slow software drawing.
2. Extract the whole zip to a normal folder you can write to, such as your
   Desktop or Documents. Not Program Files. Keep the files together.
3. Run `Launch.cmd`. A console window opens next to the game; leave it
   open while you play. Nothing else needs installing, and you don't need
   the original Blockland.
   Or use `BlocklandReImagined.exe` on its own: put it anywhere and run
   it. It unpacks the game into your user folder
   (`%LOCALAPPDATA%\BlocklandReImagined`) on first start, which takes a
   few seconds, and keeps your settings, saves and Add-Ons there. A newer
   exe updates the game and keeps them.
4. **"Windows protected your PC"**: the game isn't signed yet. Click **More
   info**, check the name is `Launch.cmd` or `bri-client.exe`, then **Run
   anyway**. If Windows blocked the download itself, right-click the zip,
   choose **Properties**, tick **Unblock**, and extract it again.

The first start asks a few things once:

- Your controls: standard if you have a numpad, laptop if you don't.
- Whether to play the Tutorial now. It teaches moving, building, tools and
  driving, and you can start it later from the main menu.
- Your name, if you're still "Blockhead".

It also picks Low, Medium or High graphics from your hardware. Change that
in Options > Graphics. If the game runs slowly, `PLAYTEST.md` has a short
list of settings to try.

Your settings, saves, screenshots and identity are kept in the `user-state`
folder beside the game. To move to a newer build, extract it to a new
folder and copy `user-state` across.

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

All of these are in the `logs` folder beside the game:

| File | What it is |
|---|---|
| `session-<time>.log` | everything the game printed during that run |
| `crash-<time>.txt` | what went wrong, with the end of the session log |
| `crash-<time>.dmp` | a memory snapshot, after a hard crash only |
| `client-<time>.stderr.log`, `.stdout.log` | what `Launch.cmd` caught, useful if the game never opened |

Please send:

- The version from the bottom corner of the main menu, for example
  `2026-09-28-a17 (1a2b3c4d5)`.
- The newest `session` log. After a crash, also the `crash` files with the
  same time.
- The map, what you did, what you expected (what Blockland v20 did, if
  you know), what happened, and whether it happens again.
- A screenshot, if it's visual: Ctrl+P saves one to `user-state\screenshots`.
- For "it's slow" reports: the session log already records your frame
  rate every minute, so the log is enough.

Keep crashes and things that stop you playing separate from "this feels or
looks different from v20".

Don't share `client.identity` or `host-identity.bin` from `user-state`:
they're your identity and your server's.

## Known limits

- **Windows only.**
- **Unsigned.** SmartScreen warns on first start (see Install).
- **Gamepads** work while playing (move, look, jump, crouch, fire, jet).
  Menus and building still need a keyboard and mouse.
- **Vehicle handling** is rebuilt, not copied from v20's engine. Tell us
  where driving feels off.
- **Bots** steer simply and can get stuck on complex builds.
- **Some v20 settings aren't there yet**: Censor Chat, Press Up to Repeat
  Chat, the ghost brick colour options, and Render My Player, Items and
  Jets. `FEATURES.md` lists everything still missing.
- **Old Add-On scripts don't run.** Imported v20 Add-Ons bring their
  bricks, weapons and vehicles, but not their custom behaviour (see
  `FEATURES.md`).
- **Lighting, shadows, water and sky** aren't final.
- **Knocked-out bricks** tumble differently on each player's screen. That's
  on purpose: they're only for show and never affect play.

`KNOWN-ISSUES.md` has a few more details.
