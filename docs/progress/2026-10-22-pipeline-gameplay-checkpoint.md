# 2026-10-22 Pipeline gameplay checkpoint

Engine main d07bc043fe26477b75cb061e1b4c1c39a07758ae contains the fall feedback,
independent upward spawn recovery, retained objective reservations, live guard
rediscovery and safe small-support walk-off fixes (PR293). Its full gate passed
539 test binaries with zero retries. Both 32-bot budgets passed: collage mean
3.99 ms against 7 ms, Afghan 2.91 ms against 5.5 ms. The merged branch is removed.
This completes the validation pending in the earlier dated spawn/guard receipt.

Private Add-Ons main 1137e3cae4164ef8539c5b03cf0de6592f8d7628 includes PR95/96:
placeholder retirement does not score or end the human round, scripted reload
blocks objective admission, queued deployment preserves current control, the
native FIFO reset event distinguishes roster assignment from late joining, and
held objectives release on death/disconnect/spawn. These are private policy and
conversion changes; no original assets or private rule code enter this repository.

The real-map three-player consumer passes required-valve victory, queued team
and SAW class changes, clean restart, death interruption and elimination,
late-join waiting/admission, and disconnect returning the real valve handle.
It uses an isolated position fixture and ordinary commands; it does not verify
walking routes, physical key delivery, visual fidelity or interactive acceptance.
A separate ordinary full-map 780-game-second run observes objective progress,
defense elimination and timeout wins, two resets and zero rule/bot failures.
All 15 bots move beyond spawn and defenders spread across live objectives.
Two late attackers still have 32/42-second no-route intervals in a narrow strip
between terrain and building. That route defect remains explicit.

Assembly prepares/checks lighting, native Add-On health is clean, and 33 Python
tests pass. Offscreen deployment shows weapon, objectives, ammo and clock;
no original visual comparison was performed. The broader private delivery plan
still requires Pipeline presentation, controls and kits, then Interdiction's
ordinary co-op slice. Maxwell owns all interactive acceptance. The full alpha
contract, source fidelity and outstanding interactive crash reports remain open.
