# 2026-10-17 AA sprint/aim private playtest

Maxwell explicitly resumed the saved interaction test and requested a runnable
playtest. Engine651efb2ff361579dde657b54d4023b2ee313bab7 supplies the reviewed
three-file optional image-state Zoom gate. Local root integration is preserved
on codex/v033-aa-gameplay-integration / draft publicPR187; no main push, full
integrated gate or publicv033 release is claimed.

AA private green359dfaa and packaginge2ed61dc696a5e71bfbab76ab82f54edb96dbc33
preserve actual App/QUIC/GPU runs133217/133637/134044/134236: scope-state,
ordering/apex/ambient, lifecycle, and original sight/no-profile controls all
exit0 with scopedClippy0. All five Draw/Lower/Hold/Raise/Reload states now clear
scope and return FOV90 with visible FP geometry; Ready/Fire/WaitRelease preserve
sights, accepted shot/audio and held-RMB reaim. Native preview adaptations and
source-policy limits remain unchanged; original full-ammo strict reds remain.

The matching release build completed successfully. A fresh sibling folder is
staged at C:/Users/Maxwell/Desktop/Games/BlocklandReImagined/dist/
AA-Sprint-Aim-Preview-651efb2f. Client SHA256 is
7b12fdb050cb02c7e7e099889c68029d23ce0a0dca8c2184e926543beb5dc592.
The development stamp prints dev-%cs (651efb2ff): the old Git date format is a
cosmetic placeholder, while the exact hash and private preview labeling hold.
No public official artifact claim is retained for the locally built client;
base tool/content provenance is named separately.

Root independently ran artifacts/verify_aa_sprint_preview.py:13670 manifest
files verified,363 tested candidate package files matched, selected AA packages
enabled and non-AA selection preserved. Actual headless startup exit0,22maps,
1381brickdefs, Add-On health:no problems. Own receipt and log are primary
artifacts/aa-sprint-preview-root-verification.json and
artifacts/aa-sprint-preview-root-startup-check.log. Version/check probes create
only expected diagnostic logs in the NEW folder. The old source content/exe/
manifest/selection matched their pre-stage values; source userdata was not read
or copied. Fresh aa-user-data remains separate. No visible game was launched.

Manual card: Launch-AA.cmd, local AA Pipeline(work in progress), /aasoldier.
HoldO+W for forward M16 sprint; Shift crouches. HoldRMB for sights, keep it held
while sprint starts/stops and during the Light-bound reload animation. LMB fires.
The new profile clears sights in barred states and reaims when Ready returns.
See AA-START-HERE.md in the new folder for setup and remaining limitations.

Live feedback remains open: ultrawide side masks, scoped hip-position flash,
intermittent body scale, standing corpses, TP/FP muzzle elevation mismatch,
state-triggered breath/grunt sounds and a proposed contextual-controls UX.
No speculative fixes or new engine scope were added. Reload/ammo, movement,
support query, lighting/collision and multiplayer fidelity remain bounded native
preview adaptations. Interactive acceptance belongs to Maxwell.
