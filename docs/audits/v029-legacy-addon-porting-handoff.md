# v0.2.9 handoff: make legacy Add-On ports easier to build and trust

Prepared 2026-10-10 from the P90 and Rallypack port on the released v0.2.8
runtime. Requested by Maxwell for Claude's next-version work. These are
recommendations, not changes already implemented or a release commitment.
Check current main before implementing: some older audits and STATUS.md still
describe earlier releases. The authoritative baseline here is installed
v0.2.8 and the evidence linked below.

## Copyable brief for Claude

Please turn the P90/Rallypack port lessons into reusable v0.2.9 import tooling,
clear diagnostics and generic engine seams. Read this handoff, AGENTS.md,
the platform principles, seam ownership table and linked progress evidence.
First compare the release-based port branch with current main. Preserve its
working recipes and behavior while integrating improvements; do not replace
unrelated main changes with the old release tree.

Start with honest import coverage, material/texture correctness, dependency
resolution, deterministic packaging and a focused validation command. Next,
give package-owned weapons a truthful bot/ammo capability interface and make
deferred weapon-state updates understandable. Address advanced combat/audio
seams only after an implementation spike proves an existing operation cannot
express the behavior. Keep gameplay policy in packages, Torque conversion
outside runtime, and original content outside Git. Do not build a TorqueScript
VM or add Rallypack-name checks to engine code. Maxwell owns every interactive
playtest. Deliver concrete acceptance evidence and a short user test card.

## What already works

The existing foundations did substantial work. This was not a case of needing
to write a custom model converter for each gun:

- The released importer converted the DTS models to native shapes and copied
  icons, textures and sounds. Weapon datablocks, images, particles, debris and
  effects were broadly available.
- Native package rules could implement firing, spare magazines, partial-mag
  retention, chambered rounds, aiming, cancellation, recoil, reloads and damage.
- Shared assets and server-only companion rules could be packaged separately.
  The host runs authoritative rules; clients receive the required shared data.
- Native distribution already downloaded changed/missing packages, checked
  hashes, cached them and replicated late-join equipment. The updated P90
  passed a real loopback clean-cache test. No new download protocol was needed.
- Basic P90 bot firing and automatic reload worked on unchanged v0.2.8 after
  restoring projectile metadata and describing hold/tap trigger use.

The result is a playable port of 45 Rallypack weapons plus three ammo items,
with the standalone P90 also installed. It is not proof of exact fidelity for
every weapon. The remaining friction is mostly conversion quality, reusable
policy ports, author feedback and richer semantics for advanced behavior.

## Recommended order

| Priority | Deliverable | Why it matters |
|---|---|---|
| First | Honest coverage report and correct model/material import | A successful import must not disguise an invisible gun or unported behavior. |
| First | Reusable framework recipes and dependency closure | A new weapon using the same framework should reuse one port. |
| First | One validation/package/install workflow | Authors should not manually repair hashes, manifests and load lists. |
| Next | Explicit package weapon/bot/ammo semantics | Bots should select, aim and reload script-owned weapons using actual mechanics. |
| Next | Clear state-callback timing and loop diagnostics | Prevent dry-fire/reload loops and eliminate timing guesswork. |
| Next | Add-On health, installation and download UX | Hosts and joining clients should understand what is enabled and what is happening. |
| Later | Advanced ballistic, fuse, sound and attachment primitives | Improve fidelity when verified gaps require an engine extension. |

This order favors reliable reuse across many Add-Ons over completing every
Rallypack special feature in one release.

## 1. Import coverage should describe playable behavior

**Observed:** P90 initially reported eight converted assets, zero asset
failures and seven converted datablocks, but lacked its framework projectile,
effects/reload sounds and twelve behavior callbacks. Rallypack itself reported
297 converted/copied assets with zero failures, alongside hundreds of behavior
gaps. Those are useful accounting numbers, not a playable-weapon verdict.

**Improve:** Generate a human summary and machine-readable coverage manifest
per selectable item. Separate geometry/materials, icon/audio/effects,
dependencies, firing, ammo/reload, modes, damage and bot support. Use explicit
states such as preserved, ported, approximate, unsupported and missing source.
Attach source locations and the applied recipe/override to each result. Group
repeated framework callbacks rather than making an author sift through hundreds
of nearly identical entries. Preserve the detailed raw report for investigation.

