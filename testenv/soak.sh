#!/bin/sh
# Long-running leak soak. Starts the loopback SSH fixture, then runs the soak
# example for N seconds (default 72h) and writes metrics/summary to
# soak-runs/<UTC timestamp>/.
#
#   testenv/soak.sh            # 72h
#   testenv/soak.sh 120        # two-minute smoke
#   FERRITERM_SOAK_OUT=/tmp/soak testenv/soak.sh
#
# The fixture is killed when the soak exits, including on Ctrl-C.
set -eu
cd "$(dirname "$0")/.."

seconds="${1:-${FERRITERM_SOAK_SECONDS:-259200}}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="${FERRITERM_SOAK_OUT:-soak-runs/$stamp}"
mkdir -p "$out"

if lsof -nP -iTCP:2222 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "port 2222 is already in use; stop the other loopback fixture first" >&2
  exit 1
fi

sh testenv/setup.sh >/dev/null
cargo build --release -p ferriterm --examples

./target/release/examples/loopback_sshd >"$out/fixture.log" 2>&1 &
fixture=$!
trap 'kill "$fixture" 2>/dev/null || true' EXIT INT TERM
sleep 1
if ! kill -0 "$fixture" 2>/dev/null; then
  echo "loopback fixture exited; see $out/fixture.log" >&2
  exit 1
fi

echo "soak: $seconds s, output $out, fixture pid $fixture"
FERRITERM_SOAK_SECONDS="$seconds" FERRITERM_SOAK_OUT="$out" \
  ./target/release/examples/soak
