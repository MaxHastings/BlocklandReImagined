# HUD panels

A HUD panel is a JSON file each player's game draws from state it
receives. A rule's Add-On runs only on the host, so a HUD is always its own
Add-On that depends on the rule, as `sample-points-hud` depends on
`sample-survival-points`.

```json
{
  "schema_version": 1,
  "slot": "hud.overlay",
  "anchor": "top_left",
  "title": "SURVIVAL POINTS",
  "background": [0.05, 0.08, 0.12, 0.8],
  "accent": [0.35, 0.85, 0.45, 1.0],
  "text": [0.92, 0.95, 1.0, 1.0],
  "rows": [
    { "label": "Points", "bind": "sample-survival-points:player/points" },
    { "label": "Awarded to everyone", "bind": "sample-survival-points:global/awarded" }
  ],
  "keys": [
    { "key": "N", "label": "Leaderboard", "package": "sample-survival-points", "command": "top" }
  ]
}
```

- `bind` is `rule-id:player/key` (the viewing player's value),
  `rule-id:players/key` (every player's value, one line each) or
  `rule-id:global/key`. The viewer must receive the key (`owner` or
  `everyone` for its own player value, `everyone` otherwise), or the Add-On
  is refused at load.
- `anchor` is `top_left`, `top_right`, `bottom_left` or `bottom_right`.
- Up to 16 rows and 8 keys. A key is one letter `A`-`Z` that sends the
  rule's command (`package` is the rule's id) with no arguments. The game
  refuses letters it already uses.
- Colors are RGBA from 0 to 1.
- `holding` (optional, up to 32) shows the panel only while the viewer
  holds one of these: an Add-On id (any of its images) or an image id.
  An ammo panel listing `["my-guns"]` appears with one of that Add-On's
  guns out and nowhere else.
