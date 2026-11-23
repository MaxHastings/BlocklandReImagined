# 2026-11-23 Background Voice Activation

Issue #450 traced to three window-focus gates in the client voice pump:
capture, outgoing frames and speaking activity. They now use the same
transmit decision. Voice Activity Detection can run behind other windows
on the existing independent worker. Push-to-talk still releases on focus
loss. Host mute, enabled voice, an accepted voice session and the selected
microphone mode continue to govern transmission. The Options hint describes
background transmission. The background audio mute controls playback.

`cargo test --locked -p bri-client --lib voice::` passed all 11 tests on
Windows, including the focus and moderation transition regression and the
device-free voice worker and loading handoff test. Microphone behavior in
a minimized real client remains an interactive check.
