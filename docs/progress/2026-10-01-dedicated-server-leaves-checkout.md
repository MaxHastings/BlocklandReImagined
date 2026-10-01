# 2026-10-01 Dedicated server leaves a checkout's content alone

`bri-server` always ran `defaults::install_from_checkout`, so the Gate's
smoke over the main checkout's content (a junction) copied our default
Add-Ons into `content/addons`, which shifted brick counts and broke
`content::tests::local_native_content_index_and_lazy_maps` in every worktree.

- New `bri_package::defaults::install_when_asked` (and `INSTALL_ENV`):
  installs only with `BRI_INSTALL_DEFAULT_ADD_ONS=1`. `bri-server` and
  `bri-client --check` both use it; a game run still installs as before.
- A release's content is packaged with its default Add-Ons, so a server
  unzipped on a VPS needs no setup (`install_from_checkout` was already a
  no-op there).
- Guard: `cargo test -p bri-net --test server_leaves_checkout`. Fails on
  48b1786ad ("bri-server wrote into the checkout's content: Installed the
  default Add-Ons brick_mirror, ragdoll, ..."), passes now.
