# 2026-10-31 Global minigame voice includes everyone

Applied Maxwell's explicit choice: Global minigame voice joins living and dead
members in the same conversation. The package's previous dead-to-living denial
now applies only to Proximity/Team. Existing group isolation, authoritative
membership, priority composition, mute and transport generation rules remain.
No new wire format or native audio processor is needed for this policy change.

Extended the actual package/session death-policy integration to verify Global
in both directions, then live-switch the same game to Proximity and Team and
verify their retained death restrictions. `cargo test --locked --profile playtest
-p bri-net --test voice_policy` passes all six cases, including real-network
membership revocation/regrant and package authorization/conflicts. Changed sim/net
all-target Clippy passes with `-D warnings`. This is scoped technical validation,
not a new manual playtest or completion of loading voice/effect seams.
