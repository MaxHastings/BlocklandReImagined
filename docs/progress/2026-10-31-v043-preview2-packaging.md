# Current preview packaging and prioritization

Maxwell requested a preview containing the full current work and asked that
remaining work be prioritized by gameplay impact, with bounded investigations
and unresolved minor or vague defects reported rather than over-polished.

At 614c1e675, built all five shipping executables with
`BRI_VERSION=v0.4.3-preview.2` and `cargo build --locked --profile playtest`
for their respective packages/binaries. Windows packaging rejected the main
checkout's older Add-On bundle because it lacks current generation receipts.
Rebuilding with the current importer and read-only v20 inputs imported the
first 19 originals, then stopped: no locally found `Weapon_Package_Tier1.zip`
matched pinned SHA-256 a500fd2c5e46f90c29e167656807cde95fdee352d99144e617a1176e88dcbc22.
Three differently hashed installed/archive copies were correctly rejected.
The preview.1 imported packages and this checkout's installed copies predate
the current importer recipe, so they cannot satisfy the packager's current
content verification. No fresh preview folder was delivered. Find a verified
source or other current verified bundle; do not silently bypass the receipt or
substitute unpinned source. The previous preview remains intact.

A bounded archive search found `Weapon_Package_Tier1 (v4).zip` at the Blockland
Archivers, but its SHA-256 is `9da5bea14fd4d689385a69e8a8eb35452eb01d086197c48cdf4846eb1b5b3119`,
matching an already-rejected local copy rather than the pin. The prior import's
report identifies its pinned source as a different revamped edition, so the
archive is not evidence that the two are interchangeable. The downloaded
candidate is private under `E:/Downloads/BlocklandReImagined-Release-Evidence`;
it was not installed or committed. The private-preview workflow can reuse the
verified prior converted package as recorded in the separate packaging handoff.
