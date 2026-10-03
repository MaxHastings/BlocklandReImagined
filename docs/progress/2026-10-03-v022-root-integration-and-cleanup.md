# v0.2.2 integration, maintenance and release preparation

Maxwell requested maintenance cleanup, branch consolidation and newcomer docs
polish after the creator/NPC lanes, followed by v0.2.2 on main. This entry
records preparation; publication evidence will be added only after verification.
Interactive playtesting remains Maxwell's.

## Shared integration choices

- MiniGame setting `Item` and `PlayerType` retain their semantic purpose in the
  client/UI bridge instead of becoming undifferentiated lists. Their values and
  authoring/save/network representation are unchanged. The UI can prioritize
  equipment and bodies without recognizing Slayer IDs or setting titles.
- Custom body conversion carries square authored collision dimensions,
  density/drag/step height and contiguous transformed mount nodes. Different
  crouched/prone widths and unknown external-base widths are reported as gaps,
  because the motor has one horizontal width. No original asset is committed.
- Custom avatar models play an authored swim loop while in liquid and show
  selected matching accessories. Death/sitting still take precedence; ordinary
  Blockhead water animation is unchanged.
- Scripts read body model, actor mount and immutable bot kind independently.
  Body changes do not transfer brain ownership. Rest is permitted for a kind's
  provider/declared companions; deletion and tool control remain creator-owned.
  Independent review caught a model-only Shark admission that could mount a
  victim before an unauthorized rest failed. Holder admission now also checks
  the actual kind provider; victim classification still follows its body.
- Sight ignores Water brick volumes while editing rays still select them and
  ordinary walls still obstruct sight. The physical Shark fixture exposed this
  generic perception bug across a water surface; it is not a Shark exception.

## Bounded maintenance

The Wrench owner extracted duplicated complete-row vocabulary/type checks,
preserving distinct handling of host-preserved rows versus unavailable copies.
The NPC owner centralized elapsed-time accounting for interrupted approaches;
scheduled event waits continue to use absolute due times.

Root moved physics query/operation registrations from the large script module
into its existing private child-module structure. An exact text comparison
against HEAD confirms the entire registration function body is unchanged;
only its private function name/visibility and call site changed. Registered
names, overload order, operation helpers and authorization are preserved.
This is a thematic maintenance boundary, not a new operation bus or framework.
Scoped review found no justified dead-code deletion; serialized fields and
intentional provider seams are retained. This does not claim a debt-free engine.

## Branch and docs preparation

The [branch audit](2026-10-03-v022-branch-retirement-audit.md) records every live
remote head and local handoff. Twelve Claude branches and both older release
branches are main ancestors. Workshop/vehicle/content handoffs have verified
integration mappings; stale production trees will not be merged over later fixes.
The useful historical Workshop artifact receipt is retained in the audit.

Before ref deletion, root created ignored `dist/v0.2.2/branch-archive/refs.txt`
and `pre-cleanup.bundle` with all refs. Bundle SHA-256:
`796e64257597dc740c92abd89ed233df8afc9e31581dd5eda5973ff10461b377`.
This archives historical committed work; the unrelated untracked Mac setup
note and current integration edits remain in the main checkout.

Newcomer entry points now distinguish playing, creating without code and
Add-On development; platform/install/log requirements are corrected.
Repository-wide local Markdown link scan: **287 targets, zero missing** at
this checkpoint (anchors/external URLs excluded). Historical research remains
dated evidence. Updated AGENTS model preference to Maxwell's latest Sol High
instruction, replacing its stale Luna instruction.

## Verification at this checkpoint

- Importer library: 30 passed before the additional stance-width regression;
  `/tmp/bri-v022-root-importer.log`. The final importer rerun remains pending.
- Client custom body/swim/accessory test: 1 passed;
  `/tmp/bri-v022-root-avatar.log`.
- Client equipment/body UI bridge test: 1 passed;
  `/tmp/bri-v022-root-minigame-bridge.log`.
- Fresh import of pinned Steam `Bot_Shark.zip` initially produced raw 14×14×7.2
  collision dimensions, buoyancy fields, four real transformed mount nodes and the
  companion manifest. Source pin is
  `02f9ec67645e6b3736be651fe4298a8796fe8d1f2edd7515028212d96b70ac04`.
  The final source audit caught v20 collision scaling: PlayerData dimensions
  are quartered by Player::step, so native Shark dimensions must be 3.5×3.5×1.8.
  Importer conversion and vanilla-equivalence regression now apply that scale;
  fresh package and physical verification follow below.

Current final regression, physical Shark, sustained performance, gate, Windows
CI and platform publication evidence are not yet complete. The original Windows
firefight crash/hang and universal NPC inference remain unclaimed.

## Added creator feedback and final regression checkpoint

Maxwell requested a simple Colorsets button/chooser in Start Game and a writable
colorsets folder, plus the linked Trueno edition bundled with credit. The chooser
uses radio rows, preview swatches and Use/Cancel draft behavior. IDs survive catalog
refresh; a missing choice does not silently launch with a replacement. The client
reads bounded text files (1–256 finite colors) without running scripts. Original
v20 whole-row integer/float and division semantics are preserved. A selected palette
initializes new authoritative worlds and goes to guests with existing replication;
saved palettes and Tutorial's stock colors remain intact.

