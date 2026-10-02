# Water only from the water bricks (2026-10-02)

Max found the Halloween pack's coffin acting as water (he splashed inside
it). Steam's Brick_Halloween coffins set `isWaterBrick = true` ("so we can
plant inside it") and the game made every brick with that field a water
zone. In v20 the zone comes only from Brick_Large_Cubes' script:
`brick8xWaterData`, `brick8xWaterRiverData`, `brick8xWaterRapidsData` and
`brick32xWaterData` each call `createWaterZone` in their own
`onTrustCheckFinished` (read from Max's v20 and Steam copies). Nothing
checks `isWaterBrick` generically, so v20 never floods the coffin.

`Definitions::load` now makes `Special::Water` from those four stock bricks,
as it already does for the checkpoint, teledoor, chest and pumpkin. The
sim and net test stand-ins for water carry those ids, like the chest's.

Guard: `definitions::tests::only_the_water_bricks_script_makes_water` (an
Add-On brick with `iswaterbrick` is not water; fails on the old rule).
`cargo test -p bri-sim` and `bri-net --lib` pass; clippy clean.
