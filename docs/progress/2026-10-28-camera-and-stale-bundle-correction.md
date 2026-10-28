# 2026-10-28 Camera framing and stale bundle correction

Maxwell found armed Zombies and wrong teams again in the newer VoIP handoff.
Preview 11's hidden `--version` is `9f1eba5ef`; main `995c13ffc` includes it.
The code was retained, but the external bundle reused in that handoff was old:
Slayer's generated companion lacked `hole_team`, and the native Zombie kind
omitted `weapons:false`. Source ZIP/version/author/completeness checks did not
prove freshness. Relabeling the bundle's build commit had not regenerated it.
This was the VoIP packaging handoff's mistake, not an engine revert.

Reimported both pinned originals with the shipped main importer and explicit
vanilla recovered core scripts. The read-only Steam Bot_Hole source resolves
the external spawn geometry; an initial reference-only import lacked it and
was not shipped. Final Zombie behavior JSON and expanded Slayer rules match
working preview 11 exactly after newline normalization. The separate contentfix1
folder preserves VoIP, existing explicit Add-On choices and all original folders.
Both baseline and correction startup exit zero, with the same two warnings in
the opted-in Butterfly Knife (sound and damage reference). The 13,316 correction
file hashes pass. Fresh-default validation correctly rejects an opted-in Ragdoll,
so that check is not claimed for the configured private preview.

Bundle schema 4 requires imported-generation receipts binding importer sources,
normal/build conversion dependencies, ports, workspace manifest/lockfile and
bundling code to every import/companion file. It reuses the existing content
regeneration dependency fingerprint. Missing, stale and modified imports fail
closed at packaging, rather than trusting a new executable or original ZIP hash.
Tests cover the actual old bot data, old team policy and packaging consumer.
Release refresh conservatively includes Rust/dependency changes. A full rebuild
keeps source pins strict; additional matching copies were found in the existing
read-only tier-tactical research inputs after the first bundle build rejected an
unlisted Steam Tier1 copy. No pin was weakened or old output stamped as current.

Maxwell also supplied a full-down third-person screenshot. The boom is computed
from player pitch plus v20 cameraTilt, which legitimately passes vertical. A
later camera-effects stage clamped the rendered pitch back to +/-90, turning it
away from that boom's anchor. Preserve the composed orientation in the final
effects stage; Controls still owns input limits. No camera distances, pivots or
guessed offsets change. A CPU projection regression checks the real final stage
at full down, on both sides of vertical, several headings/rolls and shortened
collision distances. Existing basis/portal/vehicle camera math remains relevant.

Focused results, source/input hashes, regeneration commands and corrected-build
receipts remain in ignored artifacts/voip-regression. The initial Python tool
suite passed 128 tests; final added consumer coverage and camera tests are
recorded in the corrective PR/handoff. Maxwell requested one new PR and merge
after verification. No interactive gameplay was automated; no GPU suite,
benchmark or soak is run while he playtests.

Focused checks now pass: 131 Python tool tests, the new camera projection regression, existing vertical basis coverage and seven portal-view math tests. Full bundle regeneration also exposed ambient source-edition conflicts in Tier+Tactical. The builder now creates a disposable reference view from the read-only vanilla installation plus the selected pinned original editions, so a dependency uses the edition being shipped. Native folder hashing preserves case-sensitive member order on Windows. Tests verify the selected dependency replaces only the disposable copy and leaves the source installation unchanged. No importer/port failure or source pin is skipped.
