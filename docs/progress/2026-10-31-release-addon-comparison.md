# Release Add-On conversion comparison

Compared each receipt inventory in the interrupted fresh bundle build
`diagnostics/v043-preview2-addon-bundle.building/addons` with the corresponding
prior conversion in `diagnostics/v043-reuse-bundle/addons`. Nineteen originals
had both inventories. Fifteen were byte-identical; Zombie, Shark, Slayer and
the HE grenade differed. The grenade difference was its import report. Slayer
also differed in host rules and metadata; its fresh version was already used for
the Afghan leave/rejoin checks.

The difference in Zombie's `assets/bots.json` adds
`cooperative_stack: true`; Shark's corresponding fresh output explicitly sets it
false. Correction: the runtime's `BotKind` default is true, so the previous
Zombie conversion already enables this behavior; the previous Shark conversion
instead inherits true where its fresh source-derived setting is false. The
fresh Slayer rules also contain the requested changed reset policy. These are
reasons to assess individual packages rather than ship the entire old bundle
unchanged. The full fresh build stopped at the missing pinned Tier 1
archive, so this sample does not establish the state of later Add-Ons or prove
that a hybrid bundle is release-ready. Before release, select current verified
outputs wherever possible, retain honest per-package recipe and source
provenance, and validate dependencies and the actual final package. The source
pin and old receipts must not be rewritten to imply a new import.

Comparison used the SHA-256 inventories in the two `import-generation.json`
files and a JSON diff of the two changed bot definitions. These are private
generated artifacts, not committed original content. Both comparison inputs were
produced before the latest integration commit; a final release must validate its
own exact inputs and importer recipe.
