#!/usr/bin/env bash
# Build the Vue UI + the yscale-server (native aarch64 in Docker, embedding the
# built UI), deploy to the Pi, and install/enable the systemd service.
#
# Usage: ./deploy/deploy-web.sh [PI_HOST]   (default: jake@mediapi.local)
set -euo pipefail

PI_HOST="${1:-${PI_HOST:-jake@mediapi.local}}"
IMAGE="${IMAGE:-yscale-builder}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo ">> Building Vue UI..."
( cd "$ROOT/web" && npm install && npm run build )
test -f "$ROOT/web/dist/index.html" || { echo "frontend build produced no dist/index.html"; exit 1; }

echo ">> Building yscale-server (aarch64 in Docker, embeds web/dist)..."
docker run --rm \
  -v "$ROOT":/work \
  -v yscale-cargo-registry:/usr/local/cargo/registry \
  -w /work \
  "$IMAGE" \
  cargo build --release --bin yscale-server

BIN="$ROOT/target/release/yscale-server"
test -x "$BIN" || { echo "no server binary at $BIN"; exit 1; }
echo ">> Built $(du -h "$BIN" | cut -f1) server binary"

echo ">> Deploying to ${PI_HOST}..."
scp "$BIN" "${PI_HOST}:/tmp/yscale-server"
scp "$ROOT/deploy/yscale-server.service" "${PI_HOST}:/tmp/yscale-server.service"
ssh "${PI_HOST}" '
  set -e
  sudo systemctl stop yscale-server 2>/dev/null || true
  sudo install -m 0755 /tmp/yscale-server /usr/local/bin/yscale-server
  sudo cp /tmp/yscale-server.service /etc/systemd/system/yscale-server.service
  sudo systemctl daemon-reload
  sudo systemctl enable --now yscale-server
  rm -f /tmp/yscale-server /tmp/yscale-server.service
  sleep 1
  systemctl is-active yscale-server && echo "yscale-server active" || journalctl -u yscale-server -n 20 --no-pager
'
HOSTONLY="$(echo "${PI_HOST}" | cut -d@ -f2)"
echo ">> Done. Open from any LAN device:  http://${HOSTONLY}:8080"
