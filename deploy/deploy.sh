#!/usr/bin/env bash
# Build Y-Scale-DSP for the Pi (native aarch64 in Docker) and deploy the binary
# + configs over SSH. No Rust toolchain or compilation happens on the Pi itself.
#
# Usage:
#   ./deploy/deploy.sh [PI_HOST]
# Env:
#   PI_HOST   ssh target (default: jake@treepi.local)
#   IMAGE     builder image (default: yscale-builder)
set -euo pipefail

PI_HOST="${1:-${PI_HOST:-jake@treepi.local}}"
IMAGE="${IMAGE:-yscale-builder}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo ">> Building (native aarch64 in Docker)..."
docker run --rm \
  -v "$ROOT":/work \
  -v yscale-cargo-registry:/usr/local/cargo/registry \
  -w /work \
  "$IMAGE" \
  cargo build --release

BIN="$ROOT/target/release/yscale"
test -x "$BIN" || { echo "build produced no binary at $BIN"; exit 1; }
echo ">> Built $(du -h "$BIN" | cut -f1) binary"

echo ">> Deploying to ${PI_HOST}..."
scp "$BIN" "$PI_HOST:/tmp/yscale"
scp -r "$ROOT/configs" "$PI_HOST:/tmp/yscale-configs"
ssh "$PI_HOST" '
  set -e
  sudo install -m 0755 /tmp/yscale /usr/local/bin/yscale
  sudo mkdir -p /etc/yscale
  sudo cp /tmp/yscale-configs/*.toml /etc/yscale/
  # Default config if none installed yet.
  [ -f /etc/yscale/yscale.toml ] || sudo cp /etc/yscale/passthrough.toml /etc/yscale/yscale.toml
  rm -rf /tmp/yscale /tmp/yscale-configs
  echo "installed: $(/usr/local/bin/yscale --version)"
'

echo ">> Done. Try (CAREFUL - connect a load/speaker first, start quiet):"
echo "   ssh ${PI_HOST} '/usr/local/bin/yscale --config /etc/yscale/passthrough.toml sine --freq 1000 --amp 0.1'"
