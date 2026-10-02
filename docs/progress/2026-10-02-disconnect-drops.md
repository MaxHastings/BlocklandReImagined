# Disconnects in mini-game fights (v0.1.12 playtest)

Max was dropped twice in mini-games: once fighting a bot with the bow
("Request Rejected: Not connected"), once when the Steel Ball ran him over
("Connection Failed: Invalid item drop view").

- Cause: an item the world puts down (an Add-On rule's `drop_item`, the
  `spawnItem` event) has no thrower, `ActorId(0)`. Every client's check of
  the weapons view refused a drop whose source was 0, so the first such
  drop (a mini-game death drop) disconnected everyone. The host's own check
  never ran on its view. Now `ActorId::NOBODY` names it and the check
  accepts it. Guard: `item_hooks.rs`
  `an_item_a_rule_drops_is_one_every_client_accepts` (fails on the old code
  with "Invalid item drop view").
- The bow screenshot showed only the second symptom: the held trigger was
  released to the host that had gone, which answered "Not connected" in a
  Request Rejected box on top of Connection Failed. A session that ends now
  forgets held controls instead. Guard: `runtime_input.rs`
  `a_trigger_held_when_the_game_ends_is_not_released_to_the_gone_host`.
- The bow disconnect's own reason is inferred to be the same drop (no log);
  an over-the-network bot fight with the bow was not reproduced here.
