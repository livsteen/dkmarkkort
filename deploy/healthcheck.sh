#!/usr/bin/env bash
# Tjekker appen gennem Caddy, præcis som trafikken fra tunnelen kommer ind.
# Venter op til 60 sekunder, så Caddy når at opdage den nye container.
set -euo pipefail
host="$1"
for _ in $(seq 1 30); do
  if curl -fsS -o /dev/null -H "Host: ${host}" "http://127.0.0.1:8000/"; then
    echo "${host} svarer"
    exit 0
  fi
  sleep 2
done
echo "health check fejlede for ${host}" >&2
exit 1

