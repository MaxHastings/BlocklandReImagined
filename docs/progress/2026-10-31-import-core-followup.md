# Fresh imports with both recovered core scripts

Located the missing recovered `DamageTypes.cs` in the earlier private
`zombie-hole-20261023/bundle-source-inputs` evidence. Its recorded SHA-256
`ec00557bcfbaa9feea3be0260c6d8a3983f4d25f813cf7b4db03ee24127ad579` matches
the file; the companion archived `allGameScripts-Vanilla.cs` hash matches the
one used by the candidate import. This is the previous recovered-script
evidence, not a modification to the vanilla installation.

Repeated the pinned-source build with both explicit core files and the current
playtest importer. It again reached the three fresh Zombie, Shark and Slayer
imports before stopping on the unavailable pinned Tier 1 source. Their full
receipt inventories are byte-for-byte identical to the candidate imports made
with only the vanilla script. Thus the missing second core script did not alter
these three generated outputs. This removes that specific uncertainty for the
candidate; it does not constitute a complete new bundle build or prove all
gameplay compatibility. The private outputs are under
`diagnostics/v043-complete-core-imports.building`.
