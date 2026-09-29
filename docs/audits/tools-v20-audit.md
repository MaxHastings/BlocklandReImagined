# Tools and weapons against v20 (v0.1.2)

Every stock v20 tool and weapon, checked against our runtime. Sources: the
recovered server scripts (`allGameScripts-Vanilla.cs`), the stock weapon and
item Add-Ons in the reference install, and Torque's `ShapeBase` image code
as preserved in the OpenMBU reference. Behaviour is paraphrased; no script
text is copied here.

Verdicts: **match** (already right, with the evidence), **fixed** (wrong
before this branch, fixed and tested here), **deferred** (still different,
with the reason).

## The engine rules every item shares

These three rules sit under every tool and weapon. Before this branch two of
them were wrong, and that is what broke the spray can and every switch made
while holding fire.

| Rule | v20 | Ours before | Verdict |
|---|---|---|---|
| The trigger is the player's held button | `Player::updateMove` copies the move's trigger 0 into image slot 0 every tick. It is a held level, not an edge, and it does not belong to the image. Mount a new image while the mouse is down and that image sees the trigger at once. | The trigger lived inside the mounted image and every mount started it released. The host also threw away queued presses and releases on every equip, colour change, drop and loadout change. So scrolling the spray can, or switching tools, while holding stopped firing until you clicked again. | **fixed**: `Actor.trigger` in `crates/weapons/src/runtime.rs`, copied to the right hand every tick. Queued edges are kept through switches (dropping a queued release would now leave the trigger stuck down), so only death, disconnect, a lapsed input lease and boarding a gun seat reset it. |
| Mounting waits for `allowImageChange` | `setImage` stores a new image as `nextImage` while the held image's state forbids changes (the gun mid-shot, the sword's swing). It mounts on the next state that allows them. The inventory selection changes at once. | The equip was refused with an error, so a tool picked mid-shot never came out and the HUD snapped back. | **fixed**: `NextImage` plus `Advance::Switch`. The waiting image mounts on the tick the old one reaches an allowing state, instead of entering that state, and runs that same tick. |
| Putting tools away never waits | `setImage` delays only a *new* image; unmounting (`unUseTool`, drop, `ClearTools`) is immediate. That is why the rocket launcher needs `minShotTime`: its own comment calls it a guard against an equip/dequip exploit. | Putting away mid-shot was refused like an equip. | **fixed**. `minShotTime` is kept on the actor, so re-equipping still cannot fire early (stock rocket test). |
| Mounting the image already held is a no-op | `setImage` with the same datablock only cancels a waiting `nextImage`. Each palette colour is its own `color<N>SprayCanImage`, so another colour mounts afresh. | Every `UseSprayCan`/`EquipTool` remounted, replaying Activate (the can's shake sound) and resetting the state. | **fixed**: same image and same paint keep their state. |
| Trigger transitions are level-based | `transition.trigger[triggerDown]` is checked every tick the state is not waiting on its timeout. | Same. | **match** |
| `waitForTimeout` / `allowImageChange` defaults | Both default to true in `ShapeBaseImageData`. | Importer and `State::authored` default both to true. | **match** |
| Alt trigger (slot 1) | Blockland never sends move trigger 1; right click is jet. Akimbo's left gun only sees `onFireAkimbo`'s one-tick pulse. | Same: the left hand's trigger is cleared every tick; jet is a sports trigger only. | **match** (`akimbo_fire_rate_over_seconds_matches_v20`) |

Default picked without asking: a mount that is still waiting when the player
picks the held image again is cancelled, as Torque's same-datablock path does.

## Item by item

The state machines below come from the recovered scripts. Timeouts are
seconds. At our 120 Hz a 0.04 s timeout is 5 ticks. Where a stock test
exists it is named, and it runs with the converted pack (`--include-ignored`).

### Building tools

| Item | v20 behaviour | Ours | Verdict |
|---|---|---|---|
| Hammer (`hammerImage`) | Ready, then PreFire (0.01) on press, Fire (0.2), then CheckFire, which swings again while held and goes to StopFire (0.2) on release. Every state allows image changes. The swing ray reaches 5 (5.5 looking down). | Same states from the pack. The host resolves the swing on `onFire`. | **match** (`tools_swing_only_when_held_…`, `hammer_ranges_…`) |
| Wrench (`wrenchImage`) | PreFire then Fire (0.5), then CheckFire waits for release: one swing per click, no auto-repeat. | Same. | **match** |
| Printer (`printGunImage`) | Fire (0.25) goes to Reload on timeout; either returns to Ready on release. One print per click. `UsePrintGun` toggles it off if it is already held. | Same states. The printer is an inventory tool here. | **match**. Toggling off by selecting it again is a client selection concern with no difference seen. |
| Spray can (`blueSprayCanImage`, colour copies) | Activate (0.5, no wait) goes straight to Fire if the trigger is down; otherwise CapOff (0.2, no wait, fire on trigger), then Ready. Fire loops every 0.04 while held and allows image changes; release goes to StopFire, then Ready. `UseSprayCan` mounts the colour's own image with the trigger untouched, so scrolling while held keeps spraying the new colour. It also recolours a held ghost brick and remembers the colour. | Before: every colour change started the trigger released, so spraying stopped (Max's report), and reselecting the same colour replayed the shake. | **fixed** (`a_held_trigger_keeps_spraying_through_colour_changes`, stock `a_held_spray_can_keeps_spraying_through_scrolled_colours`, host `scrolling_the_spray_can_while_holding_fire_keeps_spraying`). Ghost recolour and remembered colour: **match**. |
| FX cans (flat, pearl, chrome, glow, blink, swirl, rainbow, stable, jello) | Same state machine as the spray can. `UseFXCan` mounts the FX image and forgets the current colour can. | Same, plus the fixes above. | **fixed** (same model) |
| Painting disabled in a minigame | `UseSprayCan`/`UseFXCan` do nothing; turning painting off mid-game puts a held can away. | `ensure_may_build(Paint)`; the minigame's `unmount_paint`. | **match** |
| Brick tool (`brickImage`, `horseBrickImage`) | Ready, then Fire (0.25) on trigger; StopFire returns to Ready only on release: one swing per click. Its `onFire` deploys the ghost where it lands. | The host mounts `brickImage` while a brick is chosen; the click also deploys the ghost on the client. | **match** for a click. A trigger already held while you *pick* a brick now swings the brick image on the host (v20 does too), but our client only deploys the ghost on a fresh click: **deferred** to the first-person brick lane (`claude/fp-brick-animation*`), which owns brick placement feel. |
| Player wand / Destructo Wand | Ready loops (its timeout field is misspelt, so it loops at once) and sparkles; PreFire (0.1) forbids image changes; Fire (0.2) then CheckFire repeats while held. | Same, including the sparkling Ready self-loop. PreFire now defers a switch instead of refusing it. | **match**, plus the switching **fix** |

