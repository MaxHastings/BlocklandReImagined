> **Historical note. Written 2026-10-01 (the file's date in the cloud project folder, `ports/magazine-seam.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Magazine seam (shared by Tier+Tactical, Adventure, Sniper and any port)

Owner: the Tier lane. Code: branch `claude/tier-tactical-3onhik`, in the
weapon-seams commit 2717c502 on top of the loader commit 29bbc4ab. Final user docs
are in `docs/modding/README.md` ("Magazines", plus the script table).
Tests: `crates/weapons/tests/magazines.rs` and
`crates/sim/tests/script_api.rs` (`a_magazine_shows_its_rounds_...`).

## weapons.json: `images.<id>.magazine`

```json
"magazine": {
  "size": 30,                 // 1..=1000 rounds
  "ammo": "9mm",              // ammo type: 1..=32 chars [A-Za-z0-9._-]
  "per_shot": 1,              // default 1, 1..=size
  "reload_ticks": 240,        // 120 per second, 1..=1200
  "one_by_one": false,        // shotgun: one round per reload_ticks
  "reserve": 90,              // starting reserve, 0..=100000
  "max_reserve": 180,         // cap, default 100000, 1..=100000
  "reload_sequence": "shiftDown", // holder's arm animation as a reload starts
  "reload_sound": "pkg:reload",   // as each reload (or each shell) starts
  "empty_sound": "pkg:click",     // a shot with no rounds
  "display": "9MM Rounds"         // HUD name; defaults to `ammo`
}
```

Rust: `bri_weapons::Magazine` on `Image.magazine: Option<Magazine>`.

## Behaviour (engine, no rule needed)

- Each shot from the right hand (hand 0) takes `per_shot` rounds. A shot
  without them is refused, plays `empty_sound` and starts a reload.
- The last round starts a reload by itself. The light key reloads a held
  gun with a magazine, unless the image's `commands.light` takes the key.
- A reload lasts `reload_ticks` and fills the magazine from the reserve
  (all it lacks; one round with `one_by_one`, repeating until full or
  the reserve is empty). With `one_by_one`, a trigger pull stops the
  loading and fires.
- Putting the gun away cancels its reload.

## Where rounds and reserve live

- Rounds: per player, per item (the tool item id it was drawn as; the
  image id when a rule mounts an image with no tool selected). A
  `mount_image` scope swap keeps the tool's key. A new gun is full.
- Reserve: per player, per **ammo type**, shared by every gun with the same
  `ammo`. The first gun of a type a player draws brings its `reserve`
  (capped by its `max_reserve`). It can be `Endless`.
- Caps: `give_ammo` caps at the largest `max_reserve` of any magazine
  using that ammo; set_* calls cap at 100000; 64 ammo types per player.
- A new life (respawn) gives full magazines and starting reserves again.

## A dropped or thrown gun

The drop keeps its magazine's rounds (`Drop.rounds`). Whoever picks it up
gets those rounds in their magazine of that item, and keeps their own
reserve. If the thrower still carries another of the same item, the drop
carries no rounds (it comes full).

## Script (rules) API, capability `player`

| Call | Does |
|---|---|
| `give_ammo(p, ammo, rounds)` | Add 1..=100000 rounds to the reserve, up to the cap; an Endless reserve stays Endless; an empty gun waiting on ammo reloads at once |
| `set_reserve(p, ammo, rounds)` | Set the reserve; `()` = Endless (never runs out) |
| `set_rounds(p, item, rounds)` | Set the rounds in that item's magazine, up to its size |
| `reload(p)` | Start a reload, as the light key does |
| `player(p).magazine` | Read: `()` without a magazine, else `#{ item, rounds, size, ammo, reserve, reloading }`, where `reserve` is `()` when Endless |

There is no separate `ammo(p)` read; `player(p).magazine` is it. Rust
side: `WeaponsWorld::{ammo, reserve, set_reserve, give_ammo, set_rounds,
reload, reset_ammo}` and `Event::Ammo { actor }`.

## HUD

The engine sends the holder alone a bottom print
`<just:right>{display} {rounds} / {reserve}` (with `Reloading` during a
reload, `inf` for Endless), only when the numbers change. It clears when
the hand no longer holds a gun with a magazine. No new network message:
it is the existing private bottom-print notice. A port that wants its own
look can keep printing over it with `bottom_print`.

## Not covered (stay in rules)

Fire-mode switches, burst fire (use `set_rounds` to take the extra
rounds), ammo kinds swapped with the cancel key, magazines in the left
hand (hand 1 does not spend rounds).
