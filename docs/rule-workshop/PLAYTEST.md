# Rule Workshop: creator playtest

Rule Workshop ships as an experimental creator tool in v0.2.0. Judge whether it
feels like the next step for Wrench Events and how soon you wish you were writing
an Add-On. These are editable starting recipes, not finished game modes.
The important semantics and architectural assessment are in [DESIGN.md](DESIGN.md).
For blank-brick challenges, the ambition ladder and a friction log, use the
[creator test card](CREATOR-TEST-CARD.md) alongside this recipe guide.

## Start here

Use `Launch.cmd` on Windows, open `BlocklandReImagined.app` on macOS, or run
`launch.sh` on Linux. The release uses the game's normal per-user settings and
saves. Start a local game on Slate.
Steel Ball, Gravity Gun and Portal Bricks are included as optional Add-Ons.
Enable them in Start Game → Add-Ons for the physics recipes; Soccer requires
Steel Ball. Toys is enabled by default for the Add-On route-switch recipe. Put the Gravity Gun in your MiniGame equipment
to use it. Use ordinary vehicle spawn bricks for other vehicles.

Use a clear area. Press Escape and choose **Rule Workshop**, then an example.
The picker plants editable bricks six units east of you. Wrench a brick and
open Events. Choose an Input Event; Self is selected automatically, then choose
an Output Event. `+ IF` adds an optional condition; `X` removes it; `Copy row`
makes a neighboring editable row. Multiple conditions mean AND. With conditions,
rows expand into trigger, IF, then DO order. Delay ms is milliseconds (up to five
minutes). Send applies the edits. The classic rows
stay simple until you add a condition. Chat commands below are an alternative
to the picker, not a required setup step.

For team games, open **MiniGames → Teams & Add-Ons**. Team door and Soccer
create missing teams and put the builder on the first team; assign other players
there, or switch yourself between teams to test the gate. Conditions and `setTeam`
show your named teams; boolean checks use Yes/No. State actions label where the
variable is kept, its name and the number to set/add. Region size and object
velocity have separate labeled dimension/axis fields.

Placing an example creates a MiniGame if you do not own one, with automatic kill,
death, plant and break points set to zero. If you already own a MiniGame, it
keeps its settings: turn off automatic points when trying the scoring recipes.
Other players should join your MiniGame. Reset it after a win to play another
round and clear workshop variables. Examples belong to the host/builder.
Use a different clear area for each batch; overlapping existing bricks reject
creation. Names include the batch ID, so separate switch examples stay separate.

Save the build normally to keep the authored rules and region dimensions.
Variables and Add-On switch state are live session state, not saved game progress.
Use a copy of an existing save. Alpha save formats can change; authored rules
are saved, while live variable progress is not.

## Nine starting recipes

| Command | Starting behavior | Mutate and stress it |
|---|---|---|
| `/rulelab switch` | Classic switch opens a named panel for two seconds; reactivation extends it. No conditions. | Add sounds/lights, relays, more named targets or replace the panel. Compare authoring with ordinary v20 Events. |
| `/rulelab teamdoor` | Open and Close buttons control a solid gate, only for Blue. Fresh games create Blue/Red and put you on Blue; existing games use their first team. | Change your team in Teams & Add-Ons and verify refusal. Change the allowed team, add a delay, or require a shared `keyFound` variable. Add walls around the gate. |
| `/rulelab puzzle` | Three distinct switches in order open a gate. Any player in the MiniGame can contribute. Wrong order/repeated completed switches do not advance it; resetting the MiniGame clears progress and closes it. | Make wrong input reset progress, require simultaneous switches, add a timeout or let a ball operate one switch. Rename shared `puzzleStep` for an independent puzzle. |
| `/rulelab race` | Three ordered player/object checkpoints; three laps wins. | Reverse order, alter lap count, drive a Jeep, jump over sensors, try two racers or use a portal. Rename `checkpoint` to make a second course independent. |
| `/rulelab hill` | An uncontested occupant gains a real point each second; ten wins. | Remove the opponents check for a crowded hill, configure teams, require consecutive hold time, or make control unlock a door instead of awarding points. |
| `/rulelab slayer` | Five credited player kills wins. Environmental deaths and suicides give no point. | Change penalties/thresholds, team assignment, native damage/health/equipment actions. Try a delayed killer reward with IF Instigator Alive = Yes; die during the delay. |
| `/rulelab soccer` | Two opposing-team goal sensors and a steel-ball spawn. A credited entry into the opposing goal scores for the credited player’s team; team total five wins. Ball resets after three seconds. | First goal accepts the second team, second goal the first. Own goals reset the ball without scoring. Change the team/credit/kind checks, invent own-goal penalties, use a gravity gun, or replace a goal with a portal. |
| `/rulelab sandbox` | A three-click charged launcher shares charge between clickers; a bounce pad rewards every third visit per player; a MiniGame timer alternates colors every five seconds. | Swap velocity for damage, points or a gate. Compare shared Brick charge with private Player visits. Make the timer control the team door; change variable names to join or separate mechanisms. |
| `/rulelab addon` | Toys cycles Red → Green → Blue; core events respond to its facts. Blue increments MiniGame `blueSelections`. | Replace color responses with doors, score, launches or a win condition. Use `cycleRoute` on another brick. Disable Toys and inspect unavailable rows. Source is `packages/rule-workshop-toys/`. |

