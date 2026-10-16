# Hosted pullback and mesh slope reports

Maxwell reports two movement issues in the released-v0.3.1 Halo playtest context.
The exact executable/package identity during the symptoms is not independently
captured; preserve the delivered preview and its receipts.

- With the same Blockhead, walking/jumping on Blood Gulch mesh slopes feels
  sticky/high-friction and less able to bounce off slopes than The Slopes
  heightfield. This is not evidence of a Chief-specific movement policy issue.
- Occasional subtle repeated pullback toward earlier positions occurs while
  movement continues. Respawning clears the symptom. Maxwell explicitly
  confirmed **hosting multiplayer**; occurrence on other maps is unconfirmed.

Neither report has an established cause or reproduced regression yet. Do not
globally tune friction, disable reconciliation or infer a network connection
problem from the symptom alone. Required reliability rows B7/B8 remain open.

Root queued read-only diagnosis with the existing map/core owner after its
actual single-player Yes-button checkpoint, prioritizing hosted pullback before
lower-priority render research. Compare matched mesh/heightfield slopes and
existing jump-normal controls; for pullback capture server/client position,
velocity, grounding, input acknowledgement/correction and respawn lifecycle.
Return a bounded path proposal before production edits. Existing dropped-item
and render reports remain assigned. No interactive gameplay is authorized.

This entry and readiness update change documentation only; no runtime fix,
test pass or reporter resolution is claimed.