### Stock weapons (Add-Ons)

| Item | v20 behaviour | Ours | Verdict |
|---|---|---|---|
| Gun | Activate 0.15; Fire 0.14 (no image change, ejects shell); Smoke 0.01; Reload waits for release, so it is semi-automatic. | Same. A switch during Fire now waits for Smoke; before, it was refused. | **match** (`gun_is_semi_auto_…`), switching **fixed** |
| Akimbo guns | Right gun fires on press; on release, FireAkimbo pulses the left gun's trigger for one tick. Left gun on slot 1 comes and goes with the right. | Same; the left image mounts and unmounts with the right, including a deferred switch. | **match** (`akimbo_fire_rate_over_seconds_matches_v20`) |
| Bow | Fire 0.05, then Reload 0.5 (both wait, no image change), then Check fires again while held: automatic, about one arrow per 0.55 s. | Same. | **match** (`bow_auto_and_rocket_cooldown`) |
| Rocket launcher | Fire 0.1, Smoke 0.1, CoolDown 0.5, Reload waits for release; `minShotTime` 700 ms guards re-equip. NoAmmo/Ammo states exist but stock `ammo` is blank, so `onMount` sets ammo on. | Same; `last_shot` lives on the actor, not the image. | **match**. The test now puts it away mid-shot and re-equips with the trigger held; there is still no second rocket. |
| Spear | Charge (0.7, releasable) goes to Armed; releasing early aborts (0.3); release from Armed throws. No image change from Charge to the end of Fire. | Same. Picking another tool mid-charge now waits for Ready; before, it was refused. | **match** (`spear_short_charge_…`), switching **fixed** |
| Sword | PreFire 0.1, Fire 0.2, then CheckFire (swings again while held); StopFire 0.2. PreFire, Fire and StopFire forbid image changes. | Same. | **match** (`sword_and_broom_…`), switching **fixed** |
| Push broom | Fire loops every 0.2 while held; every state allows image changes. | Same. | **match** |
| Horse ray | Like the gun with a 0.5 s Smoke. | Same. | **match** (`horse_ray_transforms_…`) |
| Keys (red, yellow, green, blue) | PreFire, Fire 0.15, CheckFire waits for release; one check per click. | Same. | **match** (`key_and_skis_…`) |
| Skis | Fire on press toggles skis and returns to Ready on release. | Same. | **match** |
| Sports balls (basketball, dodgeball, football, soccer) | Charge/Armed/Fire throws; basketball swaps to its shoot image, which keeps the held trigger. | Same. The swap now inherits the held trigger through the shared model instead of a special copy. | **match** (`sports_charge_throw_…`) |

