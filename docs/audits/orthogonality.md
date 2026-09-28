# Orthogonality and consistency audit

Date: 2026-09-28. Code at `47dcf2a` (main). Read-only audit, requested by Max
after two shadow bugs ("the player's shadow shows on the roof and on the floor
below"; "the horse has no shadow") turned out to be shortcuts standing in for
the real mechanism. Those two are being fixed in the shadow thread and are not
repeated here.

The question: where does the game fail to follow one rule from first
principles, so that a player would say "X does this but Y doesn't, and they're
the same kind of thing", or simply "that doesn't make sense"?

Nine areas were read crate by crate and screen by screen: rendering; physics,
collision and movement; input and controls; UI and UX; networking, replication
and saves; gameplay rules; content, assets and modding; cross-crate units,
types and constants; and the client's modes and life cycle. Every finding
below was confirmed by reading the code, with file and line at `47dcf2a`. The
strongest were re-checked a second time. Nothing was run against real content
(the content packs are not in the audit container), so "what the player sees"
is inferred from the code unless it says otherwise.

Severity: **5** a player will very likely hit it in a normal session;
**4** likely in the first playtest or first Add-On use; **3** noticeable in
ordinary play; **2** an edge case or a mismatch a careful player notices;
**1** latent, no player symptom today. Fix size: **S** a few files, under a
day; **M** a subsystem; **L** a redesign of one concept.

## The general rules

Almost every finding breaks one of these. They are the "one rule per concept"
that should replace each special case.

1. **One gate per kind of permission.** Whether an actor may affect a thing
   (trust, ownership, admin, LAN, minigame, tutorial, alive) is decided by one
   function per action class, called by every path that performs the action.
2. **One recipe per life-cycle transition.** Hosting, joining, changing map,
   saving, autosaving and crashing each have one code path, and every later
   transition reuses the first one's recipe instead of rebuilding part of it.
3. **One owner per piece of state; everything else derives.** Admin role,
   eye height, abilities, the avatar, sitting, the brick bar: one holder, and
   every view is recomputed from it, never copied once on entry.
4. **Base content is just the first provider.** Built-in, imported and
   Add-On content go through the same loader, id grammar, validation and
   failure policy.
5. **One query per physical question.** "Is this in water", "is this
   resting on the map", "does this ray hit", "does this body fit here" each
   have one function used by every system.
6. **One clock, one tick rate, one unit system.** Game time drives every
   in-world visual; ticks, gravity, brick units and axes are named constants.
7. **One setting, one range, one default, one apply step.** Wherever a
   setting is written (Options, console, wheel, file), the same clamp and the
   same apply function run.
8. **One way to tell the player something went wrong.** A single reject or
   notice helper with one rule for popup vs status line vs transient print.
9. **Dialogs share one behaviour.** Escape, the close button, Enter,
   double-click, the key that opened it, and which game keys stay live are
   decided once for all dialogs.
10. **Failures are contained where they happen.** One broken Add-On or one
   failing subsystem affects only itself, and the player sees it where they
   can act on it.
11. **Every twin gets a test.** Singleplayer and hosted, host and joiner,
   base and Add-On, first map and changed map, LAN and internet.

## Ranked findings, worst first

### 1. Imported Add-On weapons disconnect every player (sev 5, S)
Add-On import mints weapon ids as `ns:weapon/name`
(`crates/addon-import/src/lib.rs:1063`). The server hands them out with no
grammar check (`crates/sim/src/session/inventory.rs:151-154`), but
`ToolInventory::validate` requires `v20.weapon.` and forbids `:` and `/`
(`crates/sim/src/session/inventory.rs:26-37`). Every client validates every
player's replicated inventory with it (`crates/net/src/replica.rs:415-426`,
again at `crates/client/src/building.rs:461`) and a failure disconnects
(`crates/client/src/app.rs:2780-2794`). Separately, the minigame catalog
rejects non-`v20.` ids (`crates/minigames/src/model.rs:74-79`) and silently
falls back to five vanilla items (`crates/sim/src/session/combat.rs:205-215`),
contradicting its own "never silently falls back" comment
(`crates/minigames/src/model.rs:146-147`). Other validators accept yet other
spellings (`crates/vehicles/src/schema.rs:282-285`,
`crates/client/src/weapon_effects.rs:130-138`). The hosted Add-On test drives
only `Session`, never replication (`crates/addon-import/tests/hosted.rs`).
**Player sees:** one person picks up the imported shotgun and everyone drops
with an inventory error; minigame loadouts shrink to five items once any
weapon Add-On is on.
**Rule:** one content-id validator (`bri_package::id`), used everywhere; no
prefix checks.

### 2. Base game packages are treated as Add-Ons with broken code (sev 5, S)
`ClientCode::load` checks every non-server entry in `packages.json`, base role
packages included (`crates/client/src/client_code.rs:49-67`).
`AddOnCode::load` fails with `client.missing` when there is no `package.json`
(`crates/client-sandbox/src/addon.rs:81`, `:306`), and base packages have none
(`crates/package/src/library.rs:115`). The package runtime skips both cases
(`crates/package-runtime/src/package.rs:351-366`); this loader doesn't. The
messages go to chat on game entry (`crates/client/src/app.rs:3546-3549`). Its
test only feeds a set holding the one sample (`client_code.rs:289-301`).
**Player sees:** a chat line "Add-On v20-… has code that cannot run" for each
base package, every time they enter a game.
**Rule:** one "which entries are Add-Ons" filter (no role, has a manifest)
shared by every loader; a missing manifest means "no code".

### 3. Effects, grass and rain skip the colour correction the world got (sev 5, S)
The game window uses a non-sRGB swapchain (`crates/client/src/platform.rs:205-211`).
The world shader re-encodes for it via `OUTPUT_ENCODED`
(`crates/render/src/scene.rs:1314-1318`, `scene.wgsl:1-5`). Particles, foliage
and weather sample their art as sRGB (decoding it to linear) and write that
straight to the same target (`crates/fx-runtime/src/gpu.rs:137` +
`particles.wgsl`, `crates/foliage/src/gpu.rs:255` + `foliage.wgsl`,
`crates/weather/src/gpu.rs:117` + `weather.wgsl`; the formats are passed in at
`crates/client/src/app.rs:4921-4939`). Their tests render into sRGB targets,
the one case that works (`crates/client/tests/foliage_scene.rs:48`,
`crates/weather/examples/offscreen_weather.rs:106`).
**Player sees:** smoke, explosions, sparks, grass and rain look darker and
harsher than the original art.
**Rule:** every pass that writes the frame uses one output-encoding
convention; tests render to the game's real swapchain format.

