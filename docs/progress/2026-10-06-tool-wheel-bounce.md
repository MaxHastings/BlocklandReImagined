# 2026-10-06 Scrolling tools no longer bounces back a slot

Max: scrolling to pick a tool or weapon sometimes feels off.

## Cause

The host answers an equip request at once (`server.rs`, `Message::Reply`)
but sends the new selection with its next update. When the reply came,
the client dropped its pending intent, and `Building::sync_tools`, run
every frame on the current view, put back the replicated selection from
before the switch. The HUD snapped back a slot until the next update, and
a wheel notch in that moment started from the old slot, so fast scrolling
skipped or bounced. v20's HUD selection is client-side and never bounces.

## Change

The accepted equip is kept (`accepted_equipment`) with the replica tick its
reply came at (`network::Event::Reply::tick`), and stays the intent until a
tool inventory from a later tick arrives. Updates and replies share the
host's ordered stream, so that inventory is the host's answer. From then on
the replicated selection is the authority again, as before.

## Evidence

- `cargo test -p bri-client --lib building::`: 32 pass, including the new
  `an_accepted_switch_holds_until_an_inventory_from_after_it`, which fails
  without the change (`[SetActiveTool(Some(0))]`).
- `cargo clippy -p bri-client --tests -- -D warnings`: clean.
- `cargo test -p bri-client --lib`: 475 pass; the 6 failures need a GPU
  adapter (none in the cloud machine).

Wheel input itself matches v20: whole notches (fractions accumulate), one
`scrollInventory` step per notch.
