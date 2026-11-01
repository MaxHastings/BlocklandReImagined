# Collage performance budget decision

Maxwell reviewed the 32-bot collage measurements and explicitly requested an
8.00 ms budget rather than further tuning a small overage. The gate's collage
threshold is now 8.00 ms; Afghan remains 5.50 ms. The gate still requires a
release build and rejects a measured mean above its selected threshold.

The prior 7.68 ms and 7.21 ms measurements exceeded the then-current 7.00 ms
limit and remain recorded as failures under that earlier limit. They fall
below the newly requested threshold, but neither is a completed full-gate
pass. The experimental navigation optimization was removed. The final gate
must run on the updated commit and report its actual measurement; the change
does not claim the p99 spikes or navigation stalls are gone.
