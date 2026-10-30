# v0.4.2 original-map policy byte portability

The first build-only release run, `41005009835` at `70662ae53a9d`, failed
macOS and Linux while fetching the Add-On bundle. Both rejected Creek's
authored waterfall/shore-only policy receipt. The local Windows checks passed.

The policy JSON checked out with eight CRLF line endings on Windows. Its raw
SHA256 was `bd5fe0e85fa0a858dd7074646d4fd645175332d9e96e7c742fa3800ceb447476`;
the repository's LF bytes hash to
`07482c345a021d7c305090e7967220a50b1403623c12a8fd9c3aef7192c1cfea`.
The applied script bytes, script hash and selected profiles all matched. The
receipt deliberately hashes the input's exact bytes, so changing checkout
line endings was enough to invalidate it on another platform.

Enforce LF for this JSON in `.gitattributes`, as for other hashed importer
inputs. The integrity assertions and playback policy remain unchanged. The
actual generated bundle must be rebuilt with canonical bytes before restarting
all three native package checks. No new sound, gain, emitter or gameplay change
is included.

The old local full gate was stopped during its build, before tests or
performance campaigns, once the platform failure required a new candidate.
Only its verified Cargo process tree was stopped; its Python owner restored the
shared content junction. Tool tests had passed in seven seconds. The build's
nonzero result is an intentional cancellation, not a compiler or test failure.
The remaining Windows build was cancelled; the actual macOS/Linux failures and
logs are retained. Neither run establishes a full release pass.

Fresh base generation had passed all 19 pack recipes and native startup before
this portability correction. An initially missing recovered UI-script path was
fixed by copying 32 existing recovered inputs read-only; the three UI source
hashes match the independently verified v0.4.1 source receipts. A first bundle
attempt refused unpinned Tier+Tactical variants; the subsequent build used the
exact source roots recorded by v0.4.1, without changing pins or omitting packs.

The final four ambience recordings exactly match the active approved +12 dB
preview package, `map_ambience_plus12db`. An initial independent comparison
incorrectly read the retained inactive unity package; resolving the active
package list corrected that verifier. The regenerated stock/tool catalog is
fully equivalent after explicit existing serde defaults and Resource path
ordering; all projectile policy remains equivalent to clean v0.4.1. No
cancelled Gun policy is included. These checks do not replace the pending
corrected-candidate full gate and native package checks.
