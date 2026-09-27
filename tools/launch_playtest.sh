#!/usr/bin/env bash
# Linux playtest launcher (packaged as launch.sh). Mirrors Launch-Playtest.ps1:
# runs the client from the package folder with its content and a local
# user-state folder, and keeps stdout/stderr logs.
set -u
root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"
mkdir -p user-state logs
stamp="$(date -u +%Y%m%d-%H%M%S)"
stdout="logs/client-$stamp.stdout.log"
stderr="logs/client-$stamp.stderr.log"
if [[ ! -x ./bri-client || ! -f content/client-content.json ]]; then
    echo "This package is incomplete: bri-client or content/client-content.json is missing."
    exit 1
fi
./bri-client --run ./content ./user-state >"$stdout" 2>"$stderr"
code=$?
echo "Client exited with code $code. Logs: $stdout and $stderr"
[[ $code -eq 0 ]] || echo "Check the stderr log for startup or runtime errors."
exit $code
