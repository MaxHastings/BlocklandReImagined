# 2026-10-03 v0.2.3 Join content preparation before admission

Root-owned integration; stability/creator lane prepared `/tmp/bri-v023-join-preparation.patch` without editing existing source. Existing Add-On content preparation seam remains the owner/mechanism. No wire fields, message meanings, protocol files, permissions or Session admission/announcement policy change.

## Evidence and cause

Maxwell reported that several people joining by direct IP appear connected, then immediately left, then connected again. No corresponding connection log was supplied, so actual transport retries in those sessions remain unclassified.

There is a concrete ordinary bootstrap path producing that sequence: `Client::connect_fetching` first admits matching shared packages and later fetches client-only content, or downloads shared differences and admits its second handshake. Its App caller in `app/session.rs` then compares actual resolved brick/weapon/vehicle/bot extras. When engine content changed it calls `client.close()`, hands the selected package set to the existing preparation worker, and joins again. `Session::join_verified` has already entered the player, greeted/announced connected and spawned, and called package join hooks. Its disconnect really removes that player and broadcasts has left. The subsequent final admission announces connected again. This is real premature world admission, not a faulty chat string or probe announcement.

`Purpose::Download` already exits the server connection task without an Event::Join. The existing listing probe stops after Challenge and never sends Hello, so it creates no Session player either. Genuine transport-loss retries in `poll_network` remain separate and retain their current failure/reconnect handling.

## Proposed correction

Use the existing authenticated probe to bind all download/game connections of one attempt to the same certificate, then fetch/load offered packages over the existing Download path before any game admission. Retain ordinary shared-package checking/refusal and canonical accepted-differences admission. Loader calls can repeat when canonical shared-package removal or failed-load fallback requires another set; arbitrary load errors remain errors/diagnostics, while current joining-without fallback policy is retained.

The App makes its existing resolved-content reload decision inside that loader callback, before admission. It records the selected set in the existing pending reload slot and returns the typed local `JoinPreparationPending` control flow. Network fetching propagates this explicit deferral rather than treating it as a failed Add-On to bypass through fallback. The existing preparation/cancellation/install/fallback continuation performs the final cached join. It does not close a temporarily admitted player to prepare content. Session join/leave notices and package hooks remain actual player lifecycle events; no real disconnect is hidden.

This adds a bounded initial probe but avoids a full admitted world transfer and removal/rejoin for content-changing joins. The final join still validates package IDs/hashes against the live host. Current host pin/key/TOFU and UI identity-change checks are retained. No claim is made that a host changing during a full App reload gets a new cross-reload trust mechanism; saved/invite pins already apply normally.

## Regression boundaries and verification

- `a_client_only_content_load_failure_never_enters_the_game`: tiny real offered client-side content, real QUIC client, real loader failure preserved. ServerReport must record zero joins/resumes. Baseline test-only patch `/tmp/bri-v023-join-preparation-baseline.patch` uses no new type and is suitable against unchanged production. It targets the known branch where shared Hello is accepted before the loader runs.
- `deferred_content_preparation_announces_only_the_final_join_and_real_departure`: a real joined observer watches a candidate through shared missing packages, client-only missing packages, and an extra shared package requiring removal. Explicit preparation deferral produces no player admission. Final prepared IDs use verified cached content and require no additional loader callback. Observer sees exactly one Candidate connected/spawned, then one has-left message on a genuine close; ServerReport records exactly two joins, observer plus final candidate, and zero resumes.

Commands requested from root (lane had no compute lease; no Cargo/GPU/game run performed):

```sh
cargo test --locked -p bri-net --test package_sync a_client_only_content_load_failure_never_enters_the_game -- --nocapture
cargo test --locked -p bri-net --test package_sync deferred_content_preparation_announces_only_the_final_join_and_real_departure -- --nocapture
cargo test --locked -p bri-net --test package_sync
cargo test --locked -p bri-client --test add_on_fallbacks
```

`rustfmt --edition 2024 --config skip_children=true` passed on temporary proposed source and regression mirrors. `git apply --check /tmp/bri-v023-join-preparation.patch` passed against the shared tree when handed over. Runtime before/after evidence pending root. Human direct-IP checks remain Maxwell's responsibility; no interactive session was automated.

## Root verification and first-Hello correction

The unchanged-production baseline failed at the intended admission boundary: one join versus zero (`/tmp/bri-v023-join-before.log`). Root applied the production preparation path and queued the remaining observer regression. No after-fix runtime result is claimed yet.

Follow-up `/tmp/bri-v023-join-preparation-loaded-refs.patch` uses the preflight loader's returned canonical PackageRefs in the first Hello, rather than discarding them and offering the original stale set. This avoids a needless rejected pre-Hello attempt. If an extra shared package causes an ordinary refusal and a subsequent loader fallback clears fetched metadata, final unavailable diagnostics still compare offered shared identities with the final successfully loaded set. The server continues to reject shared extras before Session admission; real loader errors and the existing empty fallback remain visible. No wire/schema change.

Root after-fix verification: complete package synchronization suite passed 14/14 (`/tmp/bri-v023-join-after.log`), including no early admission/connected-left notification and the final real departure. The loaded-ref follow-up and incremental observer test were integrated by root. Human direct-IP playtest remains separate.

## Additional first-Hello and late fallback boundaries

Test-only `/tmp/bri-v023-join-preparation-final-tests.patch` strengthens the existing clean/stale download-and-join regression: exactly one loader callback and zero rejected Hellos prove that the first admission uses the actual prepared PackageRefs. It adds the late ordinary loader-failure boundary: first loaded set includes an extra shared package; that pre-admission refusal removes only the extra; a real subsequent loader error falls back to empty loaded content; unavailable must include both previously prepared actual host shared IDs.

Root's first final-suite run was 14 passed and one fixture failure (`/tmp/bri-v023-join-final.log`). The new test incorrectly reused `modded_server`, a fetch-only helper with a PackageShelf but an empty game Environment. Thus all downloaded shared packages correctly became Extra in Hello. Correction `/tmp/bri-v023-join-preparation-final-fixture.patch` uses the existing canonical join-fixture pattern with `ServerOptions.environment=environment.clone()`. No production change or weakened assertion: dropped must still be exactly local-extra, unavailable must match the final absent actual host shared packages, and the loader/admission/refusal counts remain exact. Corrected runtime result pending root.

Root corrected-fixture verification: the full package synchronization suite passed 15/15 (`/tmp/bri-v023-join-final-2.log`). The first-Hello canonical-ref assertion and late ordinary load-failure diagnostics both pass with the actual game Environment fixture. This supersedes the pending corrected result above.
