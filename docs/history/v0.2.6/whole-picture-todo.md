> **Historical note. Written 2026-10-07 (the file's date in the cloud project folder, `v0.2.6/whole-picture-todo.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# For the whole-picture review (coordinator note, 16:25Z)
Max asked (cmsg_01GZRn7g8JiQ6aV1cQgSj22D1uS3Yci4eSdgjScjt1SptK): "After all this done will we be satisfied with the bot npc foundation?"

In the whole-picture review, answer directly whether the foundation is done. List each remaining
STRUCTURAL gap (not missing features) with a one-line reason. Keep it honest and short.

Candidates so far (verify each against the final main):
- mood not yet consolidating variation sources (drift/boredom/effectiveness/goof)
- native vs scripted weapon paths still split: item_attacks native-only; threat reads
  (contest.rs ~80, team.rs ~471) don't use attack_of; tactics::rate ignores push
- nav: stud grid A*, no ladders/vertical moves
- loose pusher bound in spots (push = whole health; most v20 guns push, so the bound rarely skips)
- in-process minidump can still deadlock for non-crash-crate threads (not bots, but foundation-adjacent)
- duplicate mechanisms anywhere in the ~26k-line bot module (sweep for these)
