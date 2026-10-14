`Notice::Panel` (new): a panel an Add-On shows a player (`PanelShown`: its
package and `bri_package_runtime::panel::Panel`), or `None` to close it.
`Command::Panel` (new): what the player did in it (`PanelCommand`).
A panel list's `PanelItem::drag` lets its rows be dragged to another such
list, and `PanelEvent::drop` (`PanelDrop { from, row, at }`) carries one.
