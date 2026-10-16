# 2026-10-16 Vehicle landing observation fixture corrected

Root follow-up from public main1be98fd7 on codex/v031-integration-followup.
The known native standing/landing failure was a fixture observation ordering
error: authoritative grounded arrives one tick before the automatic collision
mount is delivered. Its previous branch checked mounted before an existing
30-tick settling interval, then incorrectly evaluated the standing-only path.

Move that existing interval before branch selection and assert the bounded
600-tick landing loop actually reaches grounded or mounted. No extra wait budget,
runtime change, standing duration reduction or hop assertion removal. Synthetic
still exercises the full roof standing/hop path; native recognizes its later
automatic landing mount. Earlier red/phase traces and corrected isolated clone
evidence remain in root artifacts, described in private-fidelity-and-landing-fixture.

Applied to the real tests/vehicles.rs after first public main landing. Removed
the obsolete standing test allowance from tools/gate-known-failures.toml.
The portal collision allowance remains unresolved and root-owned.

Validation on this root change:

- check.py quick --crates bri-sim:403 passed, zero failures, four existing
  ignored; two steps113s.
- check.py focused --crates bri-sim --depth0: formatting, all-target warnings
  denied Clippy and all sim ordinary tests pass; three steps441s. Scope is
  test-only sim plus removal of allowance, so no production dependents changed.
- cargo test --locked -p bri-sim --test vehicles
  standing_on_a_vehicle_does_not_seat_a_rider_but_landing_on_it_does --
  --include-ignored: native content and synthetic both pass,2/0/0,66 filtered,
  runtime1.52s. Logs artifacts/v031-landing-fixture-{focused,native}.log.

The quick and focused commands briefly queued on the same checkout Cargo lock;
neither build was interrupted. Initial prepared patch failed to apply because of
line endings, then the exact reviewed hunk was applied normally. No test waived.
This scoped correction still needs the next integrated candidate Windows/full
gate before another public main push. Full fidelity/release hold remains.