### 4. Four ways to save a world, and only the manual one is right (sev 4, S–M)
The manual Save un-hides bricks that are mid-respawn after an explosion
(`crates/sim/src/session.rs:1196-1209`). The autosave, the save after a host
error and the final report save the raw world
(`crates/net/src/server.rs:775`, `:909`, `:936`, kept by
`crates/client/src/network.rs:170-171`). Package state (economy, removed
voxels) is saved only on a clean stop (`network.rs:160-168`), never by the
autosave or crash save (`server.rs:37`, `:775`, `:909`). Change Map consumes
the old session with no save at all (`server.rs:779-783`), though hosting
promises to keep the world it ends with (`app.rs:1791`). Package-generated
voxels are saved as authored bricks into the base map's autosave folder
(`crates/sim/src/session/packages.rs:447-495`, `crates/client/src/saves.rs:235-268`).
**Player sees:** bricks blown up just before an autosave or a crash come back
invisible and non-solid for good; after a crash a game mode's progress and its
world disagree; up to a minute of building is lost on Change Map; Slate's save
list fills with Stress Lab voxel worlds.
**Rule:** one `Session::save_snapshot()` (world with respawns normalised plus
package state), used by every save path including before Change Map.

### 5. Change Map is a half-rebuild (sev 4, M)
The client replaces its building controller on `MapChanged`
(`crates/client/src/app.rs:2759`) but the brick bar, paint index, save
context and admin flag are only sent once, on first entry
(`app.rs:3019-3043`, `:2963`, `:2986`; `crates/client/src/building.rs:335-337`,
`:403-409`). The server rebuilds the session without packages
(`app.rs:1794-1797` vs `:1776-1790`), and `Session::adopt` doesn't carry them
(`crates/sim/src/session/map_change.rs:42-110`). Tutorial movement limits are
cached on the client and never cleared (`app.rs:2636-2637`, `:3566`;
`crates/sim/src/session/tutorial.rs:356-383`). The palette is grouped one way
on entry and another way after any change (`app.rs:3001-3015` vs `:2839-2855`).
**Player sees:** after an admin changes map, the brick bar still shows ten
bricks but every slot says "Empty or invalid brick slot"; new bricks come out
in colour 0 while the HUD shows another colour; Save shows the old map's name
and picture; a game mode's HUD and keys vanish and its progress is never
saved; leaving the Tutorial by Change Map can leave jump or jet predicted off
and rubber-banding.
**Rule:** one setup recipe for first map and every later map; everything sent
on entry is re-sent when its owner is replaced.

### 6. Seven different "may I touch your stuff" rules (sev 4, M)
Hammer, wand, wrench, printer and paint use trust levels with admin and LAN
inside (`crates/sim/src/session/tools.rs:600-616`,
`crates/world/src/authority.rs:37-52`). Weapon effects on bricks (key doors,
direct hits) check only `owner == source || owner == 0`
(`crates/sim/src/session/weapons.rs:227-230`). Explosions check LAN or exact
owner, with no public bricks, trust or admin
(`crates/sim/src/session/events.rs:461-477`). Pumpkin carving checks owner,
public or admin, no trust or LAN (`crates/sim/src/session/special.rs:283-290`).
Hammer-flipping a vehicle checks owner or admin (`tools.rs:656-665`) while
click-flipping uses build trust (`crates/sim/src/session/vehicles.rs:389-406`).
The wand on a player uses the minigame damage rule but shows the trust message
(`tools.rs:475-488`, `crates/sim/src/session/combat.rs:374-397`). Dialogs open
at build trust but the edit is checked at full trust
(`tools.rs:534`, `:858-861` vs `authority.rs:214-227`).
**Player sees:** a fully trusted friend can hammer and paint your bricks but
not open your key door or carve your pumpkin; can click-flip your jeep but not
hammer-flip it; rockets destroy your bricks by radius but not by direct hit;
you can never wand a friend who trusts you, and the message blames trust;
events can be edited in the dialog and then fail on Apply with a generic
"Brick edit denied".
**Rule:** one `may_affect(actor, owner, action)` built on the actor's trust,
with each action's v20 `$TrustLevel` in one table.

### 7. Minigame "Enable Building" is never enforced; being alive is checked per command (sev 4, S)
Paint and wand call `can_build` (`crates/sim/src/session/tools.rs:337`, `:370`);
planting never does (`crates/sim/src/session.rs:1232-1265`). Alive is checked
by some commands (`session.rs:1072`, `:1182`, `:1192`, `:1279`) and not by
Plant, DropTool or tool actions (`session.rs:1034`,
`crates/sim/src/session/items.rs:35`, `tools.rs:834`). The wand's minigame
and tutorial gates apply to `/wand` only, not to the wand item a loadout or
spawner hands out (`tools.rs:361-376` vs `:451`,
`crates/sim/src/session/inventory.rs:69`).
**Player sees:** bricks can still be placed in a minigame with building off; a
dead player can plant and drop items; a minigame with the wand disabled still
wand-deletes if the loadout includes one.
**Rule:** one action gate before dispatch covering alive, minigame build
rules and tutorial; gates are checked when the action happens, not when the
tool is requested.

### 8. Enabling one Add-On can switch off every Add-On, silently (sev 4, M)
`package.json` has three readers with different strictness
(`crates/package-runtime/src/manifest.rs:20-41` strict with no `client` field;
`crates/package/src/library.rs:52-75` lenient;
`crates/client-sandbox/src/addon.rs:32-39` lenient with a `client` field), and
`MANIFEST_FILE` is defined three times. The client-code sample has a `client`
section, the strict reader rejects it, and `Catalog::load` is all-or-nothing
(`crates/package-runtime/src/package.rs:333-340`,
`crates/client/src/packages.rs:31-36`). A bad weapon, vehicle or brick file
instead stops the game from starting (`crates/client/src/content.rs:302`,
`:317-322`, `:519-521`, `:896-900`). None of it reaches the Add-Ons screen,
which only shows library problems (`crates/client/src/add_ons.rs:351-367`);
runtime problems go to stderr (`app.rs:888-904`).
**Player sees:** turning on the documented sample (`docs/architecture/client-sandbox.md:447-449`)
makes every other Add-On's HUD, rules and modes disappear with no message; a
different broken Add-On stops the game from starting; the Add-Ons screen shows
it as fine.
**Rule:** one manifest type with optional sections; a failing Add-On is
disabled alone and its problem shows on its own row.