These are ordinary brick events. Delete rows, copy bricks, change names, combine
facts/actions across examples and try ideas the recipes never anticipated.
Resetting a MiniGame resets progress; it does not remove the example bricks.
Hammer unwanted examples or use the existing clear-bricks tools, and choose a
new clear area before placing another example batch.

Examples are arranged eastward in the order described. Team door places Open,
Close, Gate; Puzzle places switches 1/2/3, then Gate. Gate panels use a tall
ordinary brick when available. Checkpoints are a line; build the course around
them. Goals are sensors on colored plates; build the field and walls.

## Regions, state and objects

A region is centered on its event brick. It follows the brick's footprint
(minimum one world unit) and is four units tall by default. To resize it, open
that brick's Wrench and turn on **Custom size** under **Detection region**;
enter width, height and depth, then Send. The outline previews the selected
brick and shows saved sensors. A region is a sensor, not a wall. Player checks
use a point above the feet; object checks use the vehicle/ball center. The
recipe plates sit just above the ground, so test the outline instead of assuming
touching its outer edge counts.

`onRegionEnter/Leave/Stay` observes players; `onObjectEnter/Leave/Stay` observes
vehicles and balls spawned by this builder. Stay fires once per second, on the
server's shared clock. A fast full passage can produce Enter and Leave in one
tick. Teleports/portal jumps currently sweep that straight path too; test
intermediate checkpoint/goal firings deliberately. Driven object facts supply the driver when there is no recent touch credit.
Unattributed natural motion supplies an Object but no Instigator.

Variable actions on a brick take scope, name and integer. `Brick` means the
selected output target; Player, MiniGame, Team and Object use that input's
context. Names are case-sensitive. IF `Self` reads the source brick; IF `Target`
reads the selected target. Player means the player the fact concerns; Instigator
means the actor (the killer for a death fact). Missing context skips a condition;
use `Exists = No` when absence is intentional.

Useful property combinations: Self/Target with Variable, Color, Players in
region or Opponents in region; Player/Instigator with Score, Team, Alive,
Is Instigator or Variable; MiniGame with Round ended or Variable; Team with
Score or Variable; Object with Object kind, Speed, Alive or Variable.
Menus show the checks supported by the selected subject. A supported check can
still lack context for a particular input; Explain shows that as unavailable.
Team refers to the acting player's current team.

## When it does nothing

Send first. The brick's builder or an administrator can press **Explain saved**
to open a small results window. Choose
**Back to game**, perform the interaction, then open Explain saved again.
**Refresh** reads newer results without leaving that window. It shows the saved
row count, region dimensions/occupants, then recent condition values, skips,
actions and rejection messages. It reads saved events, not unsent edits.
Tracing is bounded and begins when asked; it cannot reconstruct old interactions.
`/ruleexplain <brick ID>` remains available in chat; `/ruleexplain off` stops traces.

Check that the input actually occurred, a region contains the relevant center,
the ball has recent credit, the context exists, the target still exists, your
MiniGame is running, and the rule builder owns that MiniGame/object spawn.
A rule with `IF MiniGame Round ended = No` stops after a win. Reset starts
another round. Changing a delay or guard does not rewrite jobs already queued.

## Questions worth recording

- Can you make a basic door as quickly as before? Where does the extra power
  first force you to think like a programmer?
- Do delayed IF checks against current state feel useful or surprising?
- Do context names, shared variable names and team totals match your intuition?
- Which combinations work naturally, and which require duplicated bookkeeping
  rows, awkward parameter entry, or an Add-On?
- Does Explain identify the reason quickly enough? Record the brick/row, recipe,
  mutation, expected result and actual result for confusing cases.

Try a timed team race with a ball-operated door and contested finish, rather
than testing each recipe only in isolation. That kind of unexpected combination
is the point of the build.