Do not infer missing engine hooks merely because Torque functions are unported.
An unsupported source callback may need ordinary native policy, a known recipe,
or a genuine engine seam; the report should distinguish those conclusions.

**Acceptance:** P90 imported alone identifies the missing Rallypack source and
which features depend on it. With its native framework port applied, the
summary lists supported behavior and remaining approximations. Every retained
asset is accounted for as referenced, intentionally unused or unresolved.

## 2. Fix conversion fidelity in the shared importer

**Observed:** The P90's geometry existed, but its dark body appeared missing
because eight gun materials, its magazine and shell were marked additive.
Black additive surfaces contribute no light. Some missing texture references
became clear placeholders, although their images existed in the vanilla source.
The recipe repaired solid surfaces and resolved textures; startup checks alone
had not caught the visual error.

**Improve:** Audit DTS material flag interpretation using small synthetic
fixtures and representative privately supplied models. The current weapon
surface override is a workaround, not a reason to force every model opaque:
preserve legitimate translucent/additive effects. Provide deterministic texture
resolution across the Add-On, declared framework and designated vanilla source,
with provenance and clear ambiguity reports. Distinguish an intentionally
shapeless projectile from a missing model.

Provide an offscreen preview/contact sheet for selected item/image states:
held gun, reload/aim variants, shell and relevant effects. A valid shape file
does not establish visible surfaces, correct scale or correct mount transforms.
Keep manual overrides separate from generated output and regenerate hashes
automatically after applying them.

**Acceptance:** The original P90 body renders without the per-weapon opaque
repair; additive effects still render correctly; missing/ambiguous textures are
reported. Maxwell compares the preview and game appearance with the original.

## 3. Reuse framework ports instead of bespoke scripts per weapon

**Observed:** P90 delegates policy and assets to Weapon_Rallypack. Its ZIP alone
was insufficient. We first built a self-contained P90 subset, then generated
207 image configurations for the full pack from inherited source fields.
The scripts already share magazine, recoil, reload and mode policy. However,
the recipes remain dedicated Python tools with several source-name decisions.

**Improve:** Integrate the useful parts into the existing port catalog and
framework-dependency mechanism. Resolve inheritance once into a typed conversion
representation, retain source provenance, and apply versioned framework recipes.
Represent reusable policy parameters explicitly: ammo identity, magazine size,
spare magazine collection, chamber rule, reload/aim image chains, projectile,
pellets, recoil parameters and sound references. Keep framework-specific
recognition in tooling and package policy.

Compute a dependency closure for a requested subset so a single weapon retains
all required models, textures, sounds, particles and rules without shipping an
entire framework accidentally. Treat source enable/execute structure as evidence:
the importer found a disabled debug item that we had to remove along with its
physics binding. Do not execute arbitrary TorqueScript to discover it.

**Acceptance:** Import two different weapons sharing Rallypack through one
registered policy port; adding the second needs data/overrides rather than a
new full script. Report framework source requirements and resolved native
companions. Repeated generation is deterministic, and unsupported code remains
visible. A completely new framework can still require one new policy port;
promise reuse, not universal automatic TorqueScript translation.

## 4. Give authors one build/check/package/install path

**Observed:** We manually repaired damage-type namespacing, stale debug-item
physics, presentation evidence, model/presentation hashes and unsupported
wrapper player models. Avatar discovery inspected archetype files even after
their manifest provides were removed. Testing both ports enabled required an
independent content snapshot; junctions pointing outside the root were rejected.
Rhai failures also appeared only when firing, not during an idle load check.

**Improve:** Compose existing tools into one version-targeted workflow. Proposed
commands should inspect/import, apply a recipe and separate overrides, validate,
package and optionally install. Name the target release/runtime API explicitly.
Generate manifests, companion relationships, all affected hashes and a concise
build receipt from the same dependency graph. Detect global identity conflicts,
unreferenced implementation-only assets and mismatched declarations/discovery.
Make loader discovery honor declared assets consistently where appropriate.

Provide an isolated enabled-package validation overlay with separate selection
and state files, without relaxing content-root containment. Supply a bounded
headless exercise harness for a weapon's mount, fire, release, reload, death and
respawn, and clean-client transfer. Keep this focused; no exhaustive gameplay
automation or mandatory full-workspace build for each asset-only change.