### 9. Projectiles obey "Colliding"; everything else obeys "Ray Casting" (sev 4, S)
Weapon sweeps and explosion line of sight exclude only non-colliding bricks
(`crates/sim/src/simulation.rs:78-82`, `:356-363`;
`crates/sim/src/weapon_query.rs:64-70`, `:130-136`). Tools, clicks and bot
sight check `brick.raycast` (`simulation.rs:633`,
`crates/client/src/building.rs:316`).
**Player sees:** a brick with Ray Casting off still stops bullets; one with
Colliding off lets bullets through although clicks still hit it. In v20, Ray
Casting is what projectiles and rays test.
**Rule:** one "does a ray or projectile hit this brick" predicate; Colliding
governs bodies only.

### 10. The horse exists twice (sev 4, L)
As a player datablock (`crates/motor/src/player_types.rs:167-190`) and as a
vehicle mount derived from the vehicle pack
(`crates/vehicles/src/world.rs:333-380`), with its box repeated in
`crates/vehicles-import/src/lib.rs:207`. They disagree on eye height (2.4 vs
2.16), acceleration, knockback mass (`crates/sim/src/session/combat.rs:27`
vs `world.rs:765-768`), impact damage and its minigame gate
(`combat.rs:1087-1112` vs `world.rs:1473-1478`), whether brick touch events
fire (the mount throws its touches away, `world.rs:1647-1653`), whether it can
be ridden (`PlayerType::rideable` is never called), and energy (tracked twice,
`world.rs:1067-1081`). This is the same shape as the mount-shadow bug.
**Player sees:** riding a horse over a touch brick or teledoor does nothing;
the horse handles and hurts differently depending on how it was spawned.
**Rule:** one horse definition; "a thing you ride" is one concept.

### 11. Your own movement ignores other players and vehicles until corrected (sev 4, M)
Client prediction's collision mirror has map, bricks and terrain only
(`crates/sim/src/prediction.rs:89-118`, `:139-199`). The server's motor
collides with every actor and pushes players apart
(`crates/motor/src/torque.rs:136-147`, `crates/motor/src/player.rs:896-921`).
**Player sees:** walking into another player or a parked vehicle, you pass
into it and then snap back.
**Rule:** prediction collides with the server's body set, using remote bodies
at their latest poses.

### 12. Add-On bricks load three ways (sev 4, M)
The host uses `Definitions::load_with(.., brick_extras)`
(`crates/client/src/content.rs:376`); joiners and every map change use
`prepare_map` with `Definitions::load` and no extras (`app.rs:127`). The brick
selector lists package bricks (`content.rs:519-521`) but the selectable
catalog holds stock bricks only (`app.rs:1610`, `:1906`, `:2518`;
`building.rs:178-183`, `:1016-1020`).
**Player sees:** imported bricks show in the selector but can't be picked;
joiners are disconnected with "has no native render mesh" when the host loads
a save that uses them; the host hits the same after Change Map.
**Rule:** one content resolver builds definitions, meshes and catalog for
host, join and map change (general rule 4).

### 13. Add-Ons can't do what built-in content does (sev 4, L)
Only weapons, vehicles and bricks merge from Add-Ons
(`crates/net/src/content_identity.rs:31-54`). Sounds are packaged but never
played (`crates/addon-import/src/lib.rs:1971`; `crates/client/src/audio.rs:346-362`),
music, particles, emitters, player types, maps, events, weather, foliage and
UI come from one base role only (`lib.rs:1981-2023`,
`crates/client/src/content.rs:31-46`, `:275-292`, `:631-650`). Add-On items
that use a base image are dropped because import writes `v20:image/<n>` while
the base pack keys are `v20.image.<n>` (`lib.rs:1046-1054`,
`crates/weapons/src/merge.rs:98-108`). Content is found by a fixed file path,
ignoring the manifest's `provides` and `side`
(`content_identity.rs:31-44`), and the runtime and library disagree on which
side weapons belong to (`crates/package-runtime/src/content.rs:65-73` vs
`crates/package/src/library.rs:43-46`).
**Player sees:** an imported gun fires silently with no explosion effects;
Add-On music and maps never appear; some Add-On items quietly vanish.
**Rule:** every content kind loads through one provider list keyed by
`provides`; the base game is the first provider.

### 14. Join passwords are offered in three places and refused in all three (sev 4, M or S to hide)
Start Game has a password field (`crates/ui/src/screens/menus.rs:489`),
password servers open a password prompt (`menus.rs:500-521`), and Admin has a
Join password slot (`crates/ui/src/screens/admin.rs:622-626`). All three are
refused with three different messages (`crates/client/src/app.rs:1574-1577`,
`:1891-1894`, `crates/sim/src/session/admin.rs:350-358`).
**Player sees:** they type a password, press Start or Join, and get "Request
Rejected".
**Rule:** never show a control whose action is always refused. *Design choice
for Max: wire up passwords, or hide all three.*

### 15. One failing system silently stops the rest of the game tick (sev 3, M)
`Session::step` chains about 15 systems with `?`
(`crates/sim/src/session.rs:1348-1481`); the server only counts the error and
prints to stderr (`crates/net/src/server.rs:866`). A concrete trigger:
hammering or wanding an indestructible brick fires its break events first and
then errors (`crates/sim/src/session/tools.rs:420-428`, `:465-472` →
`crates/sim/src/simulation.rs:338-342`), while explosions and packages skip
such bricks quietly (`crates/sim/src/session/events.rs:452`,
`crates/sim/src/session/packages.rs:803`).
**Player sees:** the brick's break events fire but the brick stays; that
tick's items, combat and events are skipped; a recurring error early in the
chain freezes the game clock for everyone while the server looks alive.
**Rule:** check before acting; each system contains its own failures and
reports them.

