# 2026-11-04 v0.4.4 release gate corrections

Maxwell approved preview.6 (`93767ca190031702e7d3e38a4b9c2150cfc75382`)
and finished playing. He authorized the remaining checks and publication only
when they pass. No v0.4.4 tag or public release has been created.

## First extended run: failed, not release evidence

`python -u tools/gate.py --full --release-content 93767ca190031702e7d3e38a4b9c2150cfc75382`
used the isolated v044-release-gate worktree/target and physical release-prep
content. Tool tests, workspace build, packaged-content startup and warning-free
workspace all-target Clippy passed. The test phase executed 531 binaries;
18 persistent cases failed after named retries. The run stopped before the
performance/corpus completion receipt. Its full log is retained externally.

Missing stock fixture packs and explicit Bot Town/tower save inputs explain
14 failures. Restoring those test inputs passes the original unmodified test
binaries (15 debris cases, four default Add-On cases, three hosted cases, six
borrowed-brick cases, Bot Town and both tower CTF cases). Release packages.json
still selects the verified shipping variants; stock fixtures are additional
unlisted inputs. No production content policy or test assertion was changed.

The original client scope eligibility, body attachment render and 32-speaker
voice cases passed only on retry. They are flaky observations, not first-pass
success. The render log showed minutes spent compiling through FXC. Windows
full gates now stage the existing pinned/verified DXC runtime beside both loose
client and libtest executables. Whole-client render fixtures use the production
offscreen backend/pixel probe. Forced software-GPU coverage remains available.
Both attachment render variants subsequently pass through that production setup.

The listen-host Q/drop failure was a Windows Firewall question intercepting Q,
not item motion. The headless fixture declines only that exact known question;
it never changes firewall permissions and rejects unrelated dialogs. Both
single-player and listen-host item-motion cases pass with their original checks.

## Actual navigation regressions

The unchanged roof-pursuit and deck-safety cases both pass on released v0.4.3
(`d2643c62b`) and fail on preview.6. Height-first partial endpoint ranking can
retain a slightly raised spawn plate instead of approaching beneath an
unreachable roof. Nearby route height changes also granted permission for any
non-harmful drop: a tiny planned step could license walking off a six-unit deck.

The safety correction passes only the nearby verified route floor to the walk
support probe. Off-route footwork does not inherit either that floor or an air
leg's landing exemption. Existing planned descents remain permitted; additional
assertions challenge small steps, diverted walks and airborne diversions. The
original deck scenario now reports no falls. Partial endpoints now use the existing geometric distance estimate. Floor
preference only breaks discovery-cost ties. This restores useful roof approach
without inventing a floor taxonomy, extra tuning or a second route planner.
Two experiments retaining step-tolerant/height-banded endpoint preferences did
not improve the tower replay and were removed.

The private tower replay fails its stronger requirement that every zombie
reacquire the live host upstairs before any zombie kills him. The matched
preview.6 control also fails that requirement. All three begin pursuit while
their initial observations are valid; in the corrected geometric-progress run,
two reach the live host and the third is upstairs at y=315.01 when the host at
y=315.60 dies. No bot falls/dies and the host is killed normally. That is not
an all-three pass; the accepted preview's control reaches only one before death.
The stairs and balcony replays passed. The tower assertion is unchanged and
its failed result remains explicit. No complete release pass is claimed.

## Performance contract investigation

The old creature check compares two different fights and asserts a quarter of
full-bot time. Release-profile measurement was 230 us/tick for 16 creatures
versus 508 us for full bots (45%, failure). Historical v0.3.0 evidence was
227 us versus 1060 us: roughly unchanged creature work can fail when full bots
get faster. The matched released v0.4.3 check also fails: 265 us versus 929 us
(29%). Current creature thinking is faster than that baseline; the ratio
failure is not evidence of a creature slowdown.

The replacement admission-capacity check exercises all 64 creatures admitted
by the default 16-slot server limit, verifies they actually spawn under that
limit, and checks the existing absolute debug/release CPU budget. It retains
the full-bot budget check and reports comparative timings diagnostically.
No production bot behavior, fractional admission weight or CPU budget changes.
The intermediate release-profile capacity check passes: 64 creatures use
1576 us/tick against the unchanged 2000 us/tick budget; full bots use 577 us.
Final-source performance and extended validation remain pending.

## Remaining

Resolve navigation evidence without tuning to individual saves, rerun changed
checks, commit/push exact source, regenerate strict Add-On receipts, require new
platform CI/native artifacts and the complete extended release gate. Only then
merge/tag/publish and independently verify the public downloads. Preview.6's
successful CI and native build-only run do not validate the corrections above.

Changed-area all-target warning-free Clippy (`bri-sim`, `bri-chaos`,
`bri-client`), `python tools/test_gate.py`, formatting and documentation checks
pass after the corrections. Initial Clippy rejected a redundant Boolean and
an unnecessary Option unwrap; both were simplified without suppression.
The existing six hardening, eight navigation-spike and ten route cases pass
with the final geometric endpoint rule before those equivalent lint cleanups.
Final full gate is still required; no main merge or tag is authorized by these
scoped results alone.
