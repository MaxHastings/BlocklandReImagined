# Tuning bodies in the game

A body's numbers (how fast it runs, where its hands hold a weapon, where
the first-person camera sits, where riders sit) are easiest to get right
while you play it. `/tune` opens the **Tune Body** window over the body you
play, for the host of the game:

- `/tune` tunes the body you play; `/tune <name>` tunes another body in the
  game by its name or archetype id.
- **Part** picks Movement, Hands and eye, Camera, Seats, Animation or
  Health. Each number has a slider and a box for an exact value; a typed
  value may go past the slider's ends.
- A change reaches everyone in the game a moment after the slider stops:
  the host rebuilds the body from its archetype file with the new number,
  by the same code that loaded it, and every player's game gets the tuned
  body. Players of that body carry on with the new numbers.
- **Save to Add-On** writes the changes into the body's archetype file.
  Only the changed numbers are written; the rest of the file keeps its
  text, key order and spacing. A part the file leaves out (a `camera.bob`,
  a hand's socket) is added with the values the body plays now, so adding
  one number changes nothing else.
- **Undo Changes** puts back every number changed since the window opened
  or last saved.

Tuned bodies last for the game: a map change or a new game loads the files
again, so save what you want to keep.

The typed form does the same without the window: `/tune <body> <field> <value> [<field> <value>...]`, for
example `/tune runner movement.forward 20 sockets.right_hand.offset.1 0.125`.
A field is the number's path in the archetype file.

## What it tunes

| Part | Numbers |
|---|---|
| Movement | every decimal number of the archetype's `movement` (speeds, jump, air control, crouch, swim, dive...) |
| Hands and eye | `sockets.right_hand`, `left_hand` and `eye` offsets; `movement.stand_eye`, `crouch_eye`, `eye_forward` |
| Camera | `camera_distance`, `camera.fov`, `camera.eye_offset`, `camera.bob`, `camera.sway`, `camera.third_person` |
| Seats | `sockets.seat` offset, `movement.sit_eye` and `sit_eye_forward`, each rider seat's `mount_points` position |
| Animation | each `ground_speeds` clip speed and `max_time_scale` |
| Health | `max_health` |

The host checks every value as the archetype file is checked when it loads,
so a value the file could not hold is refused with the reason in chat.

## What it does not tune

- A body that `adjusts` a v20 player type, and the v20 player types
  themselves: tune those in their files.
- Numbers inside a movement controller's WebAssembly module (a custom
  movement's own constants): change them in the controller's source.
- Flags, names and choices (`jets`, `model`, `controller`): edit the file
  and host again.

Only the host tunes, and only the host's computer can save: the Add-On's
files are there. When the game's Add-Ons are not the host computer's own
list, Save to Add-On is off and says why.

Code: `crates/sim/src/tuning.rs` (fields and paths),
`crates/sim/src/session/tuning.rs` (the host), `crates/client/src/tune.rs`
(the window's model and saving), `crates/package/src/json_edit.rs`
(writing a file back as written).
