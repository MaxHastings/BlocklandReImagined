# Weapon host integration checkpoint

Native weapon catalogs now load on ordinary local host/join and dedicated
startup. Content identity8 includes declared native resource bytes with bounded
reads and canonical containment. Original resource hashes are provenance, not
misrepresented as hashes of converted shapes. All17 weapon items plus4 core tools
are installed into the wrench catalog. Known native item_ui references resolve
without rewriting original source records; unknown references remain explicit.

Session advances weapon state machines/projectiles once after each authoritative
120Hz physics tick. Reliable trigger edges queue separately per connected actor;
one edge per tick preserves a quick press/release in the same packet interval.
Queues are bounded32, replay/rate-checked, cleared on equip/disconnect and stopped
after the existing movement input lease expires. Captured action aim affects
that trigger without rewriting player body orientation or accepting packet positions.
Clearing not-yet-executed edges on an immediate equip is a current cancellation
rule; exact same-tick fire/equip ordering still needs fidelity work before handoff.

Weapon sweeps use the same Rapier collision world as the player, including
native map interiors/terrain, physical brick flags and player collider tags.
Aim excludes the shooter. Projectile source collision is skipped while starting
inside that collider, then returning projectiles can hit it. This native exit
rule remains a fidelity assumption until exact v20 source-grace behavior is
established. Radius queries use closest world bounds, deterministic nearest-first
ordering and explicit overflow counts. Solid geometry occludes blast rays;
surface-start rays advance0.001 native units to avoid self-occluding the impact
face. This epsilon remains a numerical adaptation.

Protocol7 carries mounted image/state and bounded projectile/drop views in
checkpoints/deltas. Validation precedes any world mutation. Sound, effect,
animation and shell cues use the existing reliable cursor/drop-accounted channel;
late join does not replay prior cues. The client audio adapter consumes weapon
sound profiles in world space. The other three presentation consumers still
need binding. Snapshot schema4 adds weapon views; it is not a complete weapons
checkpoint for dedicated restart.

Host-only spawn loadouts preserve authored empty slots and reject unknown items,
duplicates and live reconfiguration. Default free-build still grants only
Hammer/Wrench/Printer. This provides the boundary for upcoming minigame loadouts;
there is no remote grant command.

## Evidence and limits

- Focused collision test: player hit, thin map wall, source exit/return, closest
  bounds, visible/occluded radius targets, surface impacts and reported truncation.
- Actual native Gun Session test: quick down/up, captured perpendicular aim,
  one projectile, source identity, sound and disconnect cleanup.
- Real QUIC test: host-configured weapon loadout, equip, trigger, projectile
  evolution and sound, plus another client's late-join image/projectile state.
  Initial attempt pre-populated a live actor before server startup; existing
  ownership-scope protection correctly rejected it. The test now uses a proper
  validated pre-join host loadout without weakening identity setup.
- Malformed duplicate projectiles and oversized sound profiles reject an entire
  delta before mutation. Existing building/network suites remain exercised.

Still incomplete: the client tool map/HUD and normal pickup/drop/spawner path;
original animated server muzzle positions (currently both use authoritative eye);
normal mounted/projectile rendering; effects, image/player animations, casings;
minigame damage/impulse permissions, health/death, vehicles/sports/skis/horse
transforms, brick weapon damage and synchronous projectile-event outputs.
Free-build player damage remains denied. Unsupported gameplay intentions produce
bounded named adapter-gap counts/notices, also exposed by ServerReport, rather
than being counted as completed behavior. A projectile-contact notice applies
only to brick contacts. These explicit gaps are required alpha work, not waivers.

No visible window, automated desktop/game input or audible playback was used.