### 16. Game speed (`/timescale`) slows only some things (sev 3, S)
Only player motion and `animation_time` are scaled
(`crates/client/src/app.rs:3536-3542`, `:3576`). Arm animations, foliage,
weapon and actor effects, explosions, shells, debris, brick emitters and
weather use wall time (`app.rs:3914`, `:4036`, `:4113`, `:4123`, `:4131`,
`:4153`, `:4173`, `:4178`, `:4183`), while the server scales everything
(`server.rs:863`).
**Player sees:** in slow motion, people walk slowly but hammer swings,
explosions, debris and rain run at full speed while projectiles crawl.
**Rule:** one game clock drives every in-world visual; only UI uses wall time.

### 17. Three rules for "which water am I in" (sev 3, S–M)
The player motor picks the deepest water with at least 10% coverage
(`crates/motor/src/player.rs:697-706`); splash cues pick the first with any
(`crates/sim/src/session.rs:59-70`); vehicles pick the first whose footprint
contains their centre, map liquids first (`crates/vehicles/src/world.rs:1048`,
`:1141-1147`, `crates/content/src/water.rs:75-77`,
`crates/sim/src/simulation.rs:472-478`).
**Player sees:** a boat in a water-brick pool above map water doesn't float; a
player grazing water gets a splash but no swimming.
**Rule:** one submersion query used by every system.

### 18. Planting and chain-kill disagree on "resting on the ground" (sev 3, S)
Plant support casts rays 0.205 down from stud cells
(`crates/sim/src/simulation.rs:774-802`); chain-kill grounding uses 0.1
tolerance over every cell plus exact terrain (`simulation.rs:808-836`).
**Player sees:** a brick planted on a floor slightly off the plate grid is
accepted, but then the hammer refuses to break its neighbours and the wand
chain-kills it.
**Rule:** one "rests on map" function.

### 19. Getting out of a vehicle tests a smaller body than yours (sev 3, S)
Exits are checked with a 0.6-wide, 1.7-tall capsule centred on the exit
point (`crates/vehicles/src/world.rs:676-705`); the player is then placed with
their feet there (`crates/sim/src/session/vehicles.rs:1094-1098`) and is 1.25
wide and 2.65 tall (`crates/motor/src/player.rs:191-202`). Scale and datablock
are ignored.
**Player sees:** getting out under a low ceiling or beside a wall can leave you
stuck inside it.
**Rule:** clearance uses the occupant's own motor shape at their feet.
Torque's `Player::checkDismountPoint` does exactly this with the player's
object box.

### 20. Chat box open: the view freezes and the cursor stays hidden (sev 3, S)
Mouse look requires no dialogs open (`crates/ui/src/ui.rs:1590-1611`), the
chat input is a modal with no cursor (`crates/ui/src/screens/menus.rs:549-553`),
and the pointer stays grabbed whenever the cursor is hidden
(`crates/client/src/platform.rs:466`). v20 keeps the view live while typing.
**Player sees:** pressing T freezes the camera with no cursor.
**Rule:** if the pointer is captured, it drives the camera; otherwise it is
released.

### 21. Dialogs each decide their own keys (sev 3, S)
F2 opens the Player List but doesn't close it (`ui.rs:732`,
`crates/ui/src/screens/players.rs:197-203`), unlike the console or brick
selector. Wrench and print dialogs mark Left Shift as the only blocked key,
but being modal blocks all movement first, so that special case never runs
(`crates/ui/src/screens/wrench.rs:310`, `:1047`,
`crates/ui/src/screens/selector.rs:582`, `ui.rs:1795-1799` vs `:1825`).
Escape and Close save in Options but discard in Player Appearance, and
Appearance's Done closes Options without committing it
(`crates/ui/src/screens/options.rs:1177`,
`crates/ui/src/screens/avatar.rs:736-741`, `:787-788`). Escape during a pending
request is blocked on some screens and not others (`wrench.rs:288`,
`crates/ui/src/screens/saveload.rs:386` vs `selector.rs:356-359`). The trust
invite can't be dismissed and a second invite overwrites the first, unlike
minigame invites (`crates/ui/src/screens/trust.rs:58-79`, `ui.rs:1410-1419`).
Double-clicking a game mode does nothing because it waits for an event lists
never send (`crates/ui/src/screens/modes.rs:197` vs
`crates/ui/src/view.rs:1738-1744`); Add-Ons handles it correctly
(`crates/ui/src/screens/addons.rs:413`). There are two confirm-dialog systems
that have already drifted (`menus.rs:784-890` vs `admin.rs:648-676`, `:842`,
`:877`).
**Player sees:** F2 twice doesn't close the list; you can't walk with the
wrench open; Escape keeps changes in one editor and throws them away in the
next; double-click works in Add-Ons but not Game Mode; a trust invite can't be
closed.
**Rule:** one dialog behaviour: Escape and Close mean the same everywhere, the
opening key toggles, one "activate row" event, one confirm path, and which
game keys stay live is declared per dialog in one place.

### 22. Every screen reports a refusal its own way (sev 3, M)
Popups titled "X Rejected" on some screens (`avatar.rs:790`, `selector.rs:426`,
`:685`, `wrench.rs:420`, `:1228`), other titles elsewhere (`saveload.rs:502`,
`addons.rs:390`, `menus.rs:605`), a status line in minigames and admin
(`crates/ui/src/screens/minigames.rs:198`, `ui.rs:1228-1231`), the log in
the console, a bottom print or popup as the fallback (`ui.rs:1245-1258`).
Validation errors split the same way (`saveload.rs:322`, `menus.rs:526`,
`options.rs:1052` vs `admin.rs:690-702`, `minigames.rs:246`), and an empty
admin password is silently ignored (`admin.rs:901-908`). The Player List
swallows minigame invite failures into a status it never shows
(`players.rs:191-195`). In-game notices use three channels with different
durations (`app.rs:2606-2614`, `:2678-2690`, `platform.rs:841`). Developer
words reach players: "Native Server Credentials", "host acknowledgement",
"before the alpha handoff" (`admin.rs:127`, `:224`, `:596`,
`menus.rs:122-123`, `minigames.rs:171`). Join and disconnect messages differ
by when they happen (`crates/net/src/client.rs:223`, `:246`, `:375`, `:427`,
`crates/net/src/server.rs:587`), and the LAN list hides servers of another
version with no reason while direct join explains it
(`crates/net/src/discovery.rs:132` vs `server.rs:476-483`).
**Rule:** one reject/notice helper with one rule for popup, status line or
print; one disconnect-reason type with player text.