## Situations

| Situation | v20 | Ours now | Verdict |
|---|---|---|---|
| Hold fire, scroll paint colour | Keeps spraying, now in the new colour. | Same. | **fixed** |
| Hold fire, switch to another tool | The new tool sees the held trigger: the hammer swings, the gun fires once, the bow keeps shooting. If the old image is mid-shot, the switch waits for it. | Same. The client keeps its "fire is down" flag through the switch, so the release still reaches the host (`a_trigger_held_through_tool_and_colour_switches_is_still_released`). | **fixed** |
| Hold fire with empty hands, then take out a tool | The move trigger is held, so the tool fires as soon as it is ready. | The host keeps a held trigger with nothing in hand (`a_trigger_held_with_empty_hands_fires_the_tool_taken_out`). But an empty-handed click is sent as Activate, not as a trigger, and a UI action carries only one command, so the client does not tell the host the button is held. | **deferred**: needs the client to send the trigger alongside Activate. That is a command-flow change for its own review. Rare in play. |
| Put tools away while firing | Immediate, and the trigger stays held. | Same. | **fixed** |
| Release during a waiting switch | The new image mounts with the trigger up and does not fire. | Same (`a_release_before_the_switch_is_kept`). | **fixed** |
| Drop the held tool while firing | `DropTool` unmounts at once. | Same. The queued trigger edges are no longer thrown away. | **match**, edge handling **fixed** |
| Death while holding | `onDisabled` sets the image trigger up and drops a held ball. | Trigger released, queue cleared, ball dropped. | **match** |
| Corpse's hand | `onDisabled` does not unmount images, so the body keeps holding its item until removed. | The item is put away at death. | **deferred**: presentation only. It needs the corpse renderer to keep the image and a visual check by Max. |
| Respawn while still holding | The new player gets the held move trigger, but holds nothing until a tool is chosen. | The trigger starts released after death; the client still sends the release when the button comes up. | **deferred** (tiny edge: only a tool chosen before letting go of the respawn click differs) |
| Minigame reset / join / ClearTools | Players respawn, or `ClearTools` unmounts slot 0 at once. | Loadout replacement unmounts at once; queued trigger edges are kept. | **match** |
| Board a vehicle while holding | Passengers keep firing their tools (their moves still drive their player). The Tank and cannon gun seats put tools away and fire the gun. | Before: boarding any seat released the trigger. Now only gun seats do; they also put tools away, as before. | **fixed** |
| Light key while holding | `serverCmdLight` only toggles the light (never while dead) and never touches images. | Same. | **match** |
| A press with no release before it | Impossible with a mouse: every press follows a release. | A dialog can take the mouse-up (the wrench's opens mid-click), so the host can see two presses in a row. The second is read as a fresh click: it lets go for one tick, then presses with its own aim. Without this, a wrench click followed by the printer's click never reopened the print selector (the Gate's `app_flow` failure). Pinned by `a_press_whose_release_was_lost_is_a_fresh_click`. | ours only (keeps v20's feel when a release is lost) |
| Teleport lockout / lapsed input | Not in v20. Ours refuses presses briefly after a teleport and releases the trigger when a player's input stops for half a second. | Kept: a safety net for a lost release. | ours only |

## Nearby lanes

- `claude/fp-brick-animation-col45e`: first-person held-brick animation and
  placement effects. It touches only client rendering files, with no overlap.
  The held-trigger brick-pick case above is theirs.
- `claude/gun-script-api-2a4up5`: adds `WeaponsWorld::swap_image` beside
  `mount_image` in `crates/weapons/src/runtime.rs`. Merging it next to this
  branch is mechanical. Its image goes through the shared `mount`, so it
  inherits the held trigger. Two notes for that lane: its doc says v20's
  `mountImage` ignores `allowImageChange`, but Torque's `setImage` defers
  a new image while the state forbids it (only unmounting is immediate).
  Also, it should clear a waiting `next` image when it swaps.

## Protocol and saves

No wire protocol change: the trigger and equip commands are unchanged, and the
replicated `MountedImage` view is unchanged. Weapon-runtime saves gain two
fields with serde defaults (`trigger`, `next`); older saves load with the
trigger released and no waiting image.