**Acceptance:** A clean recipe run creates installable folders/ZIP and a report,
checks with the target released runtime and leaves the source installation and
live enabled choices unchanged. Conflicts/errors name the package, file, id and
likely repair. It catches callback runtime failures, not only parse failures.

## 5. Add explicit semantics for package-owned weapons and bots

**Observed:** Clearing image.projectile made scripted guns unknown to bots.
The top-level image command replaces native launch, so retaining the actual
projectile reference restored flight/range/blast information without duplicate
shots. A helper derives hold/tap from the authored transition graph. A real bot
then fired the P90, hit a target and completed an empty-magazine reload. This
does not expose the package's ammo state to the native inventory planner. Native
weapons may take priority in mixed loadouts. Variable-charge bow and cooked M67
grenade are outside this basic support.

**Improve:** Design a generic, bounded capability provider for package attacks
and their current availability. It should describe the actual shot delivery,
speed/range, direct/splash harm, trigger/charge behavior, ammo availability and
reload/mode controls. Support truthful dynamic values where a charge or mode
changes the attack. Bind the description to the active item/image/state and
invalidate it when those change. Reuse the existing bot planner and execute
ordinary equip/trigger/reload commands; do not inject bot scripts or grant bots
free ammo. The package remains authoritative for damage and ammo policy.

A simple constant-shot declaration should stay easy. Complex or unknown effects
must remain explicitly unsupported rather than being guessed from weapon names
or command strings. Check the existing combat capability and objective provider
architecture before introducing a parallel descriptor system.

**Acceptance:** In a mixed native/custom loadout, a bot evaluates the custom
weapon's real availability, fires through normal controls, reloads when useful
and switches after exhausting its usable ammo. A charge/fuse variant is either
truthfully described and validated or clearly unsupported. No duplicate shots,
hidden replenishment or changed human weapon behavior.

## 6. Make weapon callback timing and state limits clear

**Observed:** Torque callbacks changed state synchronously; Rhai operations
commit after the weapon step. Ammo checks needed a minimum tick before their
following transition. Ready's unconditional no-ammo transition caused repeated
dry/reload activity after all ammunition was exhausted; removing that idle
transition fixed Maxwell's reported rapid glitching. A large combined ammo map
also needed splitting into per-ammo banks to stay within bounded state values.
Unsupported math calls, reserved properties and iterator assumptions caused
avoidable script iterations.

**Improve:** Document the exact update phase and visibility of image/ammo/mount
operations, including callbacks that can run in one step. Add diagnostics for
zero-time cycles and repeated failed reload attempts; where supported, a
declarative ammo predicate or atomic weapon transition may remove timing hacks.
First prove whether existing state mechanisms suffice before adding a new API.
Keep resource limits, but have tooling estimate per-value/state-key budgets and
point to the oversized field. Publish runnable Rhai examples and the supported
math/query surface for the released runtime.

**Acceptance:** An empty gun with no reserve settles quietly after release;
cancelled reload cannot refill later; checking ammo does not rely on an
undocumented one-tick workaround. Failures explain update timing or budget
limits. Include humans and bots in the same focused control-path checks.

## 7. Improve host and joining-client feedback

**Observed:** Maxwell repeatedly needed reassurance about which folder to use,
what enabling the Add-On does, whether friends need its files and whether an
engine update is required. The transfer machinery works; an internet join has
not been established by our loopback checks. Missing presentation can fall back
without establishing original fidelity. Two unrelated Butterfly Knife warnings
also appeared during checks of the Rallypack packages.

**Improve:** Make Add-Ons show target runtime compatibility, companion rules,
dependencies, meaningful coverage and package-specific health. Explain that
importing source assets may still require a behavior port. Install/update from
a native ZIP without manual four-folder placement; preserve enabled choices,
validate first, apply atomically and state when restart is required.

Joining UI should identify required downloads, progress, verification/cache
status and actionable errors. Tell users whether a problem is host reachability,
package transfer, compatibility or content loading. Explain that Add-On transfer
does not update the game executable. Add an actionable server-side preflight
for referenced shared assets and accidental server-only presentation. Follow
the existing trust tiers and fallback/join contract; do not introduce new
prompts for ordinary data or make missing cosmetic assets block a join.

**Acceptance:** A host installs/enables one entry and understands its companions.
A clean client joins, receives required shared assets and sees the equipped
model/icon/effects. A changed package refreshes by content hash. A broken asset
has an attributed warning. Maxwell and a friend perform the actual external
internet acceptance, separately from deterministic local transfer tests.

