# 2026-10-29 v0.4.2 automatic import and compatibility messages

Maxwell narrowed the task to the importer defect and clear compatibility
messages for v0.4.2, coordinated with the merge chat and intended for main.
Porting the supplied L4B2 zombie packs and Player_Roll is deferred. Custom
gameplay policy belongs in native add-on ports; missing generic capabilities
remain engine responsibilities. No original content is committed.

Isolated work: reused clean, fully merged `plastic-destruction-playtest`,
branch `codex/v042-import-compatibility` from origin/main `a8a5e5d39`.
No existing evidence or build output was deleted. Root's shared warm broker
owns validation; local Cargo/GPU checks are held while Maxwell playtests.

Cause and correction:

- Client automatic imports passed the writable installed content root as
  `--reference`, then staged output underneath it. The offline importer
  correctly forbids output inside a protected reference installation.
- Automatic import now passes only `--installed`. Existing `add_beside`
  already discovers required sibling classic ZIPs/folders recursively, so
  no parser, protection, format, or dependency mechanism changes are needed.
- Import recipe revision increases to retry unchanged ZIPs whose failure
  was cached by the older client. Native-format ZIP installation is unchanged.
- Conversion errors retain the importer's context/cause chain. Failed-row
  text no longer assumes a replacement ZIP fixes an unsupported feature.
- Existing import-report health checks now surface unported script functions
  and unsupported findings, grouped/bounded for readability. Library warnings
  show them before enabling an Add-On, without claiming the whole package
  cannot load or preventing partial conversions. Applied native ports are
  excluded from the unported count. Missing dependencies remain named.

Synthetic regression checks added: real importer CLI with in-content staging,
installed stock inheritance and transitive sibling dependencies; explicit
reference source protection; client command/recipe invalidation and full
missing-map-asset context; compatibility warnings before enabling despite
zero failed asset conversions, excluding applied port callbacks. These
fixtures contain only authored synthetic text, no community content.

`rustfmt --edition 2024` on the four changed Rust files and `git diff --check`
passed. Cargo checks, platform CI, packaged verification and main merge are
pending; this entry makes no runtime or Linux pass claim yet.

Bounded check with the existing shipped v0.4.1 Windows importer and the
corrected argument list (`INPUT FRESH_OUTPUT --json --installed CONTENT`,
without `--reference CONTENT`) passed conversion for both zombie ZIPs and
Player_Roll, retaining respectively 82, 62 and 1 unported functions and the
named Bot_Hole/Event_botSpawnLoot missing dependencies. Map_IZB reached its
actual packaging error and retained the missing-sky cause chain. This tests
the argument correction, not the newly compiled client or its UI. Raw outputs:
`C:\Users\Maxwell\AppData\Local\Temp\bri-corrected-import-args-ifguxw2r`.

Map_IZB remains a distinct unresolved asset-resolution case. It names
`~/data/skies/Sky_Blue2/resource.dml`; the original reference contains that
sky under `base/data/skies`, but the current mission/environment reader
resolves tilde against the mission mod root (`Add-Ons`). Existing v20 map
tests also require tilde paths such as `~/Map_Slate/...` to resolve there.
Do not globally rewrite tilde paths or invent sky/terrain replacements.
Original engine path/version evidence is needed before a generic correction.
The clearer failure chain retains the exact missing path. This task does
not assert that the map or the four supplied add-ons are now playable.

Merge chat: `Merge threads and cut v0.4.1`, id
`01a1857e-2792-76f3-bb89-9a45627b6e6c`; root owns serialized main integration.
The current combined private preview is independent of this pending fix.
