# Experimental bot objective declarations

An Add-On can describe a supported desired gameplay state through
`bot_objectives(p)`. The host's existing bounded planner combines ordinary
pickup and movement actions; your real pickup and zone callbacks still perform
carriage, return, scoring and round completion. The query supplies no commands,
bot waypoints or replacement AI script. This is the current v0.2.2 alpha seam,
not a frozen beta API.

Bots must be actual MiniGame participants, alive, and have the `objective`
behavior enabled in their kind. The supplied Blockhead enables it. Immediate
combat can interrupt an approach. See [bot architecture](../architecture/bots.md)
for selection, evidence and control ownership, and [game rules](rules.md) for
package hooks, state, permissions and zones.

## Declare an existing pickup/return policy

These snippets extend an existing pickup/return policy; they are not a complete
Add-On. Merge these fields into your behavior file and retain its actual
`on_pickup`, `on_zone` and source-initialization callbacks:

```json
{
  "schema_version": 1,
  "script": "parcel.rhai",
  "bot_objectives": true,
  "on_pickup": true,
  "zones": [{"bricks": ["parcel:brick/desk"], "above": 0.4, "period_ms": 50}],
  "state": {
    "global": {"sources": {"default": {}, "persist": false}},
    "player": {
      "carrying": {"default": null, "persist": false},
      "returns": {"default": 0, "persist": false}
    }
  }
}
```

For example, ordinary source creation initializes `sources[brick-id].epoch`
to a nonnegative integer. Pickup records the exact source brick in `carrying`
and optionally mounts the declared image. A genuine return increments that
player's `returns`, clears carriage and restores the source. Those writes
belong in the real gameplay callbacks, never in this query:

```rhai
fn bot_objectives(p) {
    let me = player(p);
    if me == () || !me.bot || !me.alive || me.minigame == () { return []; }
    let sources = get("sources");
    let carried = get_player(p, "carrying");
    let destinations = [];
    for desk in bricks("parcel:brick/desk") {
        if desk.game == me.minigame {
            destinations.push(desk.id);
            if destinations.len() == 8 { break; }
        }
    }
    if destinations.is_empty() { return []; }
    let out = [];
    for home in bricks("parcel:brick/depot") {
        let key = `${home.id}`;
        if home.game != me.minigame || !(key in sources) { continue; }
        if carried != () && carried != 0 && carried != home.id { continue; }
        out.push(#{
            kind: "carry_return", id: `return:${home.id}`,
            source: #{kind: "brick", brick: home.id},
            item: "parcel:weapon/token",
            epoch: #{scope: "global", key: "sources", path: [key, "epoch"]},
            destinations: destinations,
            carriage: #{key: "carrying", worn: #{slot: 2, image: "parcel:image/token"}},
            completion: #{scope: "player", key: "returns", path: []}
        });
        if out.len() == 8 { break; }
    }
    out
}
```

Filter offers using the same eligibility as your real policy: teams, source
availability, game settings and return restrictions. This fragment assumes any
same-game desk accepts the parcel. It does not implement those callbacks or
grant their permissions. Items/images must be from the package or a declared
dependency; the live item and pickup source must match.

Your actual `on_pickup` return still determines native pickup: `()` permits the
usual inventory pickup, `"take"` consumes the item without inventory and starts
its spawner's respawn, and `false` declines native pickup. A callback that stores
worn carriage and removes its own source must avoid also granting an unintended
inventory copy. See the [pickup hook contract](rules.md).

## What each binding means

| Field | Current meaning |
|---|---|
| `id` | Unique offer identity within one query result. |
| `source` | Exact live brick pickup, or `#{kind: "drop", drop: drop_id, spawner: brick_id}` for a live drop attributed to that spawner. New identities are not substituted. |
| `epoch` | Own declared global/player state plus optional map path identifying this source incarnation. Initialize it in real policy, retain it through pickup/drop, and change it when replacement should invalidate a journey. |
| `destinations` | Existing bricks matched by this package's declared zones in the bot's actual game. Their native overlap and zone callbacks determine entry; these are not arbitrary coordinates. |
| `carriage` | Own declared player key containing the exact source **spawner brick ID**. Null or integer zero means empty. Optional `worn` also requires the exact live image in slot **2 or 3**. |
| `completion` | Own declared global/player counter whose integer value must rise above the journey's original baseline. A missing map leaf means zero; a missing declared root/player state or invalid scalar is unavailable. |

