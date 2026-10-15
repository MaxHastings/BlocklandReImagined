# Weighted one-shot sound profiles

Native `SoundDef.variants` references up to 64 distinct leaf profiles with positive
weights. Selection happens once per playback on the client. The chosen clip keeps
its original gain multiplied by the parent gain; prepared PCM/stream assets are
shared instead of decoded for each alias or playback. Loops, music, recursive
choices and differing local/world placement are rejected. Missing leaf clips are
diagnosed and omitted; if none loads, the parent file is tried as fallback. Pack
reload replaces choices. Add-On health includes leaf references.

Sound selection is cosmetic and does not change authoritative rules. Added the
protocol note, public modding documentation and seam ownership. Legacy profiles
with no choices retain their existing behavior. This does not implement layered
loops, sound continuations or an original game's skip/retry algorithm.

Evidence:

- `cargo check -p bri-client -p bri-addon-import`: pass; later loader-sharing
  refactor also passed `cargo check -p bri-client`.
- Targeted offline regression covers weighted boundaries, shared assets, gains,
  audible playback, rejected recursion/loop/placement changes, missing clips and
  reload. Pass. Initial fixture used schema 1 instead of the current schema 5;
  it now supplies `bri_weapons::SCHEMA` in JSON before deserialization. Runtime
  validation was not loosened.
- Initial quick run failed on that fixture after passing importer tests. The
  subsequent focused run completed all changed-crate checks successfully:
  `python tools/check.py focused --crates bri-client,bri-weapons,bri-addon-import,bri-net
  --depth 0`, 12 steps/757s. Includes 567 passing client units, importer/weapon
  integrations and network tests including crossover and sized join.
- A health-reference addition after the initial clippy pass was included in the
  later weapon tests and separately rechecked with `cargo clippy -p bri-weapons
  --all-targets --locked -- -D warnings`: pass.
- Private source fixture: seven impact groups/40 original clip references decode
  and play through the actual offline client mixer, .71s. Its initial use of a
  nonexistent Pack::load helper was corrected to Pack::from_json. This is technical
  playback evidence, not listening or original-selection acceptance.

Private binding only uses uniform original permutations with zero skip fractions
and no continuation links. Dirt-hit skip behavior remains explicit work. Native
source casing gravity sign was inspected and is correct; the approximate base
gravity constant is not claimed as recovered. Source audit also confirms missing
planar particle orientation and authored particle lighting, next shared work.

No interactive session, installed playtest update, full candidate gate or release
publication in this checkpoint. Final integrated/Windows/human acceptance remains
open. Prior committed engine checkpoint is on draft PR #171; private assets on
private PR #3. No private assets or fixtures are staged in the engine repository.
