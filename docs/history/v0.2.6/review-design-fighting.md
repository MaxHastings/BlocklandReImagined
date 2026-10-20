> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/review-design-fighting.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Review: design note, fighting lane (visibility + where to stand)

Checked against the plan's contract (no content names, one mechanism per job, no new timers or dials, something deleted for each mechanism added) and against main at ac2bdc2.

## Verdict: right shape, but three gaps to close in the note before code

**What's already right:**
- **Visibility is three data reads in the one sight path.**
- **Where to stand is one new chooser domain.**
- **The Team mover and the contest cover swap are both deleted in this PR.** The cover swap is the one I flagged; on main it's at `bots.rs:1887-1889`, applied at `:2339`.
- **Retreat has one home.** The note says no other give-ground rule exists.
- **The shot chooser gets an origin parameter, not a copy.**
- **F3 shows the terms.**

## Must answer before code

1. **How the three terms become one score.**
   - Worth is a shot score, cost is seconds, and exposure is a count of enemies times hurt. Adding them needs weights, and weights are dials.
   - `Mind::pick` also clamps every score to 0 or more (`surprise.rs` `clean`), so "worth minus cost minus exposure" goes flat at 0. The chooser then can't tell bad spots apart.
   - Proposal: put all three in one unit, the bot's expected harm traded per second.
     - **Worth:** the harm per second it deals from that spot. That's the candidate score the shot chooser already produces.
     - **Cost:** the seconds it spends getting there without a shot.
     - **Exposure:** the harm per second the enemies that see the spot would deal, times how hurt it is.
     - These multiply or divide into one non-negative number with no weight. For example, harm dealt over the time to get there and fire, divided by 1 + harm taken.
   - Whatever the form, the note must show it uses no new constant.
2. **What happens to the existing Stance moves.**
   - Stance already decides where a fighter stands, every tick (`bots.rs:2900-2985`):
     - `back_off` steps back inside the band's near edge;
     - `lean` presses in;
     - `strafe_leg` goes side to side;
     - melee `footwork` goes in, out and aside.
   - The spot options "a step in, out, left, right" do the same jobs. Two mechanisms would then move the same feet.
   - Proposal:
     - The spot is the anchor.
     - Strafe and footwork stay, as motion around the chosen spot, so it isn't a still target.
     - **`back_off` and `lean` are deleted**, since the in and out spots are their job now.
   - If the lane disagrees, the note should say why both stay.
3. **The step size and the option ids.**
   - "One walk-grid cell" is `nav::CELL` = 0.5 units (`nav.rs:41`). That's too small to change a line of sight round a wall.
   - Use a size the body already has: its width, or what it walks in one planning turn at its own speed.
   - Make options relative and stable (here, left, right, in, out, hop, jet). The chooser keeps drift, boredom and effectiveness per option id (`Mind::drive`), so ids that mean a world position would reset them every turn.

## Smaller points to fix in the note

- **Fog field.** Use `visible_distance` (where fog is complete), not `fog_distance` (where it starts). The note's wording blurs the two.
  - Read the session's live environment (`session/environment.rs`), not the map file. Environment commands change it during play.
- **Exposure ray cost.** Six spots × every enemy in sight can be dozens of rays per bot turn.
  - Count only the enemies the bot already perceives as threats.
  - Say what a ray the shared budget doesn't answer this tick counts as. Keep it deterministic: last known answer, or "not seen".
- **Conditional cover deletion.** If the spot doesn't hold the soccer "ignoring ball" share, the cover case comes out of this PR entirely and stays queued with a reason. It must not merge half-landed with the swap still beside it.
- **Plan item 8's "hurt, give ground"** has to come out of the plan. The note rightly says retreat has one home, so item 8 must not add a second.
- **Alpha.** "Alpha under 1" also catches fully transparent paint (alpha 0). That's fine, because players can't see it either. Check that it matches what the client draws as see-through.

## Fine as written

- Shots still stop on glass (`clear_path` untouched).
- Target size scales the aim points while the aim error stays an angle, so there's no new term.
- Spots come only from walkable grid cells, and the act stage's edge check still applies.
- With surprise strength 0, it picks the plain best spot.
- Tests are sanity absolutes only.

---

## Re-check of the revised note (branch claude/project-thread-un1qi5 @ 5bef428): approved, with one small fix

**Closed:**
- **One score.** Damage over seconds, divided by exposure, so it's never negative.
- **`back_off` and the strafe `lean`** are deleted. Strafe and footwork stay as motion around the chosen spot.
- **Options.** Seven stable relative options, each a body width.
- **Fog.** It uses `visible_distance` from the live session environment.
- **Exposure rays.** Only perceived threats are counted, and an unanswered ray keeps its last answer, which is deterministic.
- **Cover.** The case lands whole or stays queued.
- **Teammates' lines of fire.** They join *taken* through the same claims, so there is no second cost.

**One fix (small): the `1 + taken` hides a unit.**
- *taken* is health per second, so the 1 means "1 health per second". That's a weight in disguise: a bot facing 0.5 hp/s and one facing 30 hp/s get very different treatment for no reason the data gives.
- Make it a share instead: the share of its own health the bot would lose while getting there and firing.

      taken_share = taken × (travel + fire) / current health
      score = dealt / (travel + fire) / (1 + taken_share)

- Now the 1 is "its whole health", which has a meaning.
- "Times how hurt it is" drops out, because low current health raises the share by itself. That's one fewer term.

**One gap to say in the note:**
- A hurt bot with no shot from any spot scores every spot 0. It then stays *here*, in the open.
- Break that tie by the lowest *taken_share*. That's a tie-break, not a term, so a pinned hurt bot still steps out of sight.
