# Add-On sounds defined by another Add-On

Max's v0.1.12 log (/mnt/project-files/v0.1.12-findings.md) lists sounds
that "play silently". The ones checked, Frog's WWII `FWReload1sound`
(defined by Frog's, checked by importing the real zips) and Tier 2A or
skins `pistolfireSound` and `magazineOutSound` (defined by Tier 1's
server.cs), are profiles that a sibling Add-On defines. v20 has one global
datablock namespace. The importer namespaces the profiles a pack defines
(`ns:sound/name`) and leaves a name it can't find bare, and nothing
resolved a bare name to another pack's profile.

Fix (crates/weapons/src/merge.rs `resolve_shared_sounds`): after every
pack is merged, a bare sound name in an Add-On image state, state cue,
projectile or explosion that no sound is keyed by takes the Add-On profile
of that name, with the referrer's own namespace first. Guard:
`merge::tests::a_sound_another_add_on_defines_resolves_to_it`.

Not done (budget):
- Effects that show nothing (advHugeBulletFireEmitter, tierFragPortBounce,
  hegrenadePortBounce) and the missing $DamageType names. Check whether
  each is defined by an Add-On that isn't bundled (MWB defines advHuge*)
  before treating it as a resolution bug.
- Icons icon_fillcan and classicpistolakimbo.
- Sounds that no enabled Add-On defines stay silent, as in v20.
