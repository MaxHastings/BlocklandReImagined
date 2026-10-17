# 2026-10-17 Current weapon render observation for the AA experiment

Root authorized a bounded observation gap correction from exact
`651efb2ff361579dde657b54d4023b2ee313bab7`, preserving the paused surround
branch/test/patch and all earlier engine history. This is not a gameplay,
source-pose, clock, effect policy or scope-surround change.

The default-off App probe retains at most one successful current render's
already computed weapon snapshot and actual effects camera, current timed
adapter cues/handles/resolved transforms, current mounted image model/pose
inputs, replica/session context, and combined budget/cut counts. It clears on
render entry, toggles, changed effect session and disconnect; reads reject a
mismatched entered session. No extra snapshot, pose evaluation, history queue,
reliable cursor drain or cloned buffers when disabled. Current mounted-image
reads use the exact image/hand guard and retained sequence/time/live inputs.

Prepared prebudget data does not prove submission, visible pixels, original
binding orientation or source fidelity. AA must qualify the same real admitted
M16 transient on this exact tested pin with zero cuts and same-frame GPU output,
local plus observer, against independent original-source position evidence and
a wrong-node negative control. That actual consumer remains pending.

Tooling correction before source freeze: Python's default Windows text encoding
mangled original Unicode strings in session/tests. Restored the original bytes
from exact HEAD; final source diff contains only additions. Subsequent edits
use explicit UTF-8. No gameplay string change is intended.

Validation logs and frozen hashes are in this worktree's ignored artifacts/
`aa-effect-observation-*` files. Cargo jobs2/test threads8, target override unset.
Targeted, quick and full normal focused results will be recorded below.

Targeted controls: Timed cue/handle/transform/cursor/reset1pass; App probe
toggle/session/disconnect1pass. First ordinary offscreen test failed because
it waited for source admission before the emitter emitted any particle. Kept
nonempty prebudget-frame assertion and corrected only synthetic fixture wait
to require an actual emitted particle. Rerun1pass/26.27s, ordinary host/equip/
fire, current mounted-input guards, actual prepared nonempty weapon frame,
zero combined cuts, off/on/early-return/disconnect. No runtime correction.
First red retained as aa-effect-observation-app-render-first-empty.log.
Refreshed nine-source-file hash freeze after fixture correction.

Final validation: `python tools/check.py quick --crates bri-client` PASS
2steps/54s, 601passed/0failed/76existing ignored.
`python tools/check.py focused --crates bri-client --tests bri-client:weapon_frame_probe`
PASS exit0, full normal plan4steps/156s: format, all-target clippy -Dwarnings,
client601/0/76 and explicit ordinary offscreen App1/0 (23.41s).
No new ignores, waivers, full gate, main integration or AA consumer claim.
All nine frozen source/test SHA256s reverified before commit; old source lines
unchanged (only additions). Root owns review, consumer authorization and integration.