### 23. Settings: several ranges, defaults and apply steps per setting (sev 3, S)
Mouse sensitivity is 0–10 in the console and 0.02–2 in Options, and opening
Options and changing anything silently clamps a console value to 2
(`crates/ui/src/screens/console.rs:240`, `options.rs:576-579`, `:1087-1093`).
Chat lines are 1–64, 4–100 or unclamped depending on where they're set
(`console.rs:250`, `options.rs:1101-1103`, `ui.rs:905-906`), and console
changes only refresh part of the game (`console.rs:77-81` vs
`options.rs:1097-1107`). Zoom FOV falls back to 45 in one place and 10 in two
others (`ui.rs:840`, `app.rs:3557`, `crates/client/src/controls.rs:339`), and
wheel changes aren't saved (`ui.rs:848-849`). Super-shift toggle falls back to
true in two places and false in one (`ui.rs:626`, `:805`, `options.rs:923`).
The avatar lives in two stores and console edits to one are ignored
(`ui.rs:921-924`, `avatar.rs:775-784`, `app.rs:1554`). Audio builds its prefs
with no pack defaults (`crates/client/src/audio.rs:116`). "Fast 1st/3rd person
switch" is listed as honoured but does nothing (`options.rs:112`,
`controls.rs:109`).
**Rule:** one prefs registry: one default, one range, clamped where read, and
one apply function called by every writer.

### 24. The admin role is kept in three copies that drift (sev 3, S)
`bri_admin::Role`, `Actor.administrator`, and the minigame `PlayerState.admin`
(`crates/admin/src/lib.rs:26`, `crates/world/src/authority.rs:32`,
`crates/minigames/src/model.rs:207`). Login updates only the actor
(`crates/sim/src/session/admin.rs:397-407`); the minigame copy is written at
join and map change only. The UI's flag is copied on entry
(`app.rs:2963`, `crates/ui/src/ui.rs:480-486`) while the admin window reads a
live snapshot (`ui.rs:278`).
**Player sees:** after logging in as admin, Load Bricks still says it needs
admin; a de-admined player keeps it. (The minigame copy only feeds an
instant-respawn bypass that the session's own 120-tick minimum overrides.)
**Rule:** one role, every view derived from it.

### 25. Two hosts, built separately (sev 3, M–L)
The dedicated server and the in-game host assemble sessions and content
separately (`crates/net/src/dedicated.rs:67-152`, `bri-server.rs:61` vs
`crates/client/src/app.rs:183-212`). The dedicated one has no Change Map, no
packages, no admin passwords, no LAN mode, no Tutorial lessons, a copied
autosave interval, and a different save format with a 512 MB limit where the
client's is 63 MB (`crates/world/src/persistence.rs:8,59`,
`crates/world/src/build.rs:9`). The Add-On hosting test covers only the
dedicated path (`crates/addon-import/tests/hosted.rs:38-44`).
**Player sees:** a dedicated server lacks features the in-game host has; a
large dedicated world can't be loaded in the game.
**Rule:** one host setup, one save format, one set of limits.

### 26. The player cap is 32, 64 or 1024 depending on where you look (sev 3, S)
Start Game offers 1–32 (`crates/ui/src/screens/menus.rs:75`, `:259`, `:468`),
the host accepts 1–64 (`app.rs:1580`), Admin accepts 1–1024
(`admin.rs:1018-1025`, `crates/admin/src/lib.rs:12`). 64 is a bare literal in
about 15 places (`crates/sim/src/session.rs:591`, `:758`,
`crates/net/src/server.rs:410`, `discovery.rs:66-67`, `protocol.rs:60-61`,
`replica.rs:79-420`). Bots take a session slot but not a network slot
(`server.rs:800`, `crates/sim/src/session/bots.rs:81-90`).
**Player sees:** a server with bots says "Server is full" while the list shows
free slots.
**Rule:** one limit and one default. *Design choice for Max: do bots take a
player slot?*

### 27. See-through things are drawn in the wrong order (sev 3, M)
Water and glass are blended without writing depth
(`crates/render/src/scene.rs:1302`), and foliage, effects and weather draw
after the whole world pass (`app.rs:5479` vs `:5516-5519`).
**Player sees:** grass under a lake or smoke behind a glass brick draws on top
of the water or glass at full strength.
**Rule:** all see-through things are sorted together.

### 28. Brick FX go wrong on falling debris (sev 3, S)
Brick vertices store their FX centre in their own scene space
(`crates/render/src/scene.rs:436-446`); debris is built at the origin and
moved by an instance transform (`crates/client/src/brick_debris.rs:536-561`)
that the FX math never applies (`scene.wgsl:174`, `:187`).
**Player sees:** a chrome or pearl brick flashes white or dark the moment it
breaks; Undulo debris wobbles by distance from the world origin.
**Rule:** FX data goes through the same model matrix as positions.

### 29. Vehicle respawn time is modelled twice, and the tested one is unused (sev 3, S)
The sim uses the brick owner's minigame with a 1 s floor
(`crates/sim/src/session/vehicles.rs:255-267`); the minigames crate's
`respawn_delay(Vehicle)` and `wheeled_destroy_respawn_delay`
(`crates/minigames/src/policy.rs:331-358`), tested in
`crates/minigames/tests/rules.rs`, are never called.
**Rule:** one rule, and the test covers the one the game uses.

### 30. The core tools and the Tutorial map are identified by position (sev 3, S)
`CORE_TOOLS[1]` means wrench and `[2]` printer in several crates
(`crates/sim/src/session/tools.rs:837-841`, `tutorial.rs:28-29`,
`crates/client/src/building.rs:98-100`, `:483-486`,
`crates/net/src/content_identity.rs:514-520`); the Tutorial map is
`LOADABLE_MAPS[13]` in one place and a literal in two others
(`crates/client/src/content.rs:1023`, `crates/sim/src/tutorial.rs:12`,
`app.rs:4685`).
**Rule:** identity by name or capability, never by index.

