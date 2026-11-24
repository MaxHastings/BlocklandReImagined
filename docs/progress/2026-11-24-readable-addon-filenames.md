# 2026-11-24 Readable legacy Add-On filenames

Converted assets now take their filenames from their original Add-On member
paths. The importer preserves source directories where they fit package path
rules and uses source-path suffixes to resolve collisions. Stable IDs, original
member spelling and source SHA-256 still travel through the import report.
Brick catalog mesh paths remain flat because the catalog binds them by
filename. Generated placeholders have their own directory. Importer revision 4
refreshes tracked legacy conversions from sources still in the Add-Ons folder.

The importer library tests passed (78). The 41 cases in
`cargo test --locked -p bri-addon-import --test import --test installed --test borrowed_bricks --test hosted --test sample_map -- --include-ignored`
passed with the private reference content and original archives. The reimport
case edits a source texture and verifies stable filenames and IDs along with
updated manifest and source hashes. The borrowed native brick test now checks
the changed mesh bytes and vertex with the same filename. Its old assertion
expected a filename change. The initial hosted run lacked the checkout's
ignored content junction. The passing run had the same reference content at
both the environment path and the junction used by the hosted harness.

The v0.4.7 release regenerates bundled originals with this importer. The base
release content recipes have no changed inputs.
