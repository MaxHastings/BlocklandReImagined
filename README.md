# Blockland ReImagined

A new native implementation of the Blockland v20 experience. Rust and wgpu
provide the foundation, with original content migrated through separate tools.

The first Windows **core building playtest**, `2026-09-27-01`, is packaged. It includes the
native menus/maps, player movement, building/tools, supported brick events,
save/load and basic direct-IP multiplayer. It is not the complete vanilla alpha:
combat, vehicles, minigames and remaining fidelity/streaming are unfinished.
See [playtest instructions](docs/PLAYTEST.md), [known issues](docs/KNOWN-ISSUES.md),
and [the current playtest gate](docs/playtest-contract.md).
See [the alpha contract](docs/alpha-contract.md),
[implementation plan](docs/implementation-plan.md), and
[progress/evidence](docs/progress.md).

Original game assets are local inputs and are not included in source control.
Research copies are kept under ignored `.research/`; generated local content,
diagnostics and build packages live under ignored `content/`, `artifacts/` and
`dist/` respectively.

With Rust 1.93 or later:

```powershell
cargo test --workspace --locked
cargo run -p bri-render --bin gpu_probe
cargo run -p bri-physics --bin physics_probe
```

For the Windows playtest executable without a separate MSVC runtime dependency:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target x86_64-pc-windows-msvc -p bri-client --bin bri-client
```

The content bundles are local outputs, not Git downloads. The provided local
playtest package carries the selected converted packs; source builds need those
packs generated separately using the conversion guides below. Run
`bri-client --check <content-directory> <state-directory>` to validate startup
without a window/audio device, or `--run` to play. Packaging and checksum
verification are documented in [package layout](docs/playtest-package-layout.md).

The probes run headlessly and create reports under `artifacts/preflight`.
Physics currently uses Rapier; see [the decision](docs/physics-decision.md).
For terrain/brick conversion and catalog extraction, see [content conversion](docs/content-conversion.md).
For native authority, events and BLS migration, see [world state](docs/world-state.md).
For the headless QUIC host, replication and measured limits, see [networking](docs/networking.md).
For the expanded default brick catalog and pending behaviors, see [building simulation](docs/building-simulation.md).
For original particle/light conversion and remaining runtime work, see [effects conversion](docs/effects-conversion.md).
For the native screen layer and client integration boundary, see [the UI guide](crates/ui/README.md)
and [integration evidence](docs/ui-handoff-status.md).
For the development client, persistent map renderer and current gameplay binding
limits, see [native client integration](docs/native-client.md).
For original brick overlays/prints and color evidence, see [brick materials](docs/brick-materials.md).
For agreed event quality-of-life work and later scalability considerations, see
[event modernization](docs/event-modernization.md). Modding support is outside
the alpha; decisions about it follow the complete vanilla base.
