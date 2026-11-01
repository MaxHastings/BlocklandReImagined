# Loading communication network recheck

At the current integration checkout, `cargo test --locked --profile playtest
-p bri-net --test voice --test voice_policy --test chat_while_joining --test
protocol_changes` passed all 27 tests. This includes accepted loading voice,
membership rekey/revocation, map-policy removal, loading chat and protocol
history. It is an existing focused suite rerun, not new evidence of hardware
microphone behavior, every reconnect case, or complete release validation.

The pending preview remains blocked by its imported Add-On bundle: the current
generation identity hashes the importer and whole port registry, so the old
bundle's inventory cannot simply establish current importer output. The
checkout's changes since preview.1 include the importer and reference resolver;
the old bundle cannot be relabeled current just because the binaries compile.