### 31. Slash commands are three tables; v20 has one rule (sev 2, M)
A hard-coded list (`app.rs:4653-4666`), admin commands
(`crates/client/src/admin_ui.rs:187-203`), `/invite` alone (`app.rs:4620`);
Add-On commands only via a bare letter key (`ui.rs:1809-1820`). The admin
crate defines drop-at-camera and return-to-previous that the session rejects
as not implemented (`crates/admin/src/lib.rs:923-924`,
`crates/sim/src/session/admin.rs:532-534`), while a separate working drop path
exists (`session.rs:1112`).
**Player sees:** `/magicwand`, `/spy`, `/ret` say "Unknown command"; Add-On
commands can't be typed.
**Rule:** one router: any `/name` goes to the host, permission checked there.
*Design choice for Max: which v20 commands to support.*

### 32. First spawn and respawn are different paths (sev 2, M)
Joining uses the first working map spawn (`server.rs:813`,
`crates/sim/src/session.rs:583-700`); respawn uses `pick_spawn` with bot home,
checkpoint, minigame and spawn bricks, plus the spawn effect
(`combat.rs:955-1060`). Join and resume are near-duplicate bodies, and
`adopt` is a third copy that has drifted (`session.rs:653-692`, `:808-847`,
`map_change.rs:84-107`).
**Player sees:** a returning player with spawn bricks first appears at the map
spawn, with no spawn effect.
**Rule:** one spawn choice; one peer constructor.

### 33. "Sitting" is a one-off cue, not state (sev 2, S)
Sit is sent as an emote cue (`session.rs:1087`) and each client guesses and
clears it locally (`app.rs:487-491`, `:1477-1482`), unlike the player light
or vehicle burning, which are replicated state.
**Player sees:** someone who joins late sees seated players standing.
**Rule:** lasting states are replicated; cues are for one-off events.

### 34. Package explosions and weapon explosions follow different rules (sev 2, M)
Weapon explosions check the minigame, use weapon falloff, hit vehicles and
fake-kill bricks that respawn (`crates/weapons/src/runtime.rs:1645`,
`events.rs:400-494`). Package `explode` hurts everyone regardless of
minigame, removes bricks for good with no ownership check, and ignores
vehicles (`crates/sim/src/session/packages.rs:839-930`). Package entities
can't be shot, blasted or wanded (`weapon_query.rs:22-34`, `tools.rs:776-779`)
and are the only thing with a kill plane (`packages.rs:36`, `:1190`).
**Rule:** one explosion mechanism and one actor classification.

### 35. Fog is three different things (sev 2, S)
World surfaces fade their colour toward the fog on a curve (`scene.wgsl:233-240`);
foliage fades its opacity on a line (`foliage.wgsl:15`); particles and rain
ignore fog.
**Player sees:** distant explosions and rain stay bright in fog; grass fades at
a different rate than the ground.
**Rule:** one shared fog function.

### 36. Brick debris and Add-On brick models ignore the Brick Shadows setting (sev 2, S)
With Brick Shadows off, planted bricks don't cast (`app.rs:5446-5450`) but
debris and package brick models always do (`app.rs:5463-5464`).
**Player sees:** a brick suddenly gains a shadow when it breaks.
**Rule:** anything drawn as a brick follows the Brick Shadows setting.

### 37. Shadows and FX: moving bricks cast unmoved shadows (sev 2, S)
Undulo and Water FX move vertices up to 0.2 (`scene.wgsl:172-183`); the shadow
pass draws the unmoved shape (`shadow.wgsl:11`).
**Rule:** both passes share one vertex-movement function.

### 38. "Unlit" means two things (sev 2, S)
Unlit map props take player shadows and point lights
(`crates/render/src/scene_loader.rs:457-458`, `scene.wgsl:374`); unlit items
don't (`crates/client/src/items.rs:226-231`, `scene.wgsl:351`).
**Rule:** one Unlit material kind.

### 39. Eye height has three sources (sev 2, M)
The server snaps between stand and crouch eye
(`crates/motor/src/player.rs:128-136`) and fires from there
(`crates/sim/src/session/weapons.rs:174`); the client blends over 0.2 s along
the authored keys (`crates/client/src/crouch.rs:21`, `:70-82`); the vehicle
horse uses 0.9 × height.
**Player sees:** shots while crouching or standing up leave from a different
height than the camera.
**Rule:** eye height lives in motor state; the client presents it.

### 40. Terrain blocks some rays and not others (sev 2, S; no terrain on the playtest map)
Tool and bot rays see only streamed terrain tiles
(`crates/sim/src/simulation.rs:571-587`, `crates/sim/src/map.rs:320-321`);
weapons always use the exact terrain ray (`weapon_query.rs:59-61`).
Click-flipping a vehicle ignores map walls (`vehicles.rs:705-716`). Projectiles
can hit the shooter's own vehicle while tool rays skip it
(`weapon_query.rs:84-100` vs `tools.rs:771-779`).
**Rule:** one ray query that merges terrain, and one exclusion rule for self
and own vehicle.

## Further findings by area

Lower severity, or smaller instances of a rule above. All confirmed in code.

### Rendering
- Model detail level is chosen three ways and never by distance
  (`crates/client/src/avatar.rs:77-82`, `scene_loader.rs:427-430`,
  `crates/client/src/items.rs:145-152`). Sev 1–2, S.
- Sun, ambient and clear-colour defaults are defined three times with
  different values (`scene.rs:289-293`, `:680-682`, `scene_loader.rs:230-238`).
  Sev 1, S.
- Graphics pref names and defaults are duplicated between UI and client
  (`options.rs:22-27`, `:41`, `:105-109`; `crates/client/src/graphics.rs:8-11`;
  anisotropy 7/15 vs 8, `scene.rs:969`). Sev 1, S.
- Only brick chunks are culled off-screen; items, vehicles, debris and terrain
  never are (`scene.rs:1815-1862`). Performance only. Sev 1, M.
- Horse meshes keep their GPU buffers across a renderer reset while avatars
  are cleared (`app.rs:4894-4990`, `:5426`, `avatar.rs:859-873`). Possible
  crash after a GPU reset; not traced. Sev 2, S.

### Physics and units
- Gravity: the physics world keeps the library default 9.81 and each system
  patches around it; 20 is repeated in four crates
  (`crates/physics/src/lib.rs:10-14`, `crates/vehicles/src/world.rs:1839-1843`,
  `crates/motor/src/player.rs:221`, `crates/weapons/src/runtime.rs:904`,
  `crates/client/src/brick_debris.rs:31`). Any new body falls at half speed.
  Sev 2, S.
