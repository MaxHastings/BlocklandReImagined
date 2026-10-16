# 2026-10-16 Inventory, loading and map feedback

Maxwell relayed additional pharzedia v0.3.1 reports. Preserve them as distinct
observations rather than one presumed cause:

- Some textures glow near or far. The reporter later attributes some or all glow
  to bloom. This does not prove every terrain/material input or bloom value is
  correct. Retain comparison with the earlier blue-grey beach report.
- GSF Paradise lacks its rotating gas-station sign. The reporter recalls a DTS
  (possibly animated), spawned over two yellow 1x1 bricks, without mesh collision
  and pickable only after clipping past independent invisible map collision.
  Qualify the authored object and its placement/spawn/animation/pickup route;
  do not invent static geometry, a collider or a new policy from that recollection.
- Multiplayer loading screens omit the blue own-name typing indication that
  works after spawn. Diagnose actual loading input, local text/rendering and
  accepted-server identity/typing updates; preserve normal chat and input focus.
- Single-player Change Map does nothing after Yes. B1 Session/network evidence
  does not automatically cover the local UI confirmation route. Test stock and
  namespaced Add-On paths before calling it the same bug.
- Q close/reopen selects slot zero/hammer instead of the previously held tool.
  Read-only HUD inspection finds use_tools already tries cur_tool, but a host
  apply_active_tool(None) echo clears it. This is a candidate cause requiring
  an actual authoritative-echo regression, not a confirmed fix. Remembering a
  valid prior selection must not override authoritative current equipment.
- Thrown Q-menu items/weapons reportedly freeze at the top of their arc rather
  than falling. Reported mode is unknown. Trace DropTool, authoritative WorldItems
  physics/collision/settling, pause behavior and client replication/presentation;
  do not presume gravity is absent.

Local screenshots are retained privately under primary artifacts/feedback:
20261016-gsf-glow-sign-1.png SHA256
57da123ebced5cb174ef14f610daf2a3e3fd4c0bed3fcac229404802966eb767;
20261016-gsf-glow-sign-2.png SHA256
0f31eccb46462a90f5521dc26c99e2be0c208ebcec05dc212503c42c7ae3bb02.
The reporter's render mode/platform are unconfirmed, not ZaRR's CachyOS setup.

Root owns Q/typing UI investigation after the in-flight pixel-wheel correction.
Map/core owner finishes frozen B1 tests/immutable handoff first, then has only
read-only diagnosis grants: actual single-player confirmation, thrown-item trace,
then grouped GSF terrain/glow/sign source comparisons. No build interrupted,
production renderer/importer/world-item paths granted or published asset changed.
Added B4-B6 acceptance/triage rows and explicit single-player/source-prop notes.
All remain open. Agents use headless/ordinary input or bounded offscreen renders,
never interactive gameplay. Required production path grants follow actual repro.