## 8. Advanced fidelity: investigate before adding engine seams

The playtest port approximates or omits these original mechanics. This is a
backlog of behavior gaps, not proof that every entry needs engine code:

| Area | Candidate reusable improvement or investigation |
|---|---|
| Scripted stepped bullets, penetration, ricochets | Compare existing hitscan/projectile/query APIs; add generic bounded impact continuation/material response only for proven missing semantics. |
| Suppression/near misses | Investigate attributed near-miss queries/events rather than scanning every player from every weapon tick. |
| Cooked grenades and programmable airbursts | Investigate authoritative per-projectile fuse/remaining lifetime and detonation controls, with late-join presentation derived from state. |
| Blast exposure/falloff/stun | Identify which policies existing damage hooks can express and which require bounded engine queries. |
| Shot loops, tails and distant sound | Investigate positional per-listener cues, owned loop handles and reliable stop/cancel lifecycle; current distant cues are approximate. |
| Shield/temporary wrapper models | Improve documented attachment slots/transforms, multi-image examples and previews; our native shield mount is approximate and needs visual acceptance. |
| Bipods, LMG heat/barrels, sniper/revolver modes, armour/preferences | Implement reusable package policy first; expose a generic operation only when existing primitives cannot support the intended rule. |

For every new seam: record its owner in architecture/seams.md, provide a small
mechanism test plus a representative package acceptance check, and expose its
supported semantics to bots where relevant. Do not implement these as engine
branches keyed on Rallypack, P90 or another content name.

## Things worth knowing at the start

- Target the user's actual installed release, not newer main by accident.
  A package that uses a new engine hook requires the matching game update;
  downloading the package cannot install that hook into an old executable.
- Keep the original game/ZIP read-only; discover framework dependencies early.
- Asset conversion, faithful presentation, behavior support, bot support and
  internet reachability are separate acceptance questions.
- Worn images use slots 2 and 3 in this release; slot 1 is reserved. Document
  this and mount/source coordinate conventions in one discoverable example.
- Top-level image.command replaces native firing; commands.states.onfire can
  run alongside it. Explain this distinction prominently to prevent missing
  or double shots and missing bot metadata.
- Startup validation, idle script execution and firing-time script execution
  cover different failures. Supply the smallest relevant checks together.
- Keep generated content separate from overrides, and register reusable recipes
  so later ports do not rediscover the same fixes.
- Avoid treating stale summary docs as the release inventory; update the release
  index and link the current evidence and known approximations.

## Evidence and integration pointers

Port branch `codex/rallypack-p90-v028` is based on v0.2.8 (`b6500335e`). Its
working sequence ends at `79a5d8fb` before this documentation handoff:
`1175c263` P90 recipe/tests, `3ea2bce8` material correction, `6dd5ebcc` full
pack/empty-ammo correction, `79a5d8fb` basic bot descriptions and acceptance.
Inspect/cherry-pick the relevant changes onto current main; do not reset main
to this release branch. Original/generated game content is not in those commits.

- [P90 port evidence](../progress/2026-10-10-p90-native-v028-playtest.md)
- [Material failure and correction](../progress/2026-10-10-p90-material-fix.md)
- [Full pack, limitations and empty-ammo fix](../progress/2026-10-10-rallypack-v028-native-weapons.md)
- [Bot baseline failure and updated acceptance](../progress/2026-10-10-rallypack-v028-bots.md)
- Source recipes: `tools/build_rallypack_p90.py`, `tools/build_rallypack.py`,
  `tools/rallypack_bot_use.py`, `tools/rallypack_p90/`, `tools/rallypack/`.
- Private-content checks: `crates/net/tests/rallypack_p90.rs` and `rallypack.rs`.
- Final private output: `E:\Downloads\Rallypack-native-v028-bot-support` and
  `E:\Downloads\BRI-P90-native-v028-bot-support`.
- Reusable bundle: `E:\Downloads\Rallypack-Weapons-native-v028.zip`.

Minimum v0.2.9 outcome: a correct visible import with an honest support report,
one reusable framework port, one coherent validation/install path, and clearly
declared bot support. Complete simulation of every legacy framework is a longer
term goal; keep that distinction visible to authors and players.
