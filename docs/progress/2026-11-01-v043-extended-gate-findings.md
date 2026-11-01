# v0.4.3 extended gate findings

The full Windows release gate on c97d8ac8d completed history, tool tests,
workspace build, release-content startup and workspace Clippy, then ran 530
test binaries. It failed on the test pass; it did not reach performance and
is not a release pass. Complete output: private
`v043-final-gate/logs/c97d8ac8d1d2.log`. Input playback and the 32-speaker
voice case failed under the concurrent gate and passed on isolated retry;
they are flaky observations, not clean first-pass results.

Two playground failures were separately traced to tests expecting a standard
body while their unspecified kind inherited the mode's Runner; commit
25bc7aa6c preserves the original behavior assertions using an explicitly
authored standard body and asserts the Runner default in the normal fixture.

The isolated gate content held the actual release variants but not the stock
`item-presentation-pack-010`, `effects-runtime-pack-005`, and
`weapon-debris-pack-004` directories required by stock/fresh-checkout tests.
Copied the existing original baseline packs from the integration checkout
into the isolated gate content; the release content's `packages.json` and
release variants are unchanged. The supplied Bot_Town import fixture had also
been removed: regenerated it privately from `E:\Downloads\Bot_Town.zip` with
the installed native Bot_Hole dependency. Both the eight-brick catalog test
and the host/plant/save/reload/late-join test passed with
`BRI_BOT_TOWN_OUTPUT=diagnostics/gate-bot-town/bot_town`. Pointed
`BRI_TOWER_SAVE` at the user's Beta16 city tower native save and reran both
real-content tower CTF variants in the gate binary: both captured without
jets (center pickup/capture 3644/6995; wall 3632/6924 ticks).

The falling-contact test's old disconnect/resume/rejoin sequence attempted to
join a minigame the authenticated resume already restores. The fixture now sets
the elevated spawn before resume and asserts both the retained membership and
actual position, before exercising the same contact and attribution. Its
focused playtest-profile case and changed-target Clippy passed.

Other failures remain unresolved. The activatable-door case opens its door,
but the bot waits at x=-2.6345 while seeing the human through it. The opened
fixture is a slab at y=1.4..1.6; the body is 2.65 high standing, 1.0 crouched.
No cause or fix has yet been established. In a tool-loss case the tool was
removed at tick 158, the bot died at 386 and respawned with its normal loadout
at 658; the test's assertion that the tool stays absent across lives needs a
contract-aware investigation, not an automatic relaxation. Other failed bot,
mirror and default-addon cases remain in the gate log. Do not claim a release
pass, drop assertions, or call a retry-only pass clean.
