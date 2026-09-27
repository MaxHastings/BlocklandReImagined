# Administration UI integration

The UI now routes the original single Admin menu and its login, ban, unban,
brick-management, map-change, server-options, and yes/no confirmation screens
through typed `AdminAction` requests. `AdminRole` contains only Player, Admin,
and SuperAdmin. Host auto-SuperAdmin status is represented in authoritative
state; it does not add a fourth role or a second menu. The native password and
host-options controls are explicit adaptations for settings that do not have a
verified matching stock dialog.

The host supplies `AdminSnapshot` with the caller's role, local-host status,
supported operation flags, current player roles, stable host connection IDs,
and optional settings. The UI uses those flags only to show available controls;
the host still has to authorize every action. Ban, brick-group, and map rows
return with the request ID that fetched them. Selection and confirmation store
stable IDs, and the UI rechecks the current snapshot before dispatch. Rejected
requests clear their pending state and display the host's sanitized reason.
Passwords are held in a redacted secret wrapper and their input fields are
cleared after submission or screen close.

The ban editor supports the source day/hour/minute ranges and Forever choice.
Unban, player kick, brick clearing, and map changes use the original native
Yes/No dialog skin. Player ownership, local players, SuperAdmins, bots, and
legacy LAN restrictions are considered when enabling operations. Server
options are validated before dispatch. The original defaults command stays
disabled until a host defaults adapter exists, avoiding a local-only preference
change that would look authoritative.

Validation performed:

- `cargo test -p bri-ui --lib --no-default-features`: 63 passed, 2 ignored.
- `cargo test -p bri-ui --test admin_screens`: 4 passed; the GPU render test is
  explicitly ignored by default because it needs the converted content pack
  and a headless graphics adapter.
- `cargo clippy -p bri-ui --test admin_screens --no-default-features -- -D warnings`:
  passed.
- `cargo test -p bri-ui --test admin_screens source_admin_screens_render_offscreen -- --ignored`:
  passed using `content/ui-pack-003`; eight source-skinned screens rendered
  offscreen at 1024x768. No game window, desktop input, or audio was used.

The ignored render check wrote review images under the ignored
`artifacts/native-admin-ui/` directory. These are derived UI previews, not
content for version control.

The normal App now maps authenticated Session snapshots and routes supported
actions over QUIC. Supported client operations are login, kick, ban/unban/list,
brick-group queries/clearing, global brick clearing and changing the Admin password. Initial
Admin/SuperAdmin passwords are configured before host admission; the separate
server join-password field still rejects explicitly until transport supports it.
Replies retain their originating UI request IDs, rejected passwords remain
rejections, and older snapshots cannot restore revoked roles. Unowned group0
remains a valid brick group. Highlighting has a distinct capability and remains
unavailable until its gameplay handler is installed.

The root native headless App startup test now hosts with Admin/SuperAdmin
credentials, receives host SuperAdmin state, opens the original Admin menu,
fetches brick-group and ban lists, changes the Admin password through the ordinary
UI action/QUIC reply path, and checks disconnect clears admin state. It passes
using the normal identity-proved connection, a persistent host store and an
isolated test state directory. The saved key survives disconnect/reload. This
test does not claim an interactive or two-player ban-screen playtest.
Five pure client adapter tests and six UI admin tests pass. The server's
separate Session/QUIC authority coverage is recorded in runtime-integration.md.
No interactive playtest has occurred.

Ban replies retain stable IDs, sanitize display markup and use the host's time
for remaining durations. A successful correlated unban removes its row; a failed
or unrelated reply does not. Stale list revisions cannot replace newer state.
App host and join use caller-owned `client.identity`; normal hosting loads
`administration.json` and refuses anonymous downgrade. Local keys are resettable
pseudonyms, not a public account system or proof of legacy Blockland identity.

Still required: remaining host settings/role configuration UI, map changes,
spy/camera, wand and other original administration workflows. These capability
flags are not advertised as working. The complete alpha administration
requirement remains open; the first building playtest has a smaller scope.
