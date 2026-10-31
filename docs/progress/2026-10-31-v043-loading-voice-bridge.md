# 2026-10-31 Loading voice ownership and UI handoff

Continue the accepted-loading voice path through the real application. The
resource worker now publishes the authenticated accepted connection, then the
verified gameplay connection before world chunk assembly. A weak publisher is
bound to the logical join and an attempt generation: cancellation/supersession
revokes it, owner changes fail closed, and an old loading publisher cannot replace
gameplay. Planned Add-On preparation retains the accepted lease; ordinary
session teardown revokes it synchronously. UDP voice reading starts before world
assembly and its tasks are owned by the link lease, alongside control reading.
A rejected/aborted early observer closes the pending gameplay connection rather
than leaving an admitted player behind.

The independent pump keeps its listener optional while loading, presents eligible
spatial voice centrally until an actual listener exists, and continues to require
host-authorized routes/range. Loading presentation reads the independent control
snapshot, preserves the accepted roster, and shows a plain-text voice conversation
caption and horizontal names. The caption is explicitly voice-specific; it does
not misrepresent typed-chat scope. Keyboard and side-mouse PTT use loading utility
input handling, with held PTT retained through loading/download/game screen
replacement and ordinary release/focus handling retained. Gameplay controls stay
blocked during loading. The existing closed-writer typing correction also handles
the closed arrival-to-game transition through the gameplay command queue.

Changed-area validation so far:

- `cargo check --locked --profile playtest -p bri-net -p bri-client -p bri-ui
  --all-targets`: pass.
- `cargo test --locked --profile playtest -p bri-net -p bri-client -p bri-ui
  --lib voice`: 36 pass (13 client, nine net, 14 UI), including actual QUIC
  admission/observer rejection cleanup and frame-independent verified handoff.
- The new loading PTT integration initially fails at key release: no false edge.
  Code review found held-key bookkeeping in the chat-overlay branch rather than
  the loading branch. Moved it to the loading branch; the exact regression and
  three existing loading checks pass with unchanged assertions via `cargo test
  --locked --profile playtest -p bri-ui --test runtime_input loading`.
  The resulting `target/playtest/deps/runtime_input-d9b8b1438c1a2d2f.exe` also
  passes all 40 input cases, including focus/dialog/gameplay boundaries.
- `cargo test --locked --profile playtest -p bri-net -p bri-client -p bri-ui
  --lib accepted_loading_chat_routes_ui_typing_and_survives_transport_close`:
  the one selected App case passes against the actual loading transport.
- `cargo test --locked --profile playtest -p bri-net --test voice --test
  voice_policy --test chat_while_joining --test protocol_changes`: 27 pass
  (five voice, 15 policy, five joining chat, two protocol).
- `cargo test --locked --profile playtest -p bri-net --test loopback
  join_reports_the_world_download_in_bricks`: one pass.
- `cargo clippy --locked --profile playtest -p bri-net -p bri-client -p bri-ui
  --all-targets -- -D warnings`: pass. `git diff --check`: pass.

These are 105 distinct selected cases, including the full 40-case input binary;
the four loading input cases were also rerun separately after the correction.
They are scoped checks, not a full workspace or release gate.

No hardware microphone, visible game, interactive automation or new Maxwell
playtest is used. Compile time is retained in private logs; no retry was used to
hide a runtime failure. Only the identified PTT implementation was corrected.

This is not completion of loading voice. Captured-source authorization across
conversation changes still needs its own host grant and capture-epoch treatment,
including partial PCM/codec state. Combined PCM continuity and remaining modder
seams are open. Final release/platform checks and the broader v0.4.3 scope remain
open. Private command logs are in the planning evidence diagnostics directory.
