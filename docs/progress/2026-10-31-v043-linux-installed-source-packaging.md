# 2026-10-31 Linux packaging from an installed source list

The Windows preview correction already refreshed shipping defaults when its
source content list was copied from an installed release. Linux instead copied
those same folders a second time and appended duplicate package entries. This
would either fail copytree or emit conflicting identities. Mac already selects
base-role packs separately and appends its canonical shipping defaults.

Linux now composes canonical shipping defaults before selecting source folders,
skips the replaced source copies, and copies each refreshed default exactly
once. Disabled defaults remain installed but absent from the enabled list;
unrelated source packages retain their order/metadata. A shared pure helper in
addon_bundle rejects mismatched identity/directory pairs and duplicates rather
than silently resolving them. Neither source content nor source package lists
are rewritten. Package/wire schemas do not change.

`python -m unittest discover -s tools -p test_release_package_defaults.py` passes
three cases covering fresh dependency order, installed source refresh, companion
side/version, disabled default removal, idempotence, source immutability and
identity/directory/duplicate refusal. Embedded Python in the actual Linux and
Mac recipes compiles. These are scoped checks; native archives/CI/startup and
the older Windows generated-default fixture correction remain release work.