- The 120 Hz tick is defined in about 30 places; Torque's 32 ms tick converts
  with ×3.84 in the motor and ×3.75 truncated in vehicles, so a jumpDelay-3
  horse hops every 92 ms instead of 96 (`crates/motor/src/player.rs:23-25`,
  `crates/vehicles/src/world.rs:367`). Time units mix ms, s and µs. Sev 2–3, M.
- Brick units: `STUD`/`PLATE` exist but 0.5/0.2/0.25/0.1 are hand-written in
  sim and client, which must agree by hand (`crates/content/src/brick.rs:5-6`,
  `crates/sim/src/grid.rs`, `simulation.rs:733`, `:790`, `:816`,
  `building.rs:808`, `:1064-1097`). Sev 2, S.
- The Torque Z-up to Y-up swap is hand-written about 18 times, including in
  runtime crates (`scene_loader.rs:234`, `crates/events/src/migration.rs:139`,
  inverse in `crates/client/src/tool_ui.rs:766`). All agree today. Sev 1, S.
- Energy recharges per second for players and in 32 ms quanta for vehicles
  (`player.rs:666-667`, `world.rs:1067-1081`). Sev 1, S.
- The third-person camera exists twice; the tested one is unused
  (`player.rs:992-1015` vs `building.rs:696-778`). Sev 1, S.
- Vehicle poses can resurrect a removed vehicle from a late datagram; player
  poses are guarded (`crates/net/src/replica.rs:302-313` vs `:336-342`).
  Sev 1, S.
- Knockback mass and impact damage are standard-armor constants, while health
  is per datablock (`combat.rs:24-27` vs `player_types.rs`). Max health comes
  from four sources (`combat.rs:16`, `:248`, `:734`, `:978`). Sev 1, S.

### Input
- Keyboard turning shares the mouse's "Look" action, so FOV scaling and the
  vehicle invert flip apply to keys too (`controls.rs:98-130`,
  `ui.rs:550-570`). Sev 2, S–M.
- Holding chat PageUp/PageDown scrolls once; v20 repeats (`ui.rs:718-719`,
  `:1791`, `:1834-1870`). Sev 2, S.
- Add-On HUD keys stop working while Shift or Alt is held, unlike normal
  binds (`ui.rs:1810-1811`, `app.rs:466-468` vs
  `crates/ui/src/binds.rs:113-121`). Sev 2, S.
- A Fire release in some states (dead, observer, gunner seat) never reaches
  the tool code, so the next click can be lost (`app.rs:4203-4259`,
  `building.rs:926-937`). Sev 2, S.
- "Can gameplay take this input?" is decided five ways (`ui.rs:1590-1600`,
  `:1624`, `:1680-1681`, `:1791-1799`). Sev 1, S.
- Key names use three formats: "CTRL Z", bare "Z", "Ctrl+Z"
  (`binds.rs:131-147`, `ui.rs:455-461`, `crates/ui/src/input.rs:136-181`).
  Sev 1, S.
- Mouse axis bindings are stored but ignored; the wheel works only for
  inventory and `scrollInventory` can't be rebound to a key
  (`binds.rs:141-142`, `ui.rs:1682-1688`). Sev 1, S.
- Wheel steps are accumulated twice (`platform.rs:1519-1527`,
  `ui.rs:1662-1670`); text focus and zoom-held are each tracked twice
  (`ui.rs:1116-1121` vs `platform.rs:1354-1363`; `ui.rs:660-663` vs
  controls). Sev 1, S.
- First run is detected by "no saved key bindings", so a settings file with
  bindings but no favourites never gets the stock favourites
  (`ui.rs:896-898`, `:917-920`). Sev 1, S.

### UI
- The unsaved-changes guard covers menu Quit, Disconnect and closing the
  window but not the console `quit`, and Change Map and Clear All don't
  mention unsaved work (`crates/ui/src/screens/console.rs:99`,
  `admin.rs:975`, `:990-1000`). Sev 2, S.
- Native windows are built three ways with different skins and button widths;
  helpers `named` and `scroll` are copied into five screens
  (`addons.rs:25-44`, `modes.rs:19-77`, `admin.rs:29-41`, `:210`). Sev 2, M.
- Dead code: three confirm callbacks are never created, and one of them saves
  settings where the live path doesn't (`ui.rs:129-141`, `menus.rs:797-800`
  vs `options.rs:1393-1455`). Sev 1, S.
- Minigame settings have two schemas in different units (ms with lives vs
  seconds without), and the default loadout is written three times
  (`crates/minigames/src/model.rs:41-154`, `crates/ui/src/api.rs:905-919`,
  `crates/client/src/minigame_ui.rs:24-73`). Sev 2, M.
- Brick group names are built three ways; the admin list says "Former player
  7" where centre prints give the name (`tools.rs:618-628`,
  `crates/sim/src/session/trust.rs:412-418`, `admin.rs:572-592`). Sev 2, S.

### Networking and saves
- A remote admin loading a large build over a home connection times out after
  a fixed 10 s; the host never does (`crates/client/src/network.rs:315`,
  `crates/net/src/codec.rs:83-90`). Sev 3, M.
- After Change Map, the LAN list shows the old map name while direct probes
  show a raw map path (`server.rs:181-197`, `:784`, `discovery.rs:105-106`).
  Sev 3, S.
- Admin Server Options are modelled but never applied; the host hard-codes
  port and chat length (`crates/admin/src/lib.rs:313-366`,
  `admin.rs:545-546`, `session.rs:1321`, `app.rs:1757`). The screen is hidden
  today (`admin_ui.rs:81`). Sev 2, M.
- Damaged or unreadable private files are handled three ways; a permissions
  error on a package save is treated as "no save" and then overwritten; the
  host certificate key is stored unprotected while the identity key uses
  DPAPI (`server.rs:77-89`, `app.rs:1776-1778`,
  `crates/identity/src/lib.rs:143-146`). Sev 2, S.
- Three crash-safe writers bypass `bri_files` without fsync
  (`crates/client-sandbox/src/trust.rs:159-165`,
  `crates/package-runtime/src/state.rs:157-163`,
  `crates/package/src/library.rs:824-842`). Sev 2, S.
