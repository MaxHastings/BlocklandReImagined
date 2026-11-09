# 2026-11-05 v0.4.4 Alpha publication

[Published v0.4.4 Alpha](https://github.com/MaxHastings/BlocklandReImagined/releases/tag/v0.4.4) on November 05, 2026 at 05:56:46 UTC.
The version tag names `cef86341bb39add681eb406eba5bd4a5e1480a1b`. The latest-release endpoint
returns v0.4.4. Maxwell approved preview.6 and authorized publication after
the remaining checks passed. The final route correction uses the resolved
movement controller and verified landing; it adds no content-specific routes.

- CI [41970639123](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41970639123) passed all seven executed jobs.
- `python tools/gate.py --full --release-content --jobs 4 cef86341bb39add681eb406eba5bd4a5e1480a1b` passed with frozen release inputs: 531 test binaries, zero retries, warning-free Clippy and content startup. In release-profile 32-bot scenes, Collage averaged 7.32 ms/tick against 8.00 ms; Afghan 4.50 against 5.50 ms. The separate capacity check admitted 64 creatures in 16 slots at 1.931 ms/tick against 2.000 ms.
- `save_corpus --ignored --exact the_fixed_save_corpus_hosts_like_the_game --nocapture` passed all 23 actual saves. Strict Add-On verification covered 102 receipts. Source audit resolved 572 named items across 214 seam rows.
- Native run [41971040701](https://github.com/MaxHastings/BlocklandReImagined/actions/runs/41971040701) passed Windows, macOS and Linux package and startup jobs. Downloaded packages passed file membership, length and SHA-256 verification, exact binary version and matching identity across 19 effective base packs. The downloaded Windows package also passed local `--check`; the packaged checker accepted all three private Speedkart folders.
- Public asset sizes and digests and the downloaded SHA256SUMS match the tested archives:

- `BlocklandReImagined-linux.zip`: 258,253,059 bytes; SHA-256 `6e2ac20778c874cbeb409790c4b84d227597813bccca7d9d249cfaabe01035c7`.
- `BlocklandReImagined-macos.zip`: 230,292,616 bytes; SHA-256 `3c5c2a23c4be2ab8a90b68e6aea9eec3c4894afcb6565ef6b3857f8f922fad02`.
- `BlocklandReImagined-windows.zip`: 260,911,055 bytes; SHA-256 `7491aa18c6f73dc4f11de4cea496cd1316f2b68f93b5425134a9c0885cd2c2cc`.

The earlier failed gates and retry-only render cases remain recorded in the
[gate findings](2026-11-04-v044-release-gate-corrections.md) and
[controller correction](2026-11-04-v044-route-controller-ownership.md).
The optional private tower probe requiring all three zombies to reacquire
before the first kill still fails on this source and the approved preview.6
control. Legacy compatibility warnings and the full alpha acceptance work remain open.
