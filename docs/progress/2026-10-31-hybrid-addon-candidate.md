# Hybrid Add-On candidate

Reviewed the importer/port changes since the private draft bundle's recorded
`60fcbe7f4`: changed Hole bot settings apply to Zombie/Shark, the Slayer rules
default changed, and native dependency resolution was added for imports such as
Bot_Town. The old draft's recorded reference-source hashes match the pinned
source for all 100 shipping originals. Old reports do retain documented
unsupported findings; the compatibility lock does not assert those findings are
fixed or prove every gameplay behavior.

Pinned the exact draft receipt hashes for the other 97 originals in
`packages/imported-addon-compatibility.json`, tied to the current importer
recipe. Zombie, Shark and Slayer are deliberately excluded and must pass the
normal current-generation check. Compared each selected old conversion to the
draft ZIP and its own file inventory as recorded in the preceding provenance
entry. A later recipe change invalidates the lock for review.

Built the current `bri-import-addon` in the playtest profile, then attempted
the ordinary bundle build using pinned sources and read-only vanilla content.
It regenerated the first 19 originals, including the three excluded packages,
and correctly stopped at the unavailable pinned Tier 1 archive. This attempt
explicitly supplied recovered `allGameScripts-Vanilla.cs` as its only core
script because the default `DamageTypes.cs` file is unavailable here. The
three produced outputs have current recipe receipts and their expected pinned
source hashes; their exact behavior and core-script completeness still require
the combined checks before release.

Assembled `diagnostics/v043-hybrid-candidate` from the verified prior bundle,
substituted the three current conversions and regenerated credits. The normal
`default_sources` verifier accepted all 168 default/companion entries; a
separate per-original check accepted all 100, including the three fresh
receipts and the 97 reviewed prior receipts. This is a private candidate, not
an uploaded bundle or final package. Platform builds, actual packaged startup,
health reports, game lifecycle and release checks remain outstanding; existing
unsupported brick and map findings remain explicitly open.
