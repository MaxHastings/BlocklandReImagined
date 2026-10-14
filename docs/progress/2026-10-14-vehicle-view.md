# 2026-10-14 Vehicle poses: watchers get only what draws the vehicle

Follow-up to `2026-10-14-net-graph-loss.md`, Max's go-ahead in the packet
loss thread.

A driving vehicle's pose went to every player at 40 Hz with the crew's
prediction state in it: spin, mouse steering, the tyres' state, the driver's
acknowledged move and steering prefs, a mount's motor. Only the crew's
clients predict from those (`App::predict_driven`), as only a player's own
client reads their jump timers (`RemotePose`).

- `Datagram::VehicleView` ("w", `protocol-changes/vehicle-view.md`): the
  drawing fields, with position, rotation and velocity quantized as
  `RemotePose` does and the identity passage frame left out.
- `StateStream::set_crews`: the host sends the full `Datagram::Vehicle` to the
  vehicle's occupants (from the vehicle listing it last sent) and the view to
  everyone else.
- `Replica::viewed_vehicles`: vehicles whose newest pose was a view. A
  player seated before the host's crew list includes them waits for a full
  pose before predicting, instead of predicting from blank state.

Measured (`bandwidth_table`): a driving four-wheel vehicle costs watchers
101 B a pose (4 KB/s at 40 Hz), down from 276 B (11 KB/s); its crew still get
276 B. Budget: `a_driving_vehicle_costs_watchers_under_its_byte_budget` (130 B).

Tests: `stream::tests::a_vehicles_crew_gets_its_full_pose_and_everyone_else_a_view`;
`vehicle_trails` now runs guests through the view's wire round trip.
