# 2026-10-28 Recorded brick sound immersion

Maxwell requested destruction sound immersion after the PR379 merge and asked
for an active goal through a new playtest. Source remains the reused warm
plastic-destruction-playtest worktree on codex/brick-destruction-audio, based on
merged main72632dfec. Original installations and the verified preview.4 remain
unchanged. This entry is an implementation checkpoint, not final delivery.

## Mechanism and authored experience

A native brick-destruction-audio package owns an exclusive optional effect-pack
response: snap/contact/landing profiles, spatial ranges, speed reference,
cooldown, pitch range and minimum membership for a clatter landing. Conflicting
providers disable this slot and report an Add-On problem instead of selecting
an unnoticed winner. Existing SoundDef weighted variants, prepared PCM, mixer,
spatialization, carry/reverb and volume controls are reused. Positional one-shot
profiles are required. No new menu toggle, gameplay contact or network event.

Sections report one snap when coherent visible motion starts and one landing at
the checked witness contact. Staged delivery is silent. Small groups use the
single-contact response; source-game whole-brick break audio
retains its normal80ms rate rule. The native opt-in suppresses Partition/Section
member delivery sounds so they cannot become a batch-timed metronome. Restored
material cannot start a new visible-section sound.

Rapier contact-force reporting is enabled on ordinary debris and landing chunks,
with thresholds scaled by mass and authored minimum impulse-equivalent speed.
The callback samples a bounded contact anchor and converts its CoM frame using
Rapier's documented relative-dominance convention. There is no extra contact
search, physics body or structural simulation. Chunk-proxy insertion has a short
mute grace; genuine first hits of ordinary thrown bricks are not muted. Resting
gravity is below the contact threshold. Session clear/reload drops old reports
and history; loaded package policy survives ordinary session clear.

At most64 reports are retained per rendered frame across its fixed steps. The
sink prioritizes landings, then snaps, then nearby strong contacts. Out-of-range
hits do not spend the shared8-per-second budget (burst3). Source and2-unit spatial
cells have cooldowns. These share the existing16-start local-audio queue; pitch
and radius changes clone only lightweight playback metadata, not decoded samples.
Audio hashing is separate from visual randomness. No continuous rattle bed or
full collapse sample is triggered per constituent brick.

## Recordings and processing

Eleven mono48kHz PCM16 clips: two snap-role taps, six contact variations and three
scatter cuts. These are CC0 real LEGO taps/drop/pour recordings by rioforce and
C_Sander, not synthetic/AI audio. The snap role uses tapped bricks and is not
claimed to be a literal recorded stud rupture. Inputs are public HQ MP3 previews,
not original WAV masters. The original-download login was not bypassed.

Exact source URLs, authors/licenses, input/outputSHA256, trim ranges and processing
version are in packages/brick-destruction-audio/SOURCES.json and LICENSE.txt.
The offline tools/prepare_brick_audio.py validates pinned preview hashes, decodes
to float to preserve MP3 overshoot, downmixes/resamples, removes DC, applies tiny
entrance/exit fades and bounded peak/active-RMS gain, then quantizes PCM16. Runtime
files are committed; no network, ffmpeg or synthesis runs during gameplay.

## Evidence so far

- `cargo check -p bri-client -p bri-weapons --all-targets --locked` initially
  caught outdated Rapier field assumptions (manifolds is a method; contacts use
  CoM-local anchors), then two test-fixture errors. Corrected from installed0.36
  source, used the existing scratch audio fixture and matched ClipData::Pcm to
  verify shared sample allocation. No validation was relaxed.
- `cargo test -p bri-client --lib brick_debris --locked`:42 passed,0 failed,
  1.71s. Includes silent staging, one shared snap/landing with Physics Off,
  world-space physical contacts with both collider insertion orders, resting
  silence, bounded report/rate/distance/history and no replay after reset.
- `cargo test -p bri-client --lib recorded_profiles --locked`: one offline mixer
  test passed. Native exports decode without failures, seeded shaped playback
  shares PCM, nearby output is finite/audible, distant output is silent. No device
  opened; this is technical output, not human listening.
- `cargo test -p bri-weapons --test brick_audio --locked`: two passed. Covers
  actual package policy/references/invalid values and explicit provider conflict.
- `cargo test -p bri-client --lib dense_pile_keeps_sound_work_bounded_and_preserves_physics --locked -- --nocapture`:
  passed.2048 bodies/180 steps: baseline416371us, enabled432665us, three starts.
  Same final poses exactly; contact report cap held and mass-scaled thresholds
  were positive before the first step. Added~0.09ms/step in this debug CPU-only
  single measurement (~3.9% of physics time). No rendered-FPS or auditory claim.

All-target clippy, quick/focused tiers, integrated gate, real-content startup,
Windows CI, final source/package receipts and the sound playtest are next. No
interactive gameplay or listening session was automated. Maxwell judges the
sound's feel and long-run restraint in the delivered build.

## Tier-check checkpoint

The first all-target clippy pass rejected direct f32::clamp; replaced it with
this repository's checked Clamp::clamped without suppressing the lint. Quick
passed724 client units, then found one existing net fixture constructing all
PackEffects fields explicitly. Added None there; the fixture's behavior is
unchanged. Follow-up quick for net/weapons passed90/14 units (3 net cases ignored).
Other affected effect fixtures already use defaults. Full focused checks follow.
Documentation validation passed74 maintained files and six release-guide layouts.
The default-add-on registry's original LF bytes were preserved so this is a
single-entry change rather than unrelated line-ending churn.

Maxwell subsequently explicitly authorized merging this sound increment into
main when ready. Finish integrated checks, Windows CI and verified packaging;
then merge without a further approval request. Human listening/playtest remains
his later acceptance step, not a prerequisite he asked to block the merge.

Final review confirmed the eleven shipped mono48kHz PCM16 exports match their
SOURCES.json SHA256 records (397012 bytes total). All-target clippy now passes
without warnings. The dependent integration tier is running on unchanged code;
main remains72632dfec after fetch. The final receipt and PR will record the
committed candidate, integrated gate, CI and packaged startup rather than
claiming a listening/playtest acceptance from automated output.

Focused tier completed successfully in1035s: warnings-as-errors clippy;
all tests of eight dependent crates;724 client units passed,78 ignored by
the normal focused tier. No failures or test exceptions. The integrated gate
will also run ignored cases. Committing this unchanged implementation for
Windows CI, warm playtest build and the full integrated gate.
