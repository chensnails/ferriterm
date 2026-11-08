#!/bin/sh
# One-time loopback fixture key; never commit (see .gitignore).
set -eu
cd "$(dirname "$0")"
if [ ! -f host_key ]; then
  ssh-keygen -t ed25519 -N "" -C "ferriterm-testenv" -f host_key >/dev/null
  chmod 600 host_key
fi
echo "testenv ready: host_key present"