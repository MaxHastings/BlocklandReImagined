# Destruction release recheck

At integration commit `1f1a39016`, reran the relevant destruction owners after
the latest bot lifecycle changes:

- `cargo test --locked --profile playtest -p bri-client --lib effects::tests`:
  five passed; the established converted-content and timing cases were ignored
  by this default invocation. The converted-content lifecycle case was then
  run explicitly with `--ignored --exact` and passed.
- `cargo test --locked --profile playtest -p bri-client --lib audio::tests`:
  23 passed, with one established converted-content case ignored. This includes
  brick audio following actual debris and source restoration.
- `cargo test --locked --profile playtest -p bri-sim --lib
  session::destruction::tests`: 24 passed, including partition attachment
  identity, restoration, and actor/vehicle source suspension.
- `cargo test --locked --profile playtest -p bri-sim --lib
  destroyed_source_suspends_pickup`: one passed.
- `cargo test --locked --profile playtest -p bri-net --test replication`:
  23 passed, including checkpoint and restoration of attachment metadata and
  malformed-metadata atomicity.

Together these exercise the server, replicated representation, and client
owners, but they are separate test fixtures, not a single end-to-end rendered
multiplayer observation. They do not establish the final pixels, actual
packaged content, or every network delivery timing. No interactive test was
performed. Release-wide validation and the remaining acceptance work stay open.
