#!/usr/bin/env bash
# Linux playtest launcher (packaged as launch.sh). Mirrors Launch-Playtest.ps1:
# runs the client from the package folder with its content and the game's
# per-user state folder (XDG_DATA_HOME/blockland-reimagined), so a newer
# release's folder finds the same settings and saves, and keeps stdout/stderr
# logs while still showing them in the terminal.
set -u
root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"
mkdir -p logs
stamp="$(date -u +%Y%m%d-%H%M%S)"
stdout="logs/client-$stamp.stdout.log"
stderr="logs/client-$stamp.stderr.log"

# Print a message, and show it in a desktop dialog too when the launcher was
# started without a terminal (double-clicked in a file manager).
tell() {
    echo "$1"
    [[ -t 1 ]] && return
    if command -v zenity >/dev/null; then
        zenity --error --no-markup --title "Blockland ReImagined" --text "$1" 2>/dev/null
    elif command -v kdialog >/dev/null; then
        kdialog --title "Blockland ReImagined" --error "$1" 2>/dev/null
    fi
}

if [[ ! -f ./bri-client || ! -f content/packages.json ]]; then
    tell "This package is incomplete: bri-client or content/packages.json is missing. Extract the whole zip again."
    exit 1
fi
# Some extractors drop the executable bit that the zip records.
for program in bri-client bri-server bri-import-addon; do
    if [[ -f "./$program" && ! -x "./$program" ]]; then
        echo "Marking $program as executable (the extractor dropped that)."
        chmod +x "./$program" || { tell "Could not mark $program as executable. Run: chmod +x $root/$program"; exit 1; }
    fi
done
# A system older than the build machine fails in the loader before the game
# can say anything, so ask the loader first and explain its answer.
if ! loader="$(./bri-client --version 2>&1)"; then
    echo "$loader" >"$stderr"
    if [[ "$loader" == *GLIBC_* ]]; then
        needed="$(grep -o 'GLIBC_[0-9.]*' <<<"$loader" | sort -uV | tail -n 1)"
        tell "This system's C library is too old for this build: it needs ${needed/_/ } or newer, and this system has $(getconf GNU_LIBC_VERSION 2>/dev/null || echo 'an older one'). Update the system or use a newer distro release."
    else
        tell "bri-client could not start: $loader"
    fi
    exit 1
fi

# Each stream goes to the terminal and its own log; both copies finish
# before the summary below.
{
    ./bri-client --run ./content 2>&1 1>&3 3>&- | tee "$stderr" >&2
    exit "${PIPESTATUS[0]}"
} 3>&1 | tee "$stdout"
code=${PIPESTATUS[0]}
echo "Client exited with code $code. Logs: $stdout and $stderr"
[[ $code -eq 0 ]] || echo "Check the stderr log for startup or runtime errors."
exit $code
