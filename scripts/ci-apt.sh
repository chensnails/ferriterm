#!/usr/bin/env bash
# Install Ubuntu packages for the webview toolchain.
#
# The Azure-hosted GitHub runners intermittently lose DNS for *.ubuntu.com
# (systemd-resolved times out). Newer images route apt through
# mirror+file:/etc/apt/apt-mirrors.txt, so rewriting sources.list alone does
# nothing: every fetch is retried against an unresolvable host until the step
# times out. Only when the official archive is unreachable, point the
# mirrorlist and any archive entries at mirrors that answer, keeping the
# image's own configuration otherwise.
set -euo pipefail

if ! getent ahostsv4 archive.ubuntu.com >/dev/null 2>&1; then
  echo "::warning::archive.ubuntu.com does not resolve; using public mirrors"
  if [ -f /etc/apt/apt-mirrors.txt ]; then
    sudo tee /etc/apt/apt-mirrors.txt >/dev/null <<'EOF'
https://mirrors.edge.kernel.org/ubuntu/	priority:1
https://mirrors.tuna.tsinghua.edu.cn/ubuntu/	priority:2
EOF
  fi
  sudo sed -i -E \
    's#https?://([a-z0-9.-]*\.)?(archive|security)\.ubuntu\.com/ubuntu#https://mirrors.edge.kernel.org/ubuntu#g' \
    /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources 2>/dev/null || true
fi

opts=(-o Acquire::ForceIPv4=true -o Acquire::Retries=3
      -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
timeout 300 sudo apt-get "${opts[@]}" update
timeout 900 sudo apt-get "${opts[@]}" install -y "$@"
