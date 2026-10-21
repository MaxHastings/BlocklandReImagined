> **Historical note. Written 2026-10-08 (the file's date in the cloud project folder, `v0.2.7/release-list.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**


- **Faster release check** (on main 3b45c5b):
  - The bot crate now builds optimized in tests, so bot tests run 3-4x faster and the soccer test no longer times out (f5543d6).
  - The gate reuses an earlier pass when only the commit message changed, or only docs outside crates/ (0f691e5, 86f8fe6).
  - The phase summary is now kept in the gate log (cfca982).

- **Teamkills, Close Quarters, fog** (on main a226c28):
  - Bots no longer shoot a teammate standing beyond their target: a missed shot's flight past the target is priced (05b2c89, 8c08ee3).
  - Close Quarters loads with HE Grenade off; saved items from an Add-On that is off are left out instead of refusing the load (a7820f6).
  - Distant geometry fogs toward the sky colour behind it, so tall far terrain no longer looks pale (c13ff91).
  - Not fixed: a bot driving a jeep can run over its own teammate (a_jeep_on_each_side).
