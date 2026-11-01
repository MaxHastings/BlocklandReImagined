# Preview Add-On health review

Reviewed the 46 `Warning: Add-On` lines in the existing preview.2 session log
`E:/Downloads/BlocklandReImagined-v0.4.3-preview.2-windows/logs/
session-20261101-010932.log`. The lines aggregate 1,076 reported import findings;
these counts are diagnostic findings, not a measured number of missing bricks.
Three reports say the brick catalog reader refused scripts with repeated
datablock names: Demians Complete Brickpack (270 findings), GIANT Brickpack v2
(460), and WedgePlus (9). Other warnings include unsupported preferences and
retained map objects. A successful startup does not make those warnings a clean
content acceptance.

The current converter intentionally rejects a repeated same-name datablock in
`crates/convert/src/catalog.rs::read_with_globals`; this explains the reported
refusals but does not establish the original scripts' intended conflict behavior
or the correct converted result. The configured search did not locate the pinned
originals for those three packs, so their exact source cannot currently be
reimported and compared here. Do not guess a duplicate-resolution rule, suppress
the warnings, claim all referenced bricks work, or infer these pre-existing
reports were caused by the latest work. Carry the limitation into final package
triage and release notes unless supported source and validation become available.
The preview's modified user package selection and older client/content also mean
this is not a substitute for checking the exact final package.