The exact requested page's browser Download produced `Colorset_Trueno.zip`,
SHA-256 `99a1b3d3dc87af59645b71fa329160510df3af7dad88e2391a4801d301be2130`.
Its description credits Trueno. The source archive stays ignored; the public
manifest pins it, and generic import now preserves root `colorSet.txt` as data.
Importer library after collision-scale correction: **31 passed**
(`/tmp/bri-v022-root-importer-quarter.log`). Colorset import regression: **1 passed**
(`/tmp/bri-v022-root-importer-colorset.log`), followed by a successful debug importer build.

The Shark physical fixture required its provider to hide its own hole marker
before a MiniGame existed. Existing `set_brick_shown` now permits its brick
definition's provider or explicitly declared host companion, retaining `world.edit`
authorization and existing trust checks for foreign bricks. It does not grant
provider-based painting, deletion or arbitrary build edits. Positive/negative
permission regression and physical proof are pending.

Maxwell asked for longer rocket destruction debris. Client-local blast bodies
now remain opaque for **10 seconds**, then fade over **3 seconds**, versus 3+2.
Physics Quality counts, adaptive 6 ms work target, early shedding and instance
limits are unchanged. Hammer/undo kills retain their separate v20 falling
visual. The physical fade test now proves blast debris stays opaque beyond the
old five-second total life; final client regression is pending.

NPC ordinary-control tests at this checkpoint: rest **2 passed**, weapon tactics
**4 passed**, bot units **61 passed / 1 ignored timing**, water sight **1 passed**.
A traced ammo failure was real spawn immunity, not weapon range: bots now preserve
ballistic aim/valid charge hold while withholding actual attacks/release until
canonical spawn protection ends. Windows frame-time acceptance and sustained
optimized measurements remain open.

## Completed focused evidence (publication still pending)

- Colorset reader/fresh-world palette: **4 passed**, including real generated
  content (`/tmp/bri-v022-root-client-colorsets.log`).
- Longer debris physics, eviction and bounded-work regressions: **15 passed**
  (`/tmp/bri-v022-root-client-debris.log`).
- Actual original Shark textures, finite scaled geometry, authored swim and
  right/left/both held-tool poses: **1 passed**
  (`/tmp/bri-v022-root-client-shark-equipped.log`). Real-content testing exposed
  additive swim preceding absolute fallback look clips. Complete-layer stable
  partition fixes this without changing the existing order within each type;
  custom additive held-arm clips are also honored.
- Avatar regression over real and synthetic content: **36 passed**
  (`/tmp/bri-v022-root-client-avatar-content.log`).
- Script sandbox, operation registration and actor relationship regressions:
  **30 passed** (`/tmp/bri-v022-root-runtime-final.log`).
- Shark physical/permission/lifecycle: **13 passed**, including original
  imported policy. First life took genuine fallback bites; one ordinary respawn
  preceded a real capture on life two. Actual mouth alignment, exactly 600 ticks
  to credited death and no script warnings passed. No probability/admission was
  changed to satisfy the test (`/tmp/bri-v022-sol-shark-respawns.log`).
- Final creator UI: library **195**, field flow **8**, MiniGame **24**, runtime
  input **34**, final menu **6**, chooser **4** passed; native captures and
  scoped UI all-target clippy passed. Use refreshes the underlying Start Game
  summary/Launch state before save acknowledgment. See owner progress entries.

The final private staging contains **88 originals / 135 default sources** and
validated credits/companions, including correctly scaled Shark and pinned
Trueno. `dist/addon-bundle` is staged for startup/packaging; final zip/source
receipt and private draft upload are pending. Root all-target lint caught one
importer needless borrow (fixed) and one benchmark collapsible-if (owner fixing);
no lint waiver was added.

## Final local integration checks

Combined client/sim/runtime/importer/chaos all-target strict clippy passed,
followed by a fresh client/importer debug build and actual content startup:
**14 maps, 963 brick definitions, 35/35 save pictures, no Add-On health problems**.
Logs: `/tmp/bri-v022-root-final-{clippy,build,startup}.log`. No window/audio
opened. Final review caught lossy non-UTF-8 filenames aliasing colorset IDs;
the catalog now skips those filenames with a warning, and a Unix regression
checks that two invalid byte names cannot replace a valid UTF-8 palette.
The fresh focused test and release gate are still pending.

The seam ownership ledger now identifies already-integrated v0.2.1 mechanisms
as on main, separately from behavioral completeness. It does not reopen
finished work lanes or imply full fidelity. New v0.2.2 seams remain under review.

Final Shark import was rebuilt from the current embedded port notes and the
pinned read-only Steam archive, using installed native content. The initially
requested default recovered-script paths are absent on this Mac; installed
content supplies the supported base instead. Reports and companion are refreshed
in the private bundle and local generated pair; all **135 sources / 88 originals**
validate. Original source folders remain untouched.

Root inspected the final compact native Colorsets chooser and 400×300 Start Game
fit. Packaged guide checking caught stale links into the repository layout;
DESIGN now points to the shipped v0.2.2 guide, and the common packaging helper
rewrites the copied feature/known-issue links too. **11 packaged local links,
zero missing** after this repair. The apparent repository missing `$index + 1`
is a code expression in historical research, not a Markdown link.
