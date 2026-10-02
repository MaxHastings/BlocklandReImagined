# Zero quaternions read as no rotation

Brick_PlateHighRamps did not load in v0.1.14-alpha: the packaged `--check`
listed 35 problems, every `.dts` failing with "DTS: Zero quaternion" and its
textures with them. Its exporter wrote all-zero rotations on unrotated nodes.
Torque's `QuatF::setMatrix` turns a quaternion with no axis into the identity
matrix, so v20 drew these shapes unrotated.

`crates/convert/src/shape.rs` `rotation()` now returns the identity for a
quaternion of near-zero length instead of failing the shape. Test:
`a_zero_quaternion_is_no_rotation` (fails on the old code).

Not verified here against the real zip (no copy in the cloud checkout); the
Gate's packaged `--check` on the bundle is the proof. Its batch bundle check
should count every Add-On problem `--check` reports, not only brick geometry.
