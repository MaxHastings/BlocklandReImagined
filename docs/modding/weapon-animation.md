# Native weapon animation from package rules

A server rules package with the `player` capability can admit one finite native
weapon animation together with optional magazine and image-state changes:

```rhai
let queued = start_weapon_animation(player, resource, sequence, nonce,
                                    expected_rounds, rounds, image_state);
```

`resource` is the actual registered native weapon body resource, such as
`example:body/rifle`, provided by the package or an installed dependency.
Its verified bytes must match the resource currently registered for the actual
weapon. `sequence` selects a supported finite, nonlooping native clip. The host
sets rate 1 and tween 0 and resolves the actual weapon slot and instance.
`nonce` is a positive integer that increases for this package's weapon actions
within the player's life. A duplicate or stale nonce is refused.

`expected_rounds` checks the current instance's loaded rounds; `rounds` sets its
loaded rounds; `image_state` sets its image state. Each accepts `()` to omit the
check or write. Writes use the current ordinary right-hand weapon instance;
unsupported magazine/state or stale instance checks refuse the action. Rounds
must be between 0 and 100000 and still satisfy that weapon's native magazine
rules. Resource, sequence and image-state strings are bounded to 128 bytes.

The call must originate from the server's original ordinary image command.
Typing the same command or invoking it from a HUD does not supply that authority.
Queued commands retain their original owner, inventory instance and image mount;
a replacement of the same content ID does not make an old command current.

The returned integer is a queued operation handle, implicitly required. The
script may update its own declared state, for example reserving a spare magazine,
but the entire call must contain exactly this one world operation. Nested Add-On
calls, other operations, emitted events and entity variable writes are refused.
State validation and native admission happen before the script's speculative
state is published. A refusal therefore keeps the package state, magazine and
image state unchanged. The handle is not a playback token or completion result.

## Observe an admitted action

Opt in through the package's server behaviour:

```json
{ "schema_version": 1, "script": "main.rhai", "on_weapon_animation": true }
```

The script must define `fn on_weapon_animation(player, info)` with two arguments.
The host calls only the package that admitted the action, at the end of a step
after animation reconciliation. The hook may update its own declared package
state. It may not queue world operations, call another Add-On, emit events, or
write entity variables. Hook failure discards its speculative state.

| `info` field | Value |
|---|---|
| `token` | JSON string containing the actual native PlaybackId, preserving all u64 identity fields |
| `binding` | JSON string containing the original actual owner/item/image binding, preserving generation fields |
| `nonce` | The package's admitted positive nonce |
| `target`, `slot` | `"weapon"` and the actual inventory slot |
| `resource`, `sequence` | The admitted native resource and resolved clip sequence |
| `outcome` | `"started"`, `"native_natural"`, `"interrupted"`, or `"cancelled"` |
| `authority_tick`, `delivery_tick` | Decimal strings preserving the native event and observation ticks |
| `phase`, `reason` | The native record's phase and reason |
| `custody_valid` | The original live owner/body and inventory instance still hold the action |
| `current_selection_applicable` | The original binding also matches current selection and image mount |
| `current_action_applicable` | The owner epochs remain live and this nonce is still the package namespace's latest admitted action |

Host tools and the package observe the same bounded native record independently.
Draining one consumer does not steal delivery from the other. Unread records
still count toward native admission capacity. Package delivery is attempted at
most once, including failures and exhausted work shares; there is no retry queue.
A zero-duration clip can deliver both started and native_natural in one pass.
Changing a live rules script abandons that script's old observation routes while
preserving existing clocks and unread host evidence.

After admission the weapon clock follows inventory custody, independently of
current selection or mounting. Holstering can make current selection inapplicable
while the original inventory clock continues. Natural clip exhaustion is an
observation only: it does not invoke a source game's Finish routine, unlock fire,
exchange magazines again or refund ammunition. Source-specific completion policy
needs its own verified implementation.
