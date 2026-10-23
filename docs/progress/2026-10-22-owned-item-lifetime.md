# Mission-owned pickups keep their identity while their provider maintains them

A private co-op consumer could advance a case objective after its physical item
had expired through the ordinary ten-second dropped-tool lifetime. Logical
objective completion alone concealed the missing item. Its source also
distinguishes a placed model used by a player from a dropped item recoverable
by touch; that distinction belongs to package policy.

The generic `renew_drop(id, seconds)` operation renews an existing package-owned
pickup for 1..600 seconds from now. It reuses the existing `player` capability,
creation ownership and live-drop budgets. The runtime changes only the existing
expiry deadline: identity, pose, velocity, appearance and pickup data remain.
Stopping renewal permits ordinary expiration. Player-thrown items do not become
owned by a package merely because it defines the item. Ordinary v20 dropped
tools retain their ten-second default.

The existing serialized drop deadline and renderer opacity calculation carry
the updated value; no saved-state or wire field shape changes. The operation
is internal to the package runtime, not a new network command variant.

Two real-session public regressions pass: an ammo pickup survives its original
deadline and is collected once with retained data; an unmaintained renewed item
expires; invalid lifetimes, unknown IDs and an unowned player-thrown item are
refused without changing its deadline. Quick checks pass (94 seconds). Clippy
with warnings denied passes; the first focused attempt was stopped before its
integration run could overlap a temporarily staged private consumer. The final
focused run started with those temporary files removed and passes in 720 seconds
(six check steps, including integration tests of eight crates and client unit
tests). Log: `owned-item-life-focused-final.log`.
After the short-mover replication correction, the combined quick tier passes
in 71s and focused passes eight steps in 1126s, including 28 crates' integration
tests and client/render units (`owned-and-mover-focused.log`).

The private position-only consumer now uses the authored placed model and
source pickup cylinder, takes it through ordinary Use, drops it on carrier death,
keeps the same physical item beyond its original 30-second lease, and recovers
it by ordinary touch. The surviving partner reaches radio and extraction;
duplicate placed/dropped models are checked. It passes in 75.60 seconds with
zero rule/bot failures. Two humans and a deliberately small NPC budget isolate
interactions; this does not establish walking routes, network replication,
combat difficulty, attachment presentation or source physics fidelity.

Local logs: `.research/pipeline-integration/owned-item-life-*.log`; private
`mission-input-placed-dropped-case.log`. The integrated gate and Windows CI
remain required before landing. The full alpha contract stays open.