- Player identity has about seven representations; reconnect exists as
  resume tickets (test-only; the client always sends none) and as owner
  reclaim (`server.rs:600-660`, `app.rs:1849-1856`,
  `session.rs:596-602`). Sev 1–2, M.
- `docs/networking.md` says protocol 35 and 20 Hz snapshots; code says 34
  and 40 Hz poses. Sev 1, S.

### Gameplay
- "Harmful" event outputs are a lowercase string list in the session rather
  than a flag in the typed catalog; SetVelocity, AddVelocity, Dismount and
  ClearBurn aren't gated, so another player's brick can fling or dismount you
  outside a minigame (`events.rs:543-558`, `:995-1011`,
  `crates/events/src/catalog.rs:428-457`). v20 parity not checked. Sev 2, S.
- Admin Clear All and Clear Group bypass `kill_brick`, leaving respawn timers
  and undo stacks pointing at deleted bricks; Ctrl+Z silently does nothing
  for several presses (`admin.rs:450`, `:482` vs
  `crates/sim/src/session/tutorial.rs:552-555`). Sev 2, S.
- Hammer and wrench hit sounds have two paths, one dead
  (`crates/client/src/audio.rs:218-219` vs `tools.rs:731-739`). Sev 1, S.
- Checkpoint bricks swallow bot touches while teledoors fire them
  (`crates/sim/src/session/special.rs:115-117` vs `:134-136`). Sev 1, S.
- The sim reads the wall clock directly for bans, and uses one shared random
  stream for spawns, events and pumpkin faces, so carving pumpkins changes
  which spawn you get (`session.rs:393`, `:472`, `:982-985`,
  `combat.rs:1018-1021`, `special.rs:293-297`). Sev 2, S.

### Content and modding
- Special bricks (checkpoint, teledoor, spawn, treasure, pumpkin) are
  recognised by exact v20 id, while water is a data property, so an Add-On can
  make a water brick but not a checkpoint (`crates/sim/src/definitions.rs:175-189`,
  `combat.rs:30`, `special.rs:7-12`). Sev 3, S–M.
- Seven different TorqueScript readers parse the same datablocks; the vehicle
  reader drops inheritance, so a vanilla vehicle and the same vehicle via
  Import Add-On can convert differently (`crates/convert/src/tscript.rs` vs
  `crates/vehicles-import/src/main.rs:35-37` and five others). Sev 3, L.
- Duplicate ids abort the load for bricks but "first wins" for weapons and
  vehicles, and the in-game host discards the vehicle notes the dedicated
  server keeps (`definitions.rs:125-128`, `crates/weapons/src/merge.rs:36-43`,
  `content.rs:322`). Sev 3, S.
- Hand-written JSON is strict for manifest, HUD and model but lenient for
  weapons, vehicles and bricks, so a typo in `weapons.json` is silently
  ignored (`crates/weapons/src/lib.rs:303`, `crates/vehicles/src/schema.rs:9`,
  `crates/content/src/brick.rs:9`). Sev 3, S.
- Capabilities are described in two places; client-code capabilities never
  show on the Add-Ons screen; the trust prompt promises "take this back any
  time on the Add-Ons screen" but revoke is test-only
  (`crates/client/src/add_ons.rs:43-49`, `crates/package/src/capability.rs`,
  `crates/client-sandbox/src/trust.rs:220-275`). Sev 3, S.
- The modding guide contradicts the code: ids 1–64 vs 32 characters; two
  reserved names vs fifteen; "coming soon" for weapons, HUD panels and the
  Import button, all of which exist; four content kinds missing from the table
  (`docs/modding/README.md`, `crates/package/src/id.rs:12-63`). Sev 3, S.
- Missing assets fall back differently by source: a base brick without an icon
  is fatal, an Add-On one gets none (`content.rs:498-502` vs `:892-911`).
  Sev 2, S.
- The package path-safety rule is written five times with different length
  limits (160, 240, 256 characters). Sev 2, S.
- UI colours are 0–255 in the base UI and 0–1 floats in Add-On HUDs
  (`crates/ui/src/schema.rs:22-23`, `crates/package-runtime/src/content.rs:94`).
  Sev 2, S.
- Players still see "add-on" and "package.json" in problem text; the imported
  folder name and package id are normalised by different rules (64 vs 32
  characters) (`library.rs:240`, `:259`, `:345-366`,
  `crates/addon-import/src/lib.rs:45-68`). Sev 1, S.

### Tests and tooling
- Tests cover one twin: singleplayer hosting in about ten files, internet
  once, LAN and Tutorial never; Change Map without packages, saves or HUD
  checks; the Stress Lab harness uses internet trust while the product uses
  LAN trust (`crates/client/tests/multiplayer.rs:190`, `:335`,
  `crates/stresslab/src/lib.rs:137-150`). Sev 2, M.
- Workspace feature unification turns on real audio output in the audio
  crate's tests, compiling out the no-device test
  (`crates/audio/Cargo.toml:11-12`, `crates/client/Cargo.toml:32`). Sev 2, S.
- Tests read the content folder from three different environment variables
  (`BRI_CONTENT`, `BRI_CONTENT_ROOT`, `BRI_STRESSLAB_CONTENT`). Sev 1, S.
- `TrustLevel` names two unrelated things (brick trust and Add-On trust), and
  brick trust has two encodings. Sev 1, S.
- `docs/KNOWN-ISSUES.md:20` says Tutorial triggers aren't implemented; they
  are. Sev 1, S.

## Choices for Max

These would change something a player notices by design rather than fix a
bug, so they wait for Max:

- Join passwords (14): wire them up, or hide the three fields.
- Player cap (26): 32 or 64, and whether bots take a slot.
- Slash commands (31): which v20 commands to add (`/magicwand`, `/spy`, `/ret`).
- "Fast 1st/3rd person switch" (23): add v20's smooth camera transition, or
  hide the checkbox.
- Harmful event outputs: whether SetVelocity, AddVelocity and Dismount from
  someone else's brick need a minigame, as damage does.
- Particle and rain fog (35): v20 behaviour not yet checked.

## Suggested order

Fixes follow severity, grouped so one change applies one rule: content ids
and the Add-On loader filter (1, 2, 8); output encoding (3); the save snapshot
(4); the Change Map recipe (5, 12, 24); the permission and action gates (6,
7, 9); tick containment (15); then the rest.
