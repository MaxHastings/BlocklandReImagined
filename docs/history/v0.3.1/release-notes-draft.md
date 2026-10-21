> **Historical note. Written 2026-10-15 (the file's date in the cloud project folder, `v0.3.1/release-notes-draft.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# v0.3.1 release notes (draft, collected as PRs merge into v0.3.1-experimental)

- #150 Players show on the list as soon as they connect, as "Name (loading)", and the auto admin line goes out at connect. Protocol change: host and joiners must both be on v0.3.1.
- #149 Maps can now come from Add-Ons: build one in Blender, import it with bri-import-addon gltf --kind map, and friends who join download it (guide: docs/modding/maps.md, sample: packages/samples/sample-map).
- #151 Chat links are cyan again (magenta while pressed), as in v20; the mouse tip no longer covers the chat input; letters like g and y keep their outline.
- #152 Destructo Wand hits on big builds no longer stall the server (under 1 ms instead of 20 to 65 ms), and a collapse sends at most 2,048 debris cues so the blast sound is not crowded out. Hammer and undo get faster too.
- #153 The net graph (Ctrl+N) no longer shows packet loss that never happened on jittery or far-away connections, and the game stops resending data that already arrived. A player's upload is capped at 24 KB/s, under v20's Broadband limit.
- #154 A host stall (loading, a save, a busy tick) no longer leaves players' moves refused until they respawn, which felt like lag building up over time, most of all while jetting.
- #155 Modders can check and run Rhai rules without Rust: the downloads now include bri-addon-check and bri-addon-run, a script's print shows in the host console, every script function is documented, and a new guide (docs/modding/rhai.md) teaches Rhai.
- #156 The six Modular Terrain (ModTer) Add-Ons ship on by default: all 322 bricks in the brick menu and a ModTer print tab of 22 terrain prints that tile and take the paint colour. Bricks with concave collision now load for any Add-On.
- #157 Vehicles cost far less bandwidth: players not riding a vehicle get a slim pose (4 KB/s instead of 11 KB/s per moving vehicle); riders still get the full pose. Protocol change: host and joiners must both be on v0.3.1.
- #158 Bricks from Blender: bri-import-addon gltf --kind brick turns a glTF model into a brick Add-On that plants, paints, saves, syncs to joining players and takes events (paint colour only for now, no menu icon).
- #160 /reloadrules: the host reloads edited Rhai rules scripts in a running game and keeps its state; a broken edit is refused and the old script keeps running.
- #162 Add-On rules can play, stop, loop and fade music for one player or everyone, under the Music volume; Add-On music tracks may run up to 10 minutes. Protocol change: host and joiners must both be on v0.3.1.
- #164 Rules cameras can orbit and follow a physics object, and cup and trigger zones notice objects as well as players. Protocol change: host and joiners must both be on v0.3.1.
