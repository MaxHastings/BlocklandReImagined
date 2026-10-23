# Remove the shared-load all-features soak timing assertion

The gate for `65c3424b86f23bfb72217d463e6391146ed46cd3` exposed another
demonstrated flaky assertion. `all_features_soak_short` failed at 107.5 ticks/s
while the gate ran 16 test binaries and clippy; it passed alone on retry. The
original run had zero host step errors, both connected clients, answered
commands and the expected bots/vehicles. The failure was solely its 110 ticks/s
wall-clock threshold, not a broken gameplay or replication assertion.

Remove that timing assertion from the functional soak. Keep measured tick times
and rate in its diagnostic report and retain all host/client, package download,
command, replica, bot and vehicle assertions. The gate's separately executed
release performance budgets still enforce simulation cost on the real maps.
The parent candidate passed both: collage 3.70 ms against 7.00 ms; afghan 2.81 ms
against 5.50 ms. This follows Maxwell's request to remove flaky tests without
discarding the soak's useful functional coverage.

Evidence: `C:/Users/Maxwell/Desktop/Games/.bri-gate/logs/65c3424b86f2.log` and
the integrator's ignored `.research/pipeline-integration/owned-and-mover-gate.log`.
Quick passes (64s, combined client/net edits). Focused net/dependents pass in
760s; client focused checks pass separately in 149s. The functional soak's
assertions remain intact. The integrated gate is pending.