Both counters must be nonnegative signed 64-bit integers. Epoch has no
missing-leaf fallback: its complete path must already exist. Keep epoch and
completion separate when replacement can happen without success. One binding
can serve both only if every such replacement truthfully means successful return.
Completion alone does not announce a round winner; your ordinary policy must
perform the canonical scoring/round operation.

The planner retains the completion baseline across pickup and replanning.
Actual carriage observes pickup even after the item disappears. Actor life,
game/round/team, source incarnation and source/zone edits invalidate assumptions.
A successful return can replace the source atomically: completion is checked
before rejecting that changed epoch. An already occupied destination can require
ordinary exit and reentry. Unknown or inaccessible offers produce bounded
failure/repair, not a simulated pickup or awarded score.

## Bounds and read-only rules

- At most **8 opted-in providers**, **8 total offers**, and **8 distinct
  destinations per offer**; a ninth unrelated opted-in provider also reaches
  the global provider cap. Returned offer IDs must not repeat.
- IDs are positive signed-64-bit-compatible integers. State keys are at most
  **64** lowercase ASCII letters/digits/underscores; text identifiers/path
  segments are nonempty, at most **128 UTF-8 bytes**, with no control characters.
  Counter paths have at most **4** segments; aggregate descriptor text is at most
  **8192 bytes**. Unknown kinds/fields and oversized arrays are rejected.
- Each query has a **20,000-operation** Rhai limit and charges the existing
  package/server work share even when rejected. Discovery uses the common fair
  objective turn and grounding/search limits. There is no per-bot query every tick.
- State/entity writes, operations and output are forbidden, including attempted
  writes caught by script code or followed by restoration. No query outcome is
  committed. Read-only does not make snapshot/state cloning free: these use
  existing package-store bounds and shared accounting, not the descriptor text
  cap. Keep queries and their state small. `bricks(kind)` returns at most
  **4096** bricks in native ID order; the query must filter their game/policy.
- Current shared search limits are **32 actions**, **256 nodes**, **12 steps**,
  **4096 candidate evaluations**, **128 facts**, **4096 model terms** and
  **65536 model-text bytes**. Grounding separately caps **128 targets** before
  expansion. Other providers consume the same budget; these are limits, not
  promises that every admitted offer yields a reachable plan.

Use Explain and bounded `objective.query` diagnostics to inspect selection and
rejection. Verified examples are the [unfamiliar-package tests](../../crates/chaos/tests/bot_carryable_objectives.rs)
and the [actual imported CTF journey](../../crates/chaos/tests/bot_carryable_ctf.rs).
The CTF [owner query](../../crates/addon-import/ports/gamemode_slayer_ctf/rules/ctf.rhai)
uses its own policy IDs; engine discovery has no CTF/content-name handler.

## A native physical-hold affordance

Physical-object delivery is separate from worn-item carriage. An image can
describe an existing native hold with this `bot` fragment:

```json
"bot": {
  "fire": "hold", "near": 2.5, "reach": 30,
  "manipulation": {"kind": "hold", "near": 2.5, "reach": 60, "force": 90000, "turn": true}
}
```

This matches the [Gravity Gun tool](../../packages/showcase/gravity-gun-tool/assets/weapons.json)
and its [real hold/release policy](../../packages/showcase/gravity-gun/gravity.rhai).
The image must have command-backed mechanics, no projectile or melee, and
`fire: "hold"`; manipulation requires finite `0 < near < reach <= 2000` and
positive force. The descriptor must accurately match native acquisition,
maintenance and release. Declaration alone does not make an arbitrary script
tool work: live possession, permission, occupancy, reach, lift feasibility and
actual native grip are still checked. See [weapon commands](weapons.md)
and [the current pipeline limits](../audits/npc-pipeline-current.md).
