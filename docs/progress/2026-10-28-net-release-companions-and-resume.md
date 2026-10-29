# 2026-10-28 Release join companion metadata and resume cursor

The v0.4.1 coordinator assigned two failures from candidate `0eff36e30` for scoped investigation, and approved the client loader path alongside networking. No shared gate tree, main branch, release tag or build dispatch was modified here.

## Causes and corrections

The retained-brick resume failure at loopback.rs:3202 compared two independently sampled update cursors (56 versus 55), not a brick count or World-frame count. The comparison client is not read while Fresh joins/leaves and the server advances metadata; identical worlds need not have identical cursors. The test now deliberately submits and observes a chat update through Fresh while A stays unread, proving the newer non-brick update. Resume must contain that chat, cover A's observed cursor, preserve exact world equality, and still transfer exactly two World frames. Both exact world/transfer assertions remain. No expected number was blindly changed.

A separate unnecessary control message was found: never-voiced sessions sent an empty VoiceRoutes table on each new peer's admission. These hosts now send no initial routing state; existing active tables still send revocations. The voice-off regression asserts revision stays zero; voiced sessions retain the normal initial route handshake. This is not claimed as the proven cause of the original cursor timing difference.

The imported New Duplicator declares `tool_newduplicator-rules` as a companion, and its 13 downloaded bindings target that host-only package. Both the real client `mods::load_fetched` and the soak join helper only accounted for host-only dependency metadata. The downloaded tool was therefore rejected and fallback omitted its client data.

One shared `net::packages::declare_host_references` helper now accounts for declared dependencies and companions. It reads at most the existing 8 MiB manifest bound and parses the ordinary typed manifest; it preserves any actual local/downloaded package side and directory. Missing declared references become metadata-only Server entries for client catalog validation. They are never downloaded or executed, and commands remain checked by the host's existing authoritative runtime. Undeclared bind targets still fail. Both runtime and soak paths invoke this same helper.

## Scoped evidence

- `cargo clippy -p bri-net -p bri-client --all-targets -- -D warnings`: passed.
- `cargo test -p bri-net --test package_sync`: 17 passed, one native-content case ignored in the default run. Covers declared companion/dependency metadata, preserving an actual client companion, and rejecting an undeclared target.
- With `BRI_CONTENT` set to this lane's generated content, `cargo test -p bri-net --test package_sync downloaded_newduplicator_controls -- --ignored --nocapture`: passed and validated all 13 actual imported ToolNewDuplicator bindings without loading server rules. Original source installations remain unchanged; no original content is committed.
- `cargo test -p bri-net --test loopback a_resume_over_kept_bricks`: passed with deliberately newer chat metadata and exact transfer/world checks. `cargo test -p bri-net --test bandwidth a_resume_over_kept_bricks_sends_no_world_when_nothing_changed -- --exact`: passed, retaining the zero-world-transfer case.
- `cargo test -p bri-net --test voice`: all five passed, including voice-off, proximity, flood, 32 simultaneous speakers and impaired transport.
- `cargo test -p bri-client --lib mods::tests::`: both existing runtime loader cases passed.

Native logs are `artifacts/net-release-{clippy,package-sync,newduplicator,resume,bandwidth,voice,mods}.log` in plastic-destruction-playtest. No full gate/soak was started; the coordinator owns the final integrated gate and publication. No wire schema, package identity, content asset, save format or dependency changes.
