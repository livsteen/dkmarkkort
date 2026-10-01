#!/usr/bin/env bash
# Holder kortets data i /data opdateret. Kører i pipeline-containeren og
# tjekker én gang i døgnet. Data bygges, når der ingen er, når pipelinen er
# ændret siden sidst, eller når de er mere end 30 dage gamle. Serveren
# opdager selv de nye data.
set -uo pipefail

data="${MARKKORT_DATA:-/data}"
version="$(cat /app/pipelineversion)"
maks_alder_dage=30

# Skriver grunden til at bygge og lykkes, hvis data skal bygges.
skal_bygges() {
	if [[ ! -f "$data/bygget" ]]; then
		echo "der er ingen data"
	elif [[ "$(cat "$data/pipelineversion" 2>/dev/null)" != "$version" ]]; then
		echo "pipelinen er ændret"
	elif [[ -n "$(find "$data/pipelineversion" -mtime "+$maks_alder_dage")" ]]; then
		# Pipelinen genbruger en hentet fil, så den skal væk for at få
		# den nyeste udgave af markerne.
		rm -f "$data"/raw/Marker_*.zip "$data"/raw/Marker_*.zip.hentet
		echo "data er mere end $maks_alder_dage dage gamle"
	else
		return 1
	fi
}

while true; do
	if grund="$(skal_bygges)"; then
		echo "==> Bygger data: $grund"
		cp /app/input/* "$data/"
		if /app/dkmarkkort-pipeline --data "$data"; then
			echo "$version" >"$data/pipelineversion"
			echo "==> Data er bygget"
		else
			echo "==> Pipelinen fejlede, prøver igen om et døgn" >&2
		fi
	fi
	sleep 86400
done
