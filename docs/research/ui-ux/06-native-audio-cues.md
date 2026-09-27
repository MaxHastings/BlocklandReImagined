# Native UI audio cues and preferences

Implemented 2026-09-26 in `crates/ui/src/ui.rs`, `view.rs` and
`screens/options.rs`. This is the UI-side handoff to the root-owned native audio
runtime; it does not create an audio device or add an audio dependency to the UI.

## Host interface

```rust
// Public type in bri_ui::ui:
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSound {
    pub profile: &'static str,
    pub trigger: &'static str,
}

// Drain once per host frame, independently of UI requests:
for sound in ui.drain_sounds() {
    audio.play(sound.profile, bri_audio::Placement::Listener)?;
    // Alternatively use audio.play_trigger(sound.trigger, Placement::Listener).
    // Play one route, not both.
}
```

`Ui::drain_sounds() -> Vec<UiSound>` is a local cosmetic outbox. It allocates no
request IDs and creates no pending permission/acknowledgement state. The outbox
retains at most128 intents in order. Further intents are discarded and counted
by `Ui::dropped_sounds() -> u64` (cumulative, saturating); the counter is available
for host diagnostics. Draining frees capacity. Gameplay requests/settings are
independent and are never discarded by this bound.

## Original cue mapping

Source: `.research/v20-dso/client/scripts/allClientScripts-Vanilla.cs`.
Main-menu handlers14226–14294 and escape-menu handlers9328–9388 call their
individual notes from `onMouseEnter`, gated by `$Pref::Audio::MenuSounds`.
The UI audit already records the same behavior in `01-screens-and-flows.md` §1
and `03-assets-and-adaptations.md` §4. Audio profiles are declared at78–128 and
resolved by the generated audio bank.

| Controls | Original notes |
| --- | --- |
| MM_TutorialButton, MM_StartButton, MM_JoinButton, MM_PlayerButton, MM_OptionsButton, MM_DemoButton | Note3Sound through Note8Sound |
| MM_QuitButton, MM_AboutButton, MM_CreditsButton | Note0Sound, Note1Sound, Note2Sound |
| EM_Options, EM_PlayerList, EM_MiniGames, EM_AdminMenu, EM_SaveBricks, EM_LoadBricks, EM_Disconnect, EM_Quit | Note0Sound through Note7Sound |

Each intent pairs `NoteNSound` with `ui.menu_note.N`. A note is emitted only on
entry into its visible, active control on the appropriate menu, using committed
preferences. Remaining inside a control, repainting, ticking, clicking or keyboard
activation does not replay its hover note. Pointer movement over a modal dialog
clears the covered view's hover state; real reentry after dismissal emits a fresh
note. Focus loss likewise clears hover. This uses model input routing only.

`UiUpdate::PlantError` emits `AudioError` / `ui.error` only when committed
`$Pref::Audio::PlantErrorSound` is enabled. This follows `handlePlantError` at
7193–7195 and remains independent of MenuSounds. The default is false.

No generic click or message-box sound was invented. Source GUI profiles explicitly
have empty `soundButtonDown` and `soundButtonOver` fields (19615–19616,
20110–20111,20378–20379,20400–20401). The unused `AudioButtonOver` profile refers
to a missing original clip; it is not substituted with another sound.

Actual server message cues (MsgError, join/drop/admin/brick-clear/upload/process
complete/item pickup) need the corresponding authoritative event, not inference
from chat strings, player snapshot differences or tool inventories. Root's server
presentation-cue adapter owns those triggers. Generic rejected UI requests and
message boxes do not masquerade as source MsgError events.

## Options commit boundary

The following existing authored checkboxes are now enabled through the supported
local preference list:

| Preference | Source default | Host consequence after commit |
| --- | --- | --- |
| `$Pref::Audio::PlayMusic` | true | Set native music enabled state. |
| `$Pref::Audio::MenuSounds` | true | UI gates subsequent menu-note emission. |
| `$Pref::Audio::PlayBrickPlantSound` | true | Client gates native brick planting cue. |
| `$Pref::Audio::PlayBrickMoveSound` | true | Client gates native brick movement/rotation cues. |
| `$Pref::Audio::PlantErrorSound` | false | UI gates subsequent plant-error cue. |

Root seeds missing preferences from `SoundBank::defaults()` before `Ui::new`,
while preserving persisted values. Options resolves preference identity by the
case-insensitive authored variable, including controls with duplicate names.
These changes do not enable unsupported audio drivers or unrelated settings.

Draft edits change only the Options draft. Done follows the established commit:
three `UiAction::SetVolume` actions (`master`, `shell`, `sim`) and one
`UiAction::SaveSettings` carrying committed preferences. Root applies PlayMusic
and client sound gates from that saved settings payload. Cancel/Escape discards
draft audio values and emits no volume/settings changes. The original drag test
tone/live volume preview is deliberately not added; preserving this native
commit/cancel boundary was part of the assignment.

## Verification

```powershell
cargo test -p bri-ui --no-default-features --lib
cargo clippy -p bri-ui --no-default-features --lib --tests -- -D warnings
```

Results:61 tests passed,0 failed;2 preexisting locally converted-content render
tests remain ignored. Six new audio tests cover the17 note mappings, repeated
hover/reentry, click/message silence, disabled/hidden/muted/modal controls, focus,
independent plant-error preference, observable overflow, seeded checkbox defaults,
draft/cancel isolation and Done's committed SaveSettings/volume actions. Clippy
passes with warnings denied.

Validation used pure UI/model tests. No visible window, OS input automation,
speaker playback, original installation writes or synthesized resources.
Actual device playback and root's frame/notification wiring remain outside this
UI patch; Maxwell retains interactive testing responsibility.

## Progress handoff

Root may append: Added original MM/EM hover-note and optional plant-error intents
through bounded `Ui::drain_sounds`; enabled the five stock audio preference toggles
with existing Done/Cancel semantics.61 UI tests pass (2 existing render tests
ignored), Clippy passes. Device audio remains in root-owned client integration.
