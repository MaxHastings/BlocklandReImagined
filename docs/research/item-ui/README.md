# Authoritative native item HUD and controls

The normal client projects `View.tools[local_player]` into the five-slot HUD.
Slots contain stable native item IDs, never client-granted items. The immutable
production catalog uses all 17 weapon-pack003 names plus the four verified core
tool names and item-presentation-pack003 icons/tints. Synthetic core metadata
remains available to asset-independent controller fixtures.

## Integration

- `ItemUi::new(&ItemAssets, &[(String, String)], &bri_ui::pack::Pack)` builds
  immutable metadata. `catalog()` installs it once through
  `Building::set_tool_catalog`. Duplicate IDs, coverage mismatch, invalid names,
  omitted current items and active catalog replacement are rejected.
- App retains `Arc<ItemAssets>` (`item_assets()`), registers original RGBA icon
  textures through `ItemUi::register_icons(RenderContext)`, and resets their
  registration on GPU teardown/recreation. No original reader runs in the client.
- `Building::sync_tools(&ToolInventory)` validates replicated inventory before
  publishing `UiUpdate::Tools` and `SetActiveTool`. Reused slots resolve their new
  stable ID. Unknown IDs fail rather than acquire a guessed tool behavior.
- `command_sent(request, command)` and
  `command_finished(request, command, accepted)` reconcile local selection
  intentions with authoritative slots. An older reply cannot roll back a newer
  intention, even after the newer reply arrives. Pending intentions are bounded
  and invalidated when their slot changes identity. Rejection restores the known
  host selection. Host snapshots remain authoritative after replies.
- `UseTool` emits `EquipTool { slot }`. Hammer, Wrench and Printer route by ID,
  regardless of slot number. Other native weapons send `WeaponTrigger` on both
  held-fire edges. Repeated same-state edges are suppressed. A later release is
  retained across a pending or rejected equipment switch; an older equip reply
  cannot discard a newer fire press.
- `GameAction::DropTool` emits `DropTool { slot }`; only the host update removes
  the HUD item. No client `Give` path exists. Failed outbound requests roll back
  pending selection state. If a release cannot enter the transport queue, App
  disconnects so actor teardown cancels the held trigger. Disconnect clears the
  controller and all five HUD slots.

## Original evidence

Reference: read-only vanilla v20 installation, recovered scripts under ignored
`.research/v20-dso`. Line references describe these recovered files, not a
claim that script reconstruction is the original engine implementation.

| Source | Behavior used |
| --- | --- |
| `server/scripts/allGameScripts-Vanilla.cs:5278–5317`, `ServerCmdUseTool` / `ServerCmdUnUseTool` | Select the item occupying the requested slot; unuse clears the mounted tool. |
| Same file `5319–5361`, `ServerCmdDropTool` | Drop the selected inventory item, then notify inventory changes. |
| Same file `9010–9033`, `Armor::onCollision` | Existing item identity prevents duplicate inventory pickup. Native host owns that decision. |
| `client/scripts/allClientScripts-Vanilla.cs:7205–7260`, `handleItemPickup` | Update per-slot tool data, item label, authored icon and color shift. Missing icon uses the first UI-name letter from Print_Letters_Default; failing that uses the original unknown brick icon. |

Seventeen items have dedicated original icons in the native presentation pack.
Basketball, Dodgeball, Football and Soccer Ball have no dedicated icon in that
source catalog; their original UI names begin with `Ball`, so they use the
existing `add-ons/print_letters_default/icons/b` UI asset. This preserves the
source fallback without fabricating sports icons. Original unknown-icon fallback
is also supported when the first-letter asset is absent.

## Reproducible checks

Run from the repository root, with native pack003 assets already converted:

```powershell
cargo test -p bri-client --lib building::tests
cargo test -p bri-client --lib item_ui::tests -- --ignored --nocapture
cargo test -p bri-client --lib app::tests::native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture
cargo clippy -p bri-client --all-targets -- -D warnings
```

The controller tests cover replicated pickup/drop and slot reuse, stable tool
routes, newer/older equip-reply ordering, rejected switches, held-fire release,
queue failure and invalidated pending selections. Catalog checks compare all 21
names/tints and original icon bytes. The bounded offscreen GPU check renders all
21 icons twice across renderer recreation, checking missing resources and
nonempty pixels in every cell. It does not create a window or play audio.

The actual-pack App test starts a headless host with null audio, checks the
replicated starter inventory, selects Printer then Wrench through normal UI
requests, drops Wrench through the normal command path, verifies the replicated
drop and empty slot, and checks disconnect cleanup. Pickup-specific slot changes
are controller-model tests; this is not an interactive pickup playtest.

## Remaining integration and acceptance

Wand selection is represented faithfully, but use explicitly reports that native
destruction is not connected; it is never relabeled as Hammer. Sports alternate
jet/special action adapters, full host sports rules, mounted-item animation and
3D first/third-person presentation remain separate integration work. Normal
muzzle positions remain provisional until native model mounts drive them.
This change does not claim those adapters or all vanilla gameplay are complete.
Maxwell owns interactive visual, input and feel acceptance; these automated
checks do not mark the alpha contract complete.
