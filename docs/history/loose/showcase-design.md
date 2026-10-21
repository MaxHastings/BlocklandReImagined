> **Historical note. Written 2026-09-28 (the file's date in the cloud project folder, `showcase-design.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Gravity Gun and Steel Ball: current design

Status: 28 September, mid-build. The server side of both Add-Ons works in
headless tests (7 of 8 green); the client effects are next.

Both are ordinary Add-Ons in `packages/showcase/`, built only from what any
modder gets: data files, Rhai rule scripts, WebAssembly and WGSL shaders.
Where the platform lacked something, I added a general engine seam (listed
at the end), never gravity-gun or ball code. Both will ship turned on.

## Gravity Gun

Three Add-Ons: the rule (`gravity-gun`, host only), the tool you hold
(`gravity-gun-tool`, an orange-tinted launcher using the stock rocket
launcher model) and the effects (`gravity-gun-fx`, client code).

**Controls (Half-Life 2 style, on Blockland's two buttons).**
- Right click (jet): grab what you aim at, up to 24 units away, or drop
  what you hold.
- Left click, tapped: punt. Holding nothing, it shoves what you aim at
  (16 units); holding something, it throws it.
- Left click, held: charges a throw, full after 0.75 s. Release to throw.
- You get the gun on join, and `/gravitygun` hands it to you again.

**What it can move.** Players, every vehicle (jeeps, tanks, horses, balls,
boats, the steel ball) and Add-On creatures. Tumbling players count as
players, so you can catch someone mid-flight.

**How it feels.**
- Grabbing drags the object in to just beyond its own size (3.5 to 10
  units) and holds it there, following your view. Turn and it swings round
  with you.
- Response depends on mass: a player snaps into place, a jeep or tank lags
  a moment, a steel ball or cannon turret barely moves. Held things stop
  spinning, and sag slightly under their weight.
- A held object that gets stuck more than about 40 units from where it
  should be is dropped, as is anything you hold when you die, put the gun
  away, sit in a vehicle or the rules stop allowing it.
- Throw speed is 24 to 70 u/s by charge, scaled by weight (players fly
  about 1.4 times as fast, a tank at 1x, the steel ball at about half).
  A punt is a fixed 22 u/s with a little lift.
- Throwing or punting a player knocks them into v20's tumble (the ragdoll
  body used for crashes), so they fly and roll, then get up.

**Minigames and fair play.**
- You may move a player if the minigame lets you hurt them; outside
  minigames, only if they trust you to build. Vehicles: if the minigame
  lets you damage it; outside, if you could ride it (its owner trusts you).
  Creatures: always. The engine enforces this, not the script.
- Whatever you push, hold or throw is credited to you for 5 seconds. A
  vehicle you threw that runs someone over deals its normal run-over damage
  (speed times the vehicle's run-over damage) and the kill is yours. A tank
  thrown at 15 u/s deals about 375, so yes, you can crush someone with a
  tank. Outside minigames nobody is hurt.

**What is synced.** The server decides everything: grabs, holds, throws,
damage. It runs the hold physics itself and the result travels in the
existing vehicle and player pose streams. Each player's gun state is 6
small numbers of Add-On state (held object, charging, shot count, last
target), sent only when it changes. Nothing new on the wire protocol.

**Effects (client code, next).** Each player's game draws everyone's
beams from that state plus the poses it already has:
- an animated energy beam from the gun to the held object (a tube shaped
  and animated in the vertex shader, additive glow, flowing bands);
- a shimmering force field around the held object;
- a charge glow at the muzzle that swells and brightens, with particles
  drawn inward;
- on a throw or punt, a shockwave ring and a burst of sparks at the target.
Particles are GPU-animated: one mesh of quads per effect, moved and faced
to the camera in the shader, so hundreds cost one draw.

## Steel Ball

Three Add-Ons: the rule (`steel-ball`), the kit (`steel-ball-kit`: the ball
vehicle, its model and texture, and the hand-held ball) and the effects
(`steel-ball-fx`).

**The ball.** A true Rapier sphere (not a hull) of radius 1.25 (about player
height), 900 mass (three tanks), grippy (friction 0.8), dull bounce
(0.2), low rolling drag, so it rolls far and keeps its momentum. It is a
vehicle with no seats: nobody can mount it, it counts as a vehicle, and it
is also placeable from a vehicle spawn brick.

**Getting one.** You get the steel ball item on join (`/steelball` for
another). Left click rolls a ball out in front of you at a walking pace;
right click hurls one where you look. Each player keeps three at most; a
fourth replaces their oldest, and `/clearballs` puts yours away. Balls of
players who leave, or that fall out of the world, are cleaned up.

**How it plays.**
- It shoves vehicles aside through real physics (it outweighs them).
- It bowls players over: anyone it rolls into is knocked into a tumble,
  so the ball ploughs on instead of stopping dead against them. Outside
  minigames that is all; in minigames it also hurts (speed x 4, so a
  fast ball kills), credited to its owner, or to whoever threw it with the
  gravity gun.
- In minigames, a hit at 10 u/s or more into a surface breaks the brick
  it hit and small bricks within 1.2 units, using exactly the rocket's
  brick rules (minigame brick damage setting, ownership, respawn after the
  minigame's brick respawn time). Outside minigames it breaks only your own
  bricks, as a rocket does. A gentle roll just bumps into walls.

**Looks.** Without trusting the server's code you see a plain brushed
steel sphere (the fallback model). With it, the fx Add-On draws a polished
mirror sphere over it: sky and ground reflected from the world's lighting
direction, a sharp sun highlight, Fresnel rim, and a faint seam so you can
see it roll.

## Engine seams added (general, documented)

- Script `physics` capability: `push`, `tumble`, `hold`/`let_go`/`held`,
  `object`, `objects`, `objects_near`, `spawn_vehicle`/`remove_vehicle`;
  `aim()` now reports movable objects in front of bricks and whether you may
  move them; players expose eye, look direction, velocity and held item.
- Throw credit: run-overs and brick smashes by a moved vehicle credit the
  mover for 5 s.
- Tool images: `commands` runs Add-On commands on any state (charge,
  fire), and on jet (right click).
- Vehicle data: `smash` (break bricks on hard hits, rocket rules) and
  `shove` (bowl players over).
- Client code (next): read player and vehicle poses and the Add-On's
  public state (`world.read`, sandboxed tier), and additive glow blending.

## Still deciding

- Sounds: the tool has no fire sound yet. I plan to generate original
  thump, hum and zap sounds and play them from the fx code, faded by
  distance.
- Whether jetting should be suppressed while the gravity gun is out. Right
  now right click still gives a small jet hop on player types that can jet.
- Final tuning numbers once I watch the effects in an offscreen render.
