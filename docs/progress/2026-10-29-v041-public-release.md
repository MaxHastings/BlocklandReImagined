# v0.4.1 public release verification

[v0.4.1 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.1) was published October 28, 2026 at
11:59:59 PM America/Chicago (October 29 at 04:59:59 UTC). Maxwell explicitly
authorized integration and publication and confirmed he was not playtesting.
Interactive acceptance remains his; verification finished after midnight.

Source `4db7796916d8b023aa512374334973a5574ef040` reached main through
`python tools/gate.py --push --full --release-content`, with the pre-push hook
enabled. The guarded push preserved starting main `f891e5aa8`, verified the
28-commit history/protocol range, reused that exact candidate's extended pass
and updated main without force. PRs #392â€“#398 are merged; #399 had already
merged into the integration branch. The four original features are voice
policy/HUD/gain, independent tank gunner heading, human-only ordinary player
lists with bot team assignment retained, and collapse retries after temporary
physics capacity pressure. Release corrections include NPC missing voice state
and declared server companion interface resolution on join.

## Final verification

- Matching [CI run 40814738365](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40814738365)
  passed on that exact source. Linux test shards, checks, binaries and wheel
  passed; the workflow intentionally skips its Windows test job.
- `python tools/gate.py --full --release-content 4db779691` passed in 1,719 seconds:
  workspace/all-target locked build, warning-free workspace/all-target clippy,
  content startup validation, 137 Python tool contracts and 525 test binaries,
  including release-content/offscreen/native checks. No test retry was needed
  on this final run. The 18.8-minute test phase exceeded the tool's 10-minute
  duration warning; it was not omitted or relabeled as a quick pass.
- Release 32-bot/3,600-tick budgets ran on the actual saved worlds. Crazy Collage
  Madness averaged 5.26 ms per step (7.00 ms budget; p99 11.6 ms); TDM Afghan
  averaged 4.02 ms (5.50 ms budget; p99 8.2 ms). These host simulation measurements
  do not establish gameplay FPS or interactive fidelity.
- The fixed save corpus was also run explicitly with `--ignored --exact
  the_fixed_save_corpus_hosts_like_the_game --nocapture` against the warm gate
  checkout and actual content. All 23/23 saves passed in 50.20 seconds, including expected
  refusals. No missing-save/content skip supplied that coverage.
- Actual Windows ZIP: 13,426 manifest files verified; official
  `tools/package_playtest.ps1 -VerifyPackage` passed, including 164 default
  Add-Ons and 10 reference menu pictures. Packaged `--version` reports
  `v0.4.1 (4db779691)`; packaged `--check` with a fresh state passed with 21 maps,
  1,372 brick definitions, 14 modern-light maps and no Add-On health problems.
  It opened no window/audio device and reported 226 content fallbacks; zero
  Add-On health problems does not mean zero content approximations.
- Actual Linux ZIP: 13,420 manifest files verified. Its 10,148 generated Add-On
  files match the refreshed bundle byte for byte, including the NPC null guard.
- All three native build/package/startup jobs in [release build 40815232740](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40815232740) passed at the exact source. Independent verification hashed every manifest file in every actual ZIP: Windows 13,426; macOS 13,420; Linux 13,420. Each carries 10,148 generated bundle files with exact source bytes. Base packs and all 10,275 Add-On files match across platforms after following the manifest content root (inside Resources on macOS). No missing file/hash mismatch is tolerated. All public archives were downloaded again and match these tested archive hashes and the public SHA256SUMS. The remote tag resolves to the exact source and the latest-release endpoint returns v0.4.1.

## Shipping data and failures preserved

Generated Add-On bundle SHA256:
`eac4e6480d50051dad3f409ce5b32125bbb6be4d6b50058aa6b1d7c3e8af9923`
(86,498,911 bytes). 100 imported originals retain valid current importer receipts;
118 NPC WAV recordings are unchanged. The NPC policy/companion was actually
regenerated, not restamped. Main gate inputs and shipping bundle matched across
10,148 files. Base content remained current under the regeneration plan. Original
installations were read only. Earlier successful build 40810520025 used superseded
source and is not publication evidence for this release.

The earlier failed extended gate and its supported corrections remain in
[the correction receipt](2026-10-28-v041-release-gate-corrections.md). Its old
resume assertion failed then passed alone before a deterministic fixture
correction. The unchanged ragdoll soft-cone test also failed then passed alone;
that pre-existing flake is assigned in
[issue #400](https://github.com/MaxHastings/BlocklandReImagined/issues/400).
The final run passed it without a retry. No skip, weaker assertion or known
failure entry was added to conceal these observations.

The release seam audit checked all 203 declarations against the exact source:
1,922 tracked production files plus named APIs, qualified/serde aliases and
owner paths.201 mechanisms exist; Sprint-specific engine input and in-game
terrain editing remain absent. Stale integrated audio/debris labels, an incorrect
animation-library evidence cell and a broken movers table row were corrected.
The one registered seam new since v0.4.0 is package voice routing/speaker HUD;
the server companion metadata helper extends the existing companion loader.
Source availability does not imply human acceptance.

Evidence: coordinator `artifacts/v041-release/` and shared gate
`.bri-gate/logs/4db7796916d8.log`; original scoped handoffs retain their own
commands and results. Full alpha, fidelity and human acceptance remain open.

After verifying all four lane heads were ancestors of main and every resolved
target stayed under its named worktree, `tools/clean_targets.py` dry run and
`--apply` removed only those four finished target folders, freeing 214.7 GiB.
Main, coordinator, shared gate cache, unrelated worktrees, source and notes
were kept. No finished checkout with retained evidence was recursively deleted.

## Verified Actions archive digests

| Platform | Bytes | SHA256 |
|---|---:|---|
| windows | 248,775,869 | `22ecb974e73eeba84b50da275f6d531edd089f2221907929d1dd3ac12a7655b7` |
| macos | 218,352,272 | `d1d96b7ba6bca267b2988e32009ba933163cc338f0fa0b69be2b27201ded31ac` |
| linux | 245,935,811 | `afa6132b4ce7f0afa8fd43f9678205954d97775cf61de6c8521c63f6524dbc54` |

[Publish run 40820766104](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/40820766104)
succeeded, reusing corrected build 40815232740. The published release has exactly
three platform ZIPs plus SHA256SUMS, is public (not draft/prerelease), and is the
latest release. No older published tag or asset was changed.

The post-merge Windows CI run 40819266449 remains in progress at this receipt;
matching pre-merge CI, all three native release builds and the local extended
Windows release gate have passed as described above.

The documentation follow-up initially tripped the retained history guard when
replacing the candidate paragraph in STATUS with verified publication state.
The diff was checked and the commit uses the existing scoped
`Gate-Allow-Undo: docs/STATUS.md` acknowledgement for that intentional update.
The guard and pre-push hook remain enabled; no production code is reverted.
