# 2026-10-03 v0.2.2 branch retirement audit

Read-only inventory before v0.2.2 integration, against main `6ba067ef0280b74fdf1c70ecc0edfe2f4f958aed`. Live `git ls-remote --heads origin` returned 16 GitHub heads; every tip matched the local remote-tracking ref. No fetch, merge, branch deletion or push occurred. `gh pr list --state all` showed no open PRs; #20 was merged. Counts below are commits only on main / only on the branch, not estimates of missing functionality.

| Exact remote ref | Verified tip | Main-only/branch-only | Disposition |
|---|---|---:|---|
| `origin/claude/addon-icons-bounces` | `b5c0c31a09e1bb0ab1d3c874f85f9bb8e9c10cee` | 56/0 | Ancestor; retire ref |
| `origin/claude/addon-toggles-bmcl21` | `35d06e29e4ed341b06c455694aca0dd141815b33` | 64/0 | Ancestor; retire ref |
| `origin/claude/advanced-duplicator-xemi1d` | `24b5b1c93f70cde142dfc3b061782e68a5a6245f` | 99/0 | Ancestor; retire ref |
| `origin/claude/code-health-audit-eui8bq` | `45f4e7c5f6505de275f00044e0912c498fa52da0` | 92/0 | Ancestor; retire ref |
| `origin/claude/fog-matches-sky-ewgfuh` | `5c8b9111034822e3c60f3482c96b30f2939eaa1e` | 99/0 | Ancestor; retire ref |
| `origin/claude/project-thread-9cbbpg` | `aca6605d1520e29c2c8b95a46609d391c29e9bcc` | 99/0 | Ancestor; retire ref |
| `origin/claude/project-thread-fqw7d2` | `82b76f31cc5e30239149c9431a9c6c156895afeb` | 99/0 | Ancestor; retire ref |
| `origin/claude/project-thread-t8k5dx` | `5d655877dd7f0dbe85df2ec99435c27912dc9b95` | 99/0 | Ancestor; retire ref |
| `origin/claude/shared-addon-sounds` | `6fad9cb894d24796b7f7eae04406cc5318a00b04` | 98/0 | Ancestor; retire ref |
| `origin/claude/silent-brick-swap` | `1218323b02442d31cf94e33dd8c15edef6d58f07` | 99/0 | Ancestor; retire ref |
| `origin/claude/tier-jump-delay` | `9e5b5e135566b08bdf5ad5d52d639aff5460d3b2` | 99/0 | Ancestor; retire ref |
| `origin/claude/zip-only-windows-egtjjr` | `f0ffcdbe9d469253e72ab288d74d81fa598d5a5f` | 107/0 | Ancestor; retire ref |
| `origin/codex/release-v0.2.0` | `28b3e3248fa2da23ffe9e87a802f61daa7bcf21b` | 6/0 | Ancestor; retire ref |
| `origin/fix/v0.2.1-playtest` | `d738257192cb9ab54bfa7dadc0c7107d15a71956` | 1/0 | Ancestor; retire ref |
| `origin/main` | `6ba067ef0280b74fdf1c70ecc0edfe2f4f958aed` | 0/0 | Main |
| `origin/rewrite/rule-workshop` | `408e190b2e5ec325047e8e136a941e49c7303569` | 28/10 | Reviewed integration; retire after archival check |

All twelve `origin/claude/*` tips and the two release-branch tips are strict main ancestors: zero unintegrated commits. The historical `rewrite/rule-workshop` commits have different integration identities, so ancestry alone does not describe its disposition.

## Local refs and reviewed handoffs

