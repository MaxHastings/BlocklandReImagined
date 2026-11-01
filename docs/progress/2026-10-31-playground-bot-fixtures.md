# Playground bot fixture bodies during release checks

The c97d8ac8d Linux CI shard ran all 19 playground integration cases and
failed the Blockhead hand-muzzle and buggy-driver cases, including isolated
retries. Both failures reproduced identically with the preceding navigation
source, so the low-wall change did not introduce them. The first fixture's
recorded actual archetype was `playground-runner:archetype/runner`, while its
expected hand was the standard Blockhead's rig. The Blockhead bot kind leaves
its body unspecified, so the mode assigns its Runner player type. The second
test also described a Blockhead but ran that Runner: its visible armed bot
fought on foot and never boarded within the fixture's 90 seconds. This trace
does not establish that every such vehicle choice is optimal.

Both tests now author the standard body for the bot they intend to exercise
and explicitly assert that body. The muzzle-position, firing, boarding,
turning and distance assertions remain intact. The general combat scenario
still uses the real unmodified bot kind and now asserts that the unspecified
body inherits the mode's Runner type. This keeps coverage for both explicit
kind bodies and mode defaults, instead of globally changing the test Add-On or
forcing a vehicle when the bot can engage on foot.

`cargo test --locked --profile playtest -p bri-sim --test playground` passed
all 19 cases; `cargo fmt -p bri-sim -- --check` and `cargo clippy --locked -p
bri-sim --test playground -- -D warnings` passed on Windows. Linux CI and the
full release gate must be rerun on the resulting commit. No interactive test
was performed.
