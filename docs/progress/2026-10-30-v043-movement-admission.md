# v0.4.3 movement admission: burst retry starvation

The release goal remains active. This corrects a demonstrated input-admission
failure; Maxwell's intermittent host-local walking/jetting delay remains open
until a private preview and relevant capture establish its cause and acceptance.

## Evidence and correction

The preserved match 132 contains 257,398 failed movement attempts for guest
owner 28, beginning at host tick 263,937, long after its last skipped clock ticks
(970/978/993). The attempts include redundancy retries, not that many distinct
lost controls. Reconstructing the 48-token admission budget from recorded Tick,
SkippedTicks and valid Movement calls matched every admission result without a
model mismatch. The guest's produced-sequence/host-tick offset stays approximately
constant during the failure; steady client clock drift is not required.

A link burst can exhaust the bounded allowance. Previously a refused valid
sequence was not classified as received. Every later datagram retried old
controls first and spent new credit on them, keeping fresh controls behind
indefinitely even after both clocks resumed at the same rate. The previous
host-clock-skip credit correction addresses a different trigger and stays intact.

Valid controls now receive one admission decision: admitted once or dropped once.
Rate-limited copies cannot repeatedly compete with fresh input. Validation stays
before the sequence mark, the allowance is unchanged, and dropped controls neither
drive the motor nor acknowledge ahead of queued controls. Later consumed sequence
numbers acknowledge the gaps normally. No wire field or schema changed.

## Checks and limits

- Baseline prediction target: 11 passed, two existing timing probes ignored.
- New burst/redundancy regression failed against the original implementation
  with 2,400 late refusals. Its corrected run passes: no late refusals and the
  worst late sequence trail is at most two ticks.
- Full prediction target after correction: 13 passed, two existing timing probes
  ignored. Includes malformed sequence poisoning, duplicate/flood bounds and
  acknowledgement ordering.
- Replay target: all 11 passed. This checks new-build deterministic replay, not
  replay parity with old recordings whose admission behavior intentionally changes.
- `cargo clippy --locked -p bri-sim --all-targets --profile playtest -- -D warnings`
  passed without warnings.
- `cargo fmt --all` encountered Windows command-length error 206. Scoped
  `cargo fmt -p bri-sim` succeeded; the release-wide formatter must use bounded
  package batches on this machine.

Commands above used the canonical v043-integration worktree and playtest profile.
Private baseline/red/green logs live under the preserved release evidence's
`diagnostics/` folder. `cargo test --locked -p bri-net --test hardening_net movement
--profile playtest` passed both the wire-shape and hostile-datagram movement cases.

The old host-stall test's assertion that disabling skip credit causes permanent
starvation is obsolete after this fix. Replaced that counterfactual with a direct
credit contract: skipped real time admits its controls without simulating the
motor, while the uncredited case still reaches the flood bound. The credited
end-to-end host-stall case remains. No failing regression was skipped or weakened
to conceal a remaining failure.

Next: complete transport checks, retain lightweight local movement evidence for
the first preview, and continue independent bot lifecycle corrections. No
interactive game, GPU campaign or soak was launched. This is scoped evidence,
not a full release gate or human acceptance.
