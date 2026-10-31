# Bot_Town installed dependency geometry

Reproduced issue #414 with the pinned, unmodified `Bot_Town.zip` and current
generated `content`. With the original Bot_Hole ZIP beside the input, the old
importer produced eight runtime brick bindings. With Bot_Town isolated and only
native Bot_Hole installed, the same importer wrote eight `assets/bricks.json`
declarations but no runtime catalog. `bri-addon-check` incorrectly reported OK.
Inputs, outputs and logs are under the private `diagnostics/bot-town-414` folder.

The importer now looks only at installed packages matching declared required
Add-Ons, after checking source dependencies beside the input. It verifies the
native package identity, recorded original virtual path, content index, native
mesh identity/validity and safe package-relative paths. It copies the native
mesh under the borrower's own authored ID and records both the source hash
reported by the dependency and a freshly calculated native-file hash. The
source hash is provenance supplied by that installed package, not independent
authentication of an absent original. The native hash participates in the
output fingerprint. Unrelated installed packages do not lend geometry.

The checker now compares imported brick declarations with the runtime catalog,
mesh bindings and collision recipes. Missing or partial output is an error
that names unavailable bricks; it reports `0 of 8` for the reproduced broken
import. Shared `v20/` shapes are recognized as base references, but a
standalone checker without an installed base cannot independently prove those
base shapes exist.

The isolated original Bot_Town import now generates eight catalog entries,
eight mesh bindings and eight collision recipes using the installed Bot_Hole
conversion. With a private copy of that native dependency beside the result,
the checker passes. The game's actual definition loader loads all eight native
meshes and finite collision shapes. Public invented-fixture tests cover source
case variations, undeclared dependency refusal, changed native-content
fingerprints, rejected mesh identity, and the previous missing/partial checker
false success. The original source installations and archives were not edited.

Checks: `cargo test --locked --profile playtest -p bri-addon-import --test
borrowed_bricks -- --include-ignored` with `BRI_BOT_TOWN_OUTPUT` set to the
private output (4 passed); `cargo test --locked --profile playtest -p
bri-addon-import --test installed --test bot_holes` (11 passed); `cargo test
--locked --profile playtest -p bri-package-runtime --test check` (13 passed);
and warning-free Clippy for both changed crates and these focused test targets.
The missing-catalog and installed-native cases were each demonstrated failing
before their respective fixes.

Not yet complete: exercise actual planted bricks, collision, save/reload and
late join with the real imported result; audit conflicting source providers,
explicit error reporting for invalid installed metadata and any dependency
cycles. Do not close issue #414 or claim full release acceptance solely from
catalog loading.
