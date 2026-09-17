#!/bin/sh
# cclean one-line launcher for macOS / Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean/cclean.sh | sh -s -- run -y
#
# Downloads cclean.py next to this script's URL into a temp dir and runs it with
# Python 3, keeping the terminal attached so the interactive TUI works even when
# the launcher itself arrives through a pipe.
#
#   CCLEAN_RAW   base URL of the cclean folder (default: master branch on GitHub)
#   CCLEAN_PY    Python interpreter to use (default: python3, then python)
set -eu

CCLEAN_RAW="${CCLEAN_RAW:-https://raw.githubusercontent.com/yatotm/CCometixLine/master/cclean}"

die() { printf '%s\n' "cclean: $*" >&2; exit 1; }

find_python() {
    if [ -n "${CCLEAN_PY:-}" ]; then
        printf '%s' "$CCLEAN_PY"
        return
    fi
    for cand in python3 python; do
        if command -v "$cand" >/dev/null 2>&1 &&
           "$cand" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)' >/dev/null 2>&1; then
            printf '%s' "$cand"
            return
        fi
    done
    die "Python 3.8+ not found. Install it (macOS: xcode-select --install or brew install python; Linux: apt/dnf install python3) and retry."
}

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        die "need curl or wget to download $1"
    fi
}

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t cclean)"
trap 'rm -rf "$tmp"' EXIT INT TERM

script="$tmp/cclean.py"
fetch "$CCLEAN_RAW/cclean.py" "$script"
grep -q 'cclean' "$script" 2>/dev/null || die "downloaded file does not look like cclean.py ($CCLEAN_RAW/cclean.py)"

py="$(find_python)"

# When piped through `curl | sh`, stdin is the pipe; hand the real terminal to Python.
# The subshell probe catches hosts where /dev/tty exists but cannot be opened (CI, containers).
if [ ! -t 0 ] && ( : </dev/tty ) 2>/dev/null; then
    "$py" "$script" "$@" </dev/tty
else
    "$py" "$script" "$@"
fi
