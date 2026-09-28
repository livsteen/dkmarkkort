#!/usr/bin/env bash
#
# Starter udviklingsserveren, synlig på netværket. Kører den allerede, stoppes
# den først, så der aldrig er to om porten.
#
#   ./run.sh
#
# HOST og PORT kan sættes udefra; standard er 0.0.0.0:3000.
set -euo pipefail

cd "$(dirname "$0")"

# cargo og topcoat ligger i ~/.cargo/bin, som ikke altid er i PATH.
if ! command -v topcoat >/dev/null && [[ -f "$HOME/.cargo/env" ]]; then
	source "$HOME/.cargo/env"
fi

export HOST="${HOST:-0.0.0.0}"
export PORT="${PORT:-3000}"

if pkill -f "topcoat dev -p dkmarkkort"; then
	echo "==> Stoppede kørende topcoat dev"
fi

# Serverprogrammet kan overleve sin topcoat dev, så porten ryddes også.
if pids="$(lsof -ti "tcp:$PORT" -sTCP:LISTEN 2>/dev/null)"; then
	echo "==> Stopper det der lytter på port $PORT"
	kill $pids
	for _ in $(seq 1 20); do
		lsof -ti "tcp:$PORT" -sTCP:LISTEN &>/dev/null || break
		sleep 0.25
	done
fi

exec topcoat dev -p dkmarkkort
