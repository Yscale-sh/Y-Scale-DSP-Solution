#!/usr/bin/env bash
# One-time Pi-side setup for DLNA streaming THROUGH the DSP:
#   gmediarender (UPnP renderer) -> ALSA snd-aloop loopback -> yscale engine capture
# Then in the web UI pick the "DLNA / Stream In" source, and from any UPnP/DLNA
# control app play to the "mediapi" renderer.
#
# Usage: ./deploy/setup-dlna.sh [PI_HOST]   (default: jake@mediapi.local)
set -euo pipefail
PI_HOST="${1:-${PI_HOST:-jake@mediapi.local}}"

ssh "${PI_HOST}" 'bash -s' <<'REMOTE'
set -e
echo ">> snd-aloop loopback (persisted)"
echo snd-aloop | sudo tee /etc/modules-load.d/snd-aloop.conf >/dev/null
sudo modprobe snd-aloop || true
aplay -l | grep -qi loopback && echo "   loopback card present" || { echo "   ERROR: no Loopback card"; exit 1; }

echo ">> gmediarender + gstreamer (DLNA renderer, gst-launch for URL playback, codecs)"
sudo apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
  gmediarender gstreamer1.0-tools gstreamer1.0-alsa \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-libav >/dev/null
echo "   installed $(gmediarender --version 2>/dev/null | head -1 || echo gmediarender)"

echo ">> configure renderer: name=mediapi, output -> loopback"
sudo sed -i 's/^UPNP_DEVICE_NAME=.*/UPNP_DEVICE_NAME="mediapi"/' /etc/default/gmediarender
sudo sed -i 's|^ALSA_DEVICE=.*|ALSA_DEVICE="plughw:Loopback,0,0"|' /etc/default/gmediarender
sudo sed -i 's/^INITIAL_VOLUME_DB=.*/INITIAL_VOLUME_DB=-20/' /etc/default/gmediarender
sudo systemctl enable gmediarender >/dev/null 2>&1 || true
sudo systemctl restart gmediarender
sleep 1
echo "   gmediarender: $(systemctl is-active gmediarender)"
echo ">> Done. In the web UI choose 'DLNA / Stream In', then play to the 'mediapi'"
echo ">> renderer from any UPnP/DLNA control app — audio runs through your DSP."
REMOTE
