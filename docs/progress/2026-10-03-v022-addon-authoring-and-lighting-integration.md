# 2026-10-03 Add-On authoring and lighting integration

Maxwell confirmed the boundary: an unfamiliar Add-On must describe its
capabilities, and doing so must be practical for its author. The host composes
declared mechanics through the existing bounded planner; an author does not
write a second bot brain. The current alpha declarations remain deliberately
narrow rather than claiming arbitrary script understanding.

The [author guide](../modding/bot-objectives.md) supplies copyable return-policy
and physical-hold declarations, observed completion bindings, validation bounds
and actual callback responsibilities. Root requested an independent accuracy
and author-friction review before calling this handoff complete.

Root added a separate `modern_lighting` preparation step to regeneration.
It runs before `--check`, including for copied map packs, and delegates source
identity/schema validation to the Rust preparer instead of implementing a
second validator in Python. Classic/Unified bake generation stays unchanged.
The isolated lighting thread's source is not yet integrated; end-to-end
generator/startup checks and private content refresh remain pending its handoff.

`python3 -m unittest discover -s tools -p 'test_*.py'` passed all 16 existing
tooling tests. `python3 -m py_compile tools/regenerate_content.py` passed.
These checks do not prove the pending Rust generator integration.

The latest independent tool-loss fixture rerun compiled but exposed two failed
causal preconditions in the first transformed world: its callback removed the
tool before the observed intended body moved a meter. Three other adversarial
cases, seven unfamiliar package cases and the actual CTF journey passed. The
owner is investigating callback baseline/object identity and native grip
chronology; no assertions or physics were weakened to conceal that failure.
Full acceptance, exclusive active-battle timings, gate/CI and publication
remain pending.

The proving run recorded a concrete fixture identity error: its actor-only
baseline came from decoy `vehicle:4`, while the new intended grip was
`vehicle:3`; their different stationary positions appeared as 18 square meters
of movement. The corrected callback keys observations by actor and native
object reference, keeps the genuine one-meter displacement requirement, and
continues observing every body rather than filtering away decoys.

That investigation also found an actual integration gap. A normal ray can
briefly acquire an intervening body while aim settles. The Hold directive now
requests ordinary trigger-up for a different observed grip, retaining its
original bounded approach deadline. The root brain retains ownership of those
controls during recovery so legacy carrying cannot start a competing swing.
It never forces a ray hit or directly sets a grip; repeated obstruction must
expire or replan normally. Native release preserves real momentum.

`/tmp/bri-v022-final-ordinary-controls.log` passes 66 current-source cases:
creator acceptance 8, held-out 1, interactions 15, objective rest 2, existing
objectives 11, physical objectives 9, physics interactions 11, search 4 and
tactics 5. These include the actual mounted charged-release regression.
The corrected independent loss/guard cases are being rerun separately.

The final corrected independent run passed CTF 1, unfamiliar package 7 and
adversarial 5 (`/tmp/bri-v022-native-loss-guard-final.log`, adversarial 2.60 s).
Its last fixture failure was a rejected color index: the native host palette
contained four entries while the authored callback assumed eight. The callback
now chooses a different valid color. Same-tick application remains strict;
the actual valid change causes due-time Color failure and prevents score, win
and cosmetic output despite witnessed native entry in the coasting variant.
Independent outcome review signs off the fourteen mechanisms and held-out
composition; stronger name/ID/order/decor variation for two positive fixtures
is being added before complete contract signoff.

The affected all-target clippy run found only a test-module placement lint in
the new combat adapter. The complete unchanged test module was moved to the
end by its owner. The strict recheck is pending; no lint exception was added.

Independent review of the isolated lighting handoff found three concrete
issues: obsolete compatibility preparation jobs surviving mode switches,
requested settings disagreeing with the accepted renderer source after failed
reload, and accepted small lamp radii producing invalid shadow projections.
Heavy compatibility preparation also needed to move off the render thread.
The owner is implementing narrow lifecycle/bounds corrections before root
integration. No final lighting or release acceptance is claimed yet.

Final strengthened variants pass: search 4 in 0.29 s
(`/tmp/bri-v022-final-search-variants.log`) and package 7 in 0.26 s
(`/tmp/bri-v022-final-package-variants.log`). The package observer had to resolve
the actual native source after the existing setup steps: normal `LoadBuild`
discards save keys and allocates new IDs, and its queued command applies during
the next ordinary step. Neither correction changes production behavior or
manufactures an outcome. Independent review now signs off all fourteen bounded
NPC journeys plus the held-out composition at source/headless outcome level.

Protocol audit added two version-count markers. New shared Add-On JSON metadata
is downloaded through ordinary package objects; older strict parsers reject it,
so peers must fail early at version handshake. Existing serialized event rows
also gain delayed Projectile execution semantics. Internal desired-state,
death/round observers and diagnostic views do not add wire fields. The alpha
metadata remains experimental, with no beta compatibility promise.
