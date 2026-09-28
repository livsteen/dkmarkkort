// Kortet på forsiden.
//
// Siden er renderet af serveren og har alt kortet skal bruge i
// data-attributter. Her sættes OpenLayers op, og panelets knapper kobles til.
// Filtrering sker lokalt: hver mark i tiles'ene har sin `gruppe` og sin
// landsdel (`nuts3`), så et filterskift tegner de hentede tiles igen uden
// at spørge serveren.

(() => {
	'use strict';

	const kortEl = document.getElementById('kort');
	const landsdelEl = document.getElementById('landsdel');
	const gruppeKnapper = [...document.querySelectorAll('[data-gruppe]')];
	const baggrundKnapper = [...document.querySelectorAll('[data-baggrund]')];

	const filter = {
		nuts3: null,
		slukket: new Set(),
	};

	const tilKort = (tekst) =>
		ol.proj.transformExtent(tekst.split(' ').map(Number), 'EPSG:4326', 'EPSG:3857');

	const escape = (tekst) =>
		tekst.replace(/[&<>"']/g, (t) => `&#${t.charCodeAt(0)};`);

	const link = (href, tekst) => `<a href="${escape(href)}">${escape(tekst)}</a>`;

	// Ét Style-objekt pr. gruppe, oprettet på forhånd. Stilfunktionen kaldes
	// for hver mark i hver tegning og skal bare slå op.
	const stile = new Map(
		gruppeKnapper.map((knap) => [
			knap.dataset.gruppe,
			new ol.style.Style({
				fill: new ol.style.Fill({ color: `${knap.dataset.farve}b0` }),
				stroke: new ol.style.Stroke({ color: 'rgba(40, 40, 40, 0.35)', width: 0.6 }),
			}),
		]),
	);

	const stil = (mark) => {
		const gruppe = mark.get('gruppe');
		if (filter.slukket.has(gruppe)) return null;
		if (filter.nuts3 !== null && mark.get('nuts3') !== filter.nuts3) return null;
		return stile.get(gruppe) ?? stile.get('ukendt');
	};

	const marker = new ol.layer.VectorTile({
		source: new ol.source.VectorTile({
			format: new ol.format.MVT(),
			url: kortEl.dataset.tiles,
			maxZoom: 14,
			attributions: kortEl.dataset.kreditering
				.split('|')
				.map((tekst) => link('/kilder', tekst)),
		}),
		style: stil,
	});

	const baggrunde = baggrundKnapper.map(
		(knap) =>
			new ol.layer.Tile({
				visible: knap.getAttribute('aria-pressed') === 'true',
				source: new ol.source.XYZ({
					url: knap.dataset.baggrund,
					maxZoom: 19,
					attributions: link(knap.dataset.krediteringUrl, knap.dataset.kreditering),
				}),
			}),
	);

	const hele = tilKort(kortEl.dataset.danmark);

	const kort = new ol.Map({
		target: kortEl,
		layers: [...baggrunde, marker],
		view: new ol.View({ center: ol.extent.getCenter(hele), zoom: 7, maxZoom: 20 }),
		// Kilderne kræver kreditering, så den er foldet ud fra start.
		controls: [
			new ol.control.Zoom(),
			new ol.control.ScaleLine(),
			new ol.control.Attribution({ collapsible: true, collapsed: false }),
		],
	});

	const vis = (udstraekning, varighed) =>
		kort.getView().fit(udstraekning, { padding: [24, 24, 24, 24], duration: varighed });

	vis(hele, 0);

	landsdelEl.addEventListener('change', () => {
		const valgt = landsdelEl.selectedOptions[0];
		filter.nuts3 = landsdelEl.value || null;
		marker.changed();
		vis(filter.nuts3 ? tilKort(valgt.dataset.udstraekning) : hele, 500);
	});

	for (const knap of gruppeKnapper) {
		knap.addEventListener('click', () => {
			const taendt = knap.getAttribute('aria-pressed') !== 'true';
			knap.setAttribute('aria-pressed', String(taendt));
			if (taendt) {
				filter.slukket.delete(knap.dataset.gruppe);
			} else {
				filter.slukket.add(knap.dataset.gruppe);
			}
			marker.changed();
		});
	}

	baggrundKnapper.forEach((knap, nr) => {
		knap.addEventListener('click', () => {
			baggrundKnapper.forEach((anden, i) => {
				anden.setAttribute('aria-pressed', String(i === nr));
				baggrunde[i].setVisible(i === nr);
			});
		});
	});
})();
