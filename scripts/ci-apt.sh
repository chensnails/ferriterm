#!/usr/bin/env bash
# Install Ubuntu packages for the webview toolchain.
#
# The Azure-hosted GitHub runners intermittently fail to resolve or fetch from
# *.ubuntu.com, and newer images route apt through
# mirror+file:/etc/apt/apt-mirrors.txt, so editing sources.list alone does
# nothing. Resolving the archive is not proof it serves packages either, so
# point apt at Tencent and Aliyun, which answer from every runner region.
set -euo pipefail

if [ -f /etc/apt/apt-mirrors.txt ]; then
  sudo tee /etc/apt/apt-mirrors.txt >/dev/null <<'EOF'
https://mirrors.cloud.tencent.com/ubuntu/	priority:1
https://mirrors.aliyun.com/ubuntu/	priority:2
EOF
fi
sudo sed -i -E \
  's#https?://([a-z0-9.-]*\.)?(archive|security)\.ubuntu\.com/ubuntu#https://mirrors.cloud.tencent.com/ubuntu#g' \
  /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources 2>/dev/null || true

opts=(-o Acquire::ForceIPv4=true -o Acquire::Retries=3
      -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20
      # The mirrors are reached directly; never route them through a proxy.
      -o Acquire::http::Proxy=false -o Acquire::https::Proxy=false)
timeout 300 sudo apt-get "${opts[@]}" update
timeout 900 sudo apt-get "${opts[@]}" install -y "$@"
