# Offline effects extension importer

This standalone command consumes the verified `effects-pass-004` pack, rechecks
its source/image hashes against the designated original installation, and emits
a fresh native runtime pack. It never interprets TorqueScript. Regex extraction
records literal relationships and inherited literal datablock fields; those
records do not execute callbacks, conditional code or gameplay state machines.
The prior stock effect conversion remains the authority for particles/emitters.

```powershell
cargo run --manifest-path crates/fx-import/Cargo.toml -- 'E:/Downloads/B4v21Launcher/versions/Blockland v20' content/effects-pass-004 .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs content/effects-runtime-pack-002
python crates/fx-import/verify_pack.py content/effects-runtime-pack-001 'E:/Downloads/B4v21Launcher/versions/Blockland v20' .research/v20-dso/server/scripts/allGameScripts-Vanilla.cs artifacts/native-effects-runtime/independent-verification.json
cargo test --manifest-path crates/fx-import/Cargo.toml
cargo clippy --manifest-path crates/fx-import/Cargo.toml --all-targets -- -D warnings
```

The output directory must not exist; its parent must exist and resolve outside
the original install. Reads are bounded, ZIP matches are unique and source paths
cannot traverse or follow aliases outside the original root. Outputs retain
original compressed image bytes and explicit provenance. The Python verifier
independently checks image headers, hashes, primary original bytes and all native
references using only the standard library.

`manifest.json` schema 1 contains image records, the recovered emitter-level
alpha override, source relationships and explosion groups. Explosion lights are
lowered to explicit native curves with deterministic group lifetimes. Source
relationships retain `v20/<datablock-class>/<symbol>` owner IDs; consumed resource
IDs retain existing `v20/emitter/...`, `v20/light/...` IDs and add
`v20/explosion/...`, `v20/explosion-light/...` namespaces.

The importer carries the old conversion diagnostics in `source-proof.json` and
keeps unsupported debris/model relationships and the one unverified lifetime
default in `manifest.unresolved`. It does not promote installed community packs
to vanilla scope: it reads only the source list already established by the stock
conversion, then compares those inputs with the designated primary installation.
