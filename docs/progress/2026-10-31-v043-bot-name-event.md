# Native bot name event

Added `setBotName` to the existing brick-bot event catalog. It accepts bounded
text, uses the existing cleaned/unique player-name and admin/minigame update
path, and holds the requested name through ordinary brick reconciliation. An
empty value returns to the derived brick/kind name. The override belongs to a
brain life rather than a new save schema; a spawn event can author it again.
This native brick target is not claimed to be a drop-in implementation of the
original Bot-target event.

The builder integration test edits and fires an actual event row, sees the
renamed bot in session names, advances reconciliation, then clears it. The
existing persistent-battle lifecycle test additionally checks the control and
continued combat. Both targeted tests passed; the event catalog unit check
passed; grouped formatting and `cargo clippy --locked --profile playtest -p
bri-events -p bri-sim -p bri-chaos --lib --test bot_builder -- -D warnings`
passed. Initial compilation exposed a `bail!` return-type mistake and the
integration fixture initially looked for a name on `PlayerState`; both were
corrected before the passing tests.

Save/reload, replacement-life spawn rows, multiplayer name replication and
source Bot-target adaptation still require direct coverage. Other bot event
groups and exact-player tracking remain open.
