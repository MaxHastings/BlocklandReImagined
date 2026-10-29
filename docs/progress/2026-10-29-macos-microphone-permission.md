# 2026-10-29 macOS microphone permission diagnosis

Maxwell reports no detected voice/microphone signal in the downloaded
BlocklandReImagined-v0.4.1-macos. Fetched origin/main and v0.4.1 without
changing the current codex/voip-lan-playtest checkout. The local session logs
identify v0.4.1 (4db779691), matching the fetched tag. Those logs contain only
the session header and provide no capture error evidence. Saved settings have
voice enabled, default input, push-to-talk, gain 2.070922 and threshold 0.
The source meter measures capture independently of the push-to-talk key.

Both v0.4.1 and origin/main (9898fc75) omit NSMicrophoneUsageDescription
from the generated app Info.plist. Apple documents this key as required for
microphone access. This is a confirmed packaging defect and likely explains
the report, but does not establish the actual downloaded app's permission
state or live capture result. The earlier private Mac VOIP wrapper included
this key; that change never reached the shared packaging script.

Added the purpose string before app signing and a permission-description
preflight to package verification. bash -n tools/package_mac.sh passed.
A temporary Python probe parsed the generated plist and exercised the actual
verification preflight: valid description accepted, absent/blank descriptions
rejected. Confirmed the tagged recipe lacks the key. No Rust code changed,
no build/game/microphone was launched, no push or release was performed.

macOS denied shell access to ~/Downloads (Operation not permitted), so the
downloaded app was neither inspected nor modified. Next: inspect/repair or
repackage the downloaded app with the key before ad-hoc signing, then Maxwell
reopens it, grants microphone permission and checks Options → Audio input
meter followed by ordinary push-to-talk gameplay. Local source changes alone
do not repair the downloaded release. Full alpha acceptance remains open.

## Authorized v0.4.1 Mac package correction

Maxwell then explicitly requested publishing a corrected v0.4.1 Mac release.
This authorizes the scoped replacement of its Mac ZIP and SHA256SUMS despite
the usual immutable-assets policy; the tag and Windows/Linux assets stay fixed.
Switched to codex/macos-microphone-permission from origin/main, carrying only
this thread's local changes and preserving the unrelated October 2 note.

Downloaded the actual published archive and SHA256SUMS into ignored
artifacts/v041-mac-microphone/original. Its original SHA256 is
d1d96b7ba6bca267b2988e32009ba933163cc338f0fa0b69be2b27201ded31ac.
Extracted public v0.4.1 source into the ignored evidence directory to run its
exact content/default-Add-On/menu-art verification without rebuilding anything.
Original archive verification passed all 13,420 files and its signature.
The actual Info.plist lacked the microphone purpose string.

Patched only that field, ran codesign --force --deep --sign -, regenerated the
manifest and repacked with ditto -c -k --keepParent. All file paths/content
match the original except Info.plist, CodeResources and embedded signatures
in the four executables. An initial byte-comparison assertion correctly
rejected treating those helper signatures as unchanged; signature removal also
retains the original __LINKEDIT virtual allocation size. Final comparison
removes each embedded signature and normalizes only that allocation field,
then requires every other executable byte to match. All four passed. No
instruction/data sections or bundled content changed. Evidence includes the
normalized executable hashes and exact changed-file list.

Corrected folder and final ZIP passed the patched permission preflight,
all 13,420 manifest hashes, v0.4.1 default Add-On/import/media generation
checks, curated menu images and strict deep signature verification. Packaged
--version reports v0.4.1 (4db779691). Packaged --check with a fresh isolated
state passed: 21 maps, 1,372 brick definitions, 35/35 pictures and 14 modern
light-parameter maps. It retains the same 226 content fallback diagnostics;
no window or audio device opened. bash -n, git diff --check and the documentation
unit suite passed. These are scoped packaging checks, not a full gate.

Published the Mac ZIP using gh release upload v0.4.1 --clobber, then uploaded
the updated SHA256SUMS and edited the release note with a dated correction and
microphone test instructions. The GitHub connector cannot upload release
assets, so the existing signed-in gh account performed those writes. GitHub's
post-upload asset digests match the locally verified files; the release ID,
source target and original Windows/Linux asset records are unchanged.

Corrected Mac ZIP: 224,895,391 bytes; SHA256
bf62a9be0ab648a4d54697243233a9c80eb4df4134f0c258ab4aba57f76de0c2.
SHA256SUMS digest:
207af92ec08ebc39dd45f2f0917222f5242e8324115a2aab01c0c84dda8d025d.
Actual public re-download verification is in progress below. Live microphone,
permission prompting and gameplay acceptance remain Maxwell's. Source patch
will be submitted separately; publication does not move the v0.4.1 tag.
