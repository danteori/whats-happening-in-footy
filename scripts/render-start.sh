#!/usr/bin/env bash
# Render start step: join the tailnet (only if TS_AUTHKEY is set), then run the backend.
#
# Render containers have no /dev/net/tun, so tailscaled runs in userspace mode and
# the backend reaches tailnet machines through its SOCKS5 proxy on localhost:1055.
# If joining fails, the site still starts; it just can't reach the tailnet.
set -euo pipefail

if [[ -n "${TS_AUTHKEY:-}" ]]; then
  sock=/tmp/tailscaled.sock
  ts() { ./bin/tailscale --socket="$sock" "$@"; }

  ./bin/tailscaled --tun=userspace-networking --socks5-server=localhost:1055 \
    --state=mem: --socket="$sock" >/tmp/tailscaled.log 2>&1 &
  for _ in {1..50}; do [[ -S "$sock" ]] && break; sleep 0.2; done

  key="$TS_AUTHKEY"
  # An OAuth client secret (tskey-client-...) never expires, but the node options
  # have to be passed along with it. Ephemeral: the node is removed after it goes
  # offline, since a new one registers every time Render restarts the service.
  if [[ "$key" == tskey-client-* ]]; then
    key="${key}?ephemeral=true&preauthorized=true"
  fi

  if ts up --auth-key="$key" --advertise-tags="${TS_TAGS:-tag:render}" \
    --hostname="${TS_HOSTNAME:-footy-render}" --timeout=30s; then
    echo "tailscale: joined the tailnet as $(ts ip -4)"
  else
    echo "tailscale: could not join the tailnet, starting without it. Last daemon logs:" >&2
    tail -n 20 /tmp/tailscaled.log >&2 || true
  fi
fi

exec ./target/release/backend
