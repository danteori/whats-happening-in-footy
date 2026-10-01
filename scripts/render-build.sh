#!/usr/bin/env bash
# Render build step: fetch the Tailscale binaries, then build the backend.
set -euo pipefail

# To upgrade: take the version from https://pkgs.tailscale.com/stable/ and the
# hash from tailscale_<version>_amd64.tgz.sha256 next to it.
TS_VERSION="1.102.4"
TS_SHA256="50748df1045e60b5b695f19f4c56b0da36c019948b440fb456b6584a50f0d8b9"

tarball="$(mktemp)"
curl -fsSL -o "$tarball" "https://pkgs.tailscale.com/stable/tailscale_${TS_VERSION}_amd64.tgz"
echo "${TS_SHA256}  ${tarball}" | sha256sum -c --quiet -
mkdir -p bin
tar -xzf "$tarball" -C bin --strip-components=1 \
  "tailscale_${TS_VERSION}_amd64/tailscale" "tailscale_${TS_VERSION}_amd64/tailscaled"
rm "$tarball"

cargo build --release --locked
