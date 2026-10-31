# Pursuit commitment while routing

The local destination timeout already paused during route calculation and
travel, but `bot_evidence` still discarded the observation at its original
expiry. An NPC could consequently stop before navigating to the place it had
actually seen. Search now keeps a separate local commitment clock while a
chase/search route is in progress. The observation's original timestamp,
position and expiry remain unchanged; it is not re-shared as fresh allied news.
Once routing stops, the remaining local search time runs down normally.

The retained intent is canceled when the target dies, leaves, or becomes an
ally, and newer knowledge supersedes it. An older relay cannot replace it; a
shorter deadline cannot later be lengthened by the same observation. Repeated
route calls in one tick do not add extra time. This does not yet implement the
requested connected-building exploration beyond the existing local probes.

`cargo test --locked --profile playtest -p bri-sim search_memory::tests --lib`
passed 12 cases. `cargo test --locked --profile playtest -p bri-chaos --test
bot_knowledge` passed 12 cases, including expiry while traveling, eventual
search termination, and target death and team changes before and after expiry.
Warning-free `cargo clippy --locked --profile playtest -p bri-sim -p bri-chaos
--lib --test bot_knowledge -- -D warnings` passed. Private 14400-tick replays
of the mattress, tower, house, and staircase saves killed the target at ticks
2360, 4973, 1234, and 3240 respectively, with zero recorded bot-step failures;
reports are `v043-retained-{mattress,tower,house,stairs}.json` in planning
diagnostics. These are headless outcomes, not evidence of visual stair quality
or human playtest acceptance. Cube stacking remains unsolved.