| Local ref | Tip | Main-only/branch-only | Evidence/disposition |
|---|---|---:|---|
| `codex/bot-vehicle-coordination` | `6694839cbd67fcc41fbebc3ccc019616852d66d9` | 24/1 | Integrated as `609f2a0a`; retire historical ref. |
| `codex/content-reload` | `e2f2e0597da920e226ca0d66fb532257669690b8` | 22/2 | `c4c0f484` integrated as `17c8a5fa`; receipt `e2f2e059` as `5c29d12e`. |
| `codex/gate-coverage` | `792f65c6f16ae4be12e99c15423c6d3a78c27f53` | 22/0 | Main ancestor; retire ref. |
| `codex/release-v0.2.0` | `1ca461c6ec25e83286cc32dda6259fcdd8504061` | 5/0 | Main ancestor; retire ref. |
| `fix/v0.2.1-playtest` | `d738257192cb9ab54bfa7dadc0c7107d15a71956` | 1/0 | Main ancestor; preserve public release tags, retire branch ref. |
| `rewrite/rule-workshop` | `408e190b2e5ec325047e8e136a941e49c7303569` | 28/10 | Reviewed GUI/runtime snapshot integrated at `b8205f99`; follow-ups below. |
| `codex/v0.2.2-hardening` | `ed74537a589efd525e44cd0925d1e0c08b7706ee` | 0/1 | Active integration branch and dirty source; preserve. |
| `main` | `6ba067ef0280b74fdf1c70ecc0edfe2f4f958aed` | 0/0 | Preserve. |

Patch/evidence comparison:

- Vehicle coordination: 20 of 32 touched paths are byte-identical at integration. The other twelve differences retain concurrent Workshop, reload, teleport-heading, fixture and seam integration. The main release-coordination entry explicitly records `6694839c` → `609f2a0a` and its validation.
- Content reload: 86 of 101 source paths match at integration, including the complete worker implementation, role-based fixture helper, door port and content tooling. Remaining differences reconcile concurrent UI/CI/gate work. Its final receipt matches byte-for-byte and `git cherry main codex/content-reload` marks that receipt equivalent.
- Workshop: 67 of 84 branch paths match its initial integration. Branch-only artifact packaging/profile restrictions were intentionally replaced by production packaging. `e0181731` → `66da61bc` (isolation/budget fix) and `347997d6` → `eb3315fa` (heading fix) are patch-equivalent. Main subsequently improves guides, two-ball authoring, regions, NPCs and packaging. Do not merge the stale branch wholesale or restore its default Add-On/toolbelt policy.

## Historical receipt archived before retirement

The documentation-only `408e190b` appends 111 historical handoff lines to `docs/progress/2026-10-02-rule-workshop-ui-review.md` that were not copied to main. Runtime/integration conclusions are covered by main's release-coordination entry. The following static artifact receipt is worth retaining; this audit did not re-download the artifact or repeat those historical tests:

- Artifact-only Windows run [37071765152](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37071765152), source `839390231a8a80067398eb2c04125f8bab32a078`; artifact [11256253268](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/37071765152/artifacts/11256253268). No public tag/release was created by that lane.
- Recorded ZIP SHA-256: `55670f1aee2628e84abd095ebc837a9425ba7e8b387f03316bdb7b5e0040f8e1`; 135,184,334 bytes; 9,372 manifest files verified. Receipt checks include CRCs, safe/unique ZIP paths, file sizes/hashes, no unlisted files, executable headers, guides/local links, nine recipes and Toys.
- Historical Windows checks recorded 873 passing tests, zero failures, 137 ignored, strict clippy, startup, packaging and extracted smoke; catalog 14 maps / 966 brick definitions. This was an intermediate GUI snapshot, before the follow-up runtime fixes and combined release. It is not current v0.2.2 acceptance or a human playtest result.

## Worktree preservation

- Primary checkout: active v0.2.2 lane patches and new entries, including the pre-existing untracked Mac setup entry; preserve everything for root integration.
- `content-reload`: clean tracked source, no target directory. Preserve any needed ignored private-content/archive evidence before worktree removal.
- `rule-workshop-ui`: tracked PLAYTEST.md has two wording edits; main already contains the reset/credited-player corrections plus newer two-ball guidance. No target. Untracked `content` is a symlink to the primary content directory; never delete its target.
- Dedicated gate worktree: only untracked content symlink, with its gate target; preserve it for the gate's own management.

Recommendation: no old runtime branch needs integration. Retire ancestor refs and these completed handoff refs after root's artifact-preservation check; retain release tags. Root owns deletion and worktree cleanup. All recommendations remain unapplied by this audit.
