# Confined-bot recovery

The previously failing, explicitly opted-in sealed-brick case now passes. A
bot can move around inside a small enclosure without making actual progress
out of it, so an absolute two-unit displacement timer alone was insufficient.
For a kind that explicitly permits self-respawn, an exhausted navigation
frontier whose visited nodes all lie within the local radius now supplies a
separate confinement observation. A soft search limit or pending frontier is
not a confinement observation. The existing patience and suicide permission
still govern the choice; default kinds remain opted out.

The observation is tied to the static collision revision and position, is
discarded when another living actor is near, and clears on death, body change,
or the rules or another player taking control. The new score is used only while
grounded and dry. This is evidence from the bounded navigation model, not a
claim that every physically possible exit has been proven impossible. Dynamic
cooperative supports and broad pursuit/search work remain separate.

Checks passed: `cargo test --locked --profile playtest -p bri-chaos --test
bot_brain sealed_in` (both the default no-suicide and explicit opt-in cases),
`cargo test --locked --profile playtest -p bri-sim
an_exhausted_graph_does_not_resume_as_if_it_only_ran_out_of_work --lib`,
`cargo test --locked --profile playtest -p bri-sim
confinement_evidence_expires_on_geometry_actor_or_location_change --lib`,
and `cargo clippy --locked --profile playtest -p bri-sim -p bri-chaos --lib
--test bot_brain -- -D warnings`. The older failure recorded in
`2026-10-31-v043-bot-self-respawn-policy.md` was reproduced on the baseline;
the same test passes with this change. This does not diagnose the distinct cube
respawn or finish the user's five saved scenarios.
