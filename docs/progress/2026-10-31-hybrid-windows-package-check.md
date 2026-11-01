# Hybrid Windows package and Afghan saves

At `51c95808a`, built the five shipping executables with `BRI_VERSION` set to
`v0.4.3-preview.3` and the locked playtest profile. The normal Windows packager
assembled the private hybrid candidate under `diagnostics/hybrid-package-check`
without the private-preview prior-import bypass. `-VerifyPackage` accepted
13,458 manifest files, the default Add-Ons and ten reference menu images. The
packaged client `--check`, run hidden and waited on with an isolated state
directory, exited zero. Its health report contains 19 problem entries; its
fallback report has 226 entries representing 425 occurrences. Known duplicate
brick-catalog refusals remain among the health findings. This package has a
different enabled Add-On selection from the earlier preview, so the counts are
not an improvement comparison or a clean health result.

The actual packaged files show Zombie `cooperative_stack=true`, Shark
`cooperative_stack=false`, and Slayer `reset_when_empty=false`. To test the
saved minigames without touching the verified package, copied its content into
a private test directory and used the same enabled-package list as the prior
successful Afghan fixture. With the current headless scenario probe, both
actual saves loaded with no set-aside bricks. TDM Afghan had 15 living bots
before leave, 16 after, 16 moved while the owner was away, and the owner
rejoined. TDM Afghan with Zombies had 16 before and after, ten moved, and the
owner rejoined. Each observation spans two simulated seconds away, not a
long-running multiplayer test. Both runs emitted partial-route warnings and
the first also emitted a stall warning; the pass does not establish captures
or perfect navigation.

The first attempts against the untouched release defaults failed because the
saved game did not include the owner. Enabling only Slayer and the Hole/Zombie
packages still failed; matching the complete previously used enabled-package
list, which includes Blockhead Bot and other packages, produced the above
results. Thus this verifies the saved scenario with its enabled Add-Ons, not
automatic activation of disabled Add-Ons by loading a save. No user save or
verified package was modified. Reports and isolated health data stay private
under the diagnostic directory. Other platform packages, final release gates,
and the fresh imports' single recovered core-script limitation remain open.
