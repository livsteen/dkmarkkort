#!/usr/bin/env bash
# Holder kortets data i /data opdateret. Kører i pipeline-containeren og
# tjekker én gang i døgnet. Data bygges, når der ingen er, når pipelinen er
# ændret siden sidst, eller når de er mere end 30 dage gamle. Serveren
# opdager selv de nye data.
#
# Fejler pipelinen, skrives tidspunktet i /data/fejlet, så serveren kan sige
# det, og der prøves igen om en time. Filen slettes, når pipelinen lykkes.
set -uo pipefail

data="${MARKKORT_DATA:-/data}"
version="$(cat /app/pipelineversion)"
maks_alder_dage=30
vent_sekunder=86400
vent_efter_fejl_sekunder=3600

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
	vent="$vent_sekunder"
	if grund="$(skal_bygges)"; then
		echo "==> Bygger data: $grund"
		cp /app/input/* "$data/"
		if /app/dkmarkkort-pipeline --data "$data"; then
			echo "$version" >"$data/pipelineversion"
			rm -f "$data/fejlet"
			echo "==> Data er bygget"
		else
			date -u +%Y-%m-%dT%H:%M:%SZ >"$data/fejlet"
			vent="$vent_efter_fejl_sekunder"
			echo "==> Pipelinen fejlede, prøver igen om en time" >&2
		fi
	fi
	sleep "$vent"
done
