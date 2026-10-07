// Kortet på forsiden.
//
// Siden er renderet af serveren og har alt kortet skal bruge i
// data-attributter. Her sættes OpenLayers op, og panelets knapper kobles til.
// Filtrering sker lokalt: hver mark i tiles'ene har sin `gruppe` og sin
// landsdel (`nuts3`), så et filterskift tegner de hentede tiles igen uden
// at spørge serveren. Zoomet ud vises oversigten, hvor hver pixel bærer
// gruppe og landsdel som tal, og den filtreres på samme måde.
//
// Alt andet om en mark end det kortet tegner, står i databasen. Et klik på en
// mark og en søgning efter en bedrift spørger derfor serveren (`/mark/{id}`,
// `/soeg`, `/bedrift/{cvr}`), og svaret sættes ind i sidens skabeloner.

(() => {
	'use strict';

	const kortEl = document.getElementById('kort');
	const landsdelEl = document.getElementById('landsdel');
	const gruppeKnapper = [...document.querySelectorAll('[data-gruppe]')];
	const baggrundKnapper = [...document.querySelectorAll('[data-baggrund]')];
	const panelEl = document.getElementById('panel');
	const panelIndholdEl = document.getElementById('panel-indhold');
	const minimerKnap = panelEl.querySelector('[data-minimer]');
	const infoEl = document.getElementById('info');
	const infoBedriftKnap = document.getElementById('info-bedrift');
	const soegEl = document.getElementById('soeg');
	const forslagEl = document.getElementById('forslag');
	const soegBeskedEl = document.getElementById('soeg-besked');
	const bedriftEl = document.getElementById('bedrift');
	const bedriftRydKnap = document.getElementById('bedrift-ryd');
	const skabelonBedrift = document.getElementById('skabelon-bedrift');
	const skabelonMark = document.getElementById('skabelon-mark');

	const filter = {
		nuts3: null,
		slukket: new Set(),
	};

	const fraGrader = (udstraekning) =>
		ol.proj.transformExtent(udstraekning, 'EPSG:4326', 'EPSG:3857');

	const tilKort = (tekst) => fraGrader(tekst.split(' ').map(Number));

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

	// Den valgte mark beholder sin gruppes farve og får en rød kant. Den
	// tegnes øverst, så kanten ikke skjules af naboerne.
	const fremhaevet = new Map(
		gruppeKnapper.map((knap) => [
			knap.dataset.gruppe,
			new ol.style.Style({
				fill: new ol.style.Fill({ color: `${knap.dataset.farve}d0` }),
				stroke: new ol.style.Stroke({ color: '#d7191c', width: 3 }),
				zIndex: 1,
			}),
		]),
	);

	// Marken fra serveren, som den står i info-panelet.
	let valgt = null;

	const stil = (mark) => {
		const gruppe = mark.get('gruppe');
		// Den valgte mark vises også når et filter ville skjule den: den er
		// valgt for at blive set.
		if (valgt !== null && mark.getId() === valgt.id) {
			return fremhaevet.get(gruppe) ?? fremhaevet.get('ukendt');
		}
		if (filter.slukket.has(gruppe)) return null;
		if (filter.nuts3 !== null && mark.get('nuts3') !== filter.nuts3) return null;
		return stile.get(gruppe) ?? stile.get('ukendt');
	};

	// Zoomet ud er markerne for mange til at tegne hver for sig, og kortet
	// viser oversigten. Markerne tager over efter zoom 11, hvor kortet bruger
	// tiles'ene fra zoom 10, som har alle marker med.
	const MARKER_FRA = 11;

	// Begge lag viser markdata, så krediteringen står der på alle zoom.
	const kreditering = kortEl.dataset.kreditering
		.split('|')
		.map((tekst) => link('/kilder', tekst));

	const marker = new ol.layer.VectorTile({
		minZoom: MARKER_FRA,
		source: new ol.source.VectorTile({
			format: new ol.format.MVT(),
			url: kortEl.dataset.tiles,
			minZoom: 10,
			maxZoom: 14,
			attributions: kreditering,
		}),
		style: stil,
	});

	// En pixel i oversigten er gruppens nummer gange 16 plus landsdelens
	// nummer, eller 0 hvor der ingen mark er. Skyggeren får den som 0–1, så
	// den ganges op igen.
	const kode = ['round', ['*', ['band', 1], 255]];
	const gruppeNr = ['floor', ['/', kode, 16]];
	const landsdelNr = ['%', kode, 16];

	// Gruppens farve står på dens nummer, lige så gennemsigtig som på
	// markerne. Plads 0 og ubrugte numre er uden farve.
	const palet = Array.from({ length: 16 }, () => [0, 0, 0, 0]);
	for (const knap of gruppeKnapper) {
		palet[Number(knap.dataset.nr)] = `${knap.dataset.farve}b0`;
	}

	const gruppeVar = (knap) => `gruppe${knap.dataset.nr}`;
	// 1 når pixlens gruppe er slået til, ellers 0.
	const gruppeVist = [
		'match',
		gruppeNr,
		...gruppeKnapper.flatMap((knap) => [Number(knap.dataset.nr), ['var', gruppeVar(knap)]]),
		0,
	];

	const oversigt = new ol.layer.WebGLTile({
		maxZoom: MARKER_FRA,
		source: new ol.source.XYZ({
			url: kortEl.dataset.overblik,
			minZoom: 5,
			maxZoom: 11,
			// En pixel er en kode og må ikke blandes med naboens.
			interpolate: false,
			attributions: kreditering,
		}),
		style: {
			// Landsdel 0 er hele landet.
			variables: {
				landsdel: 0,
				...Object.fromEntries(gruppeKnapper.map((knap) => [gruppeVar(knap), 1])),
			},
			color: [
				'case',
				[
					'all',
					['==', gruppeVist, 1],
					['any', ['==', ['var', 'landsdel'], 0], ['==', landsdelNr, ['var', 'landsdel']]],
				],
				['palette', gruppeNr, palet],
				[0, 0, 0, 0],
			],
		},
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
		layers: [...baggrunde, oversigt, marker],
		view: new ol.View({ center: ol.extent.getCenter(hele), zoom: 7, maxZoom: 20 }),
		// Kilderne kræver kreditering, så den er foldet ud fra start.
		controls: [
			new ol.control.Zoom(),
			new ol.control.ScaleLine(),
			new ol.control.Attribution({ collapsible: true, collapsed: false }),
		],
	});

	// En mark er lille, så der zoomes ikke længere ind end at man kan se
	// hvor den ligger.
	const vis = (udstraekning, varighed) =>
		kort.getView().fit(udstraekning, {
			padding: [24, 24, 24, 24],
			duration: varighed,
			maxZoom: 16,
		});

	vis(hele, 0);

	landsdelEl.addEventListener('change', () => {
		const valgt = landsdelEl.selectedOptions[0];
		filter.nuts3 = landsdelEl.value || null;
		marker.changed();
		oversigt.updateStyleVariables({ landsdel: Number(valgt.dataset.nr ?? 0) });
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
			oversigt.updateStyleVariables({ [gruppeVar(knap)]: taendt ? 1 : 0 });
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

	// --- Opslag hos serveren ---

	const hentJson = async (url) => {
		const svar = await fetch(url);
		if (!svar.ok) throw new Error(`${url} svarede ${svar.status}`);
		return svar.json();
	};

	const heltal = new Intl.NumberFormat('da-DK');
	const hektar = new Intl.NumberFormat('da-DK', {
		minimumFractionDigits: 2,
		maximumFractionDigits: 2,
	});
	const areal = (ha) => (ha === null ? '' : `${hektar.format(ha)} ha`);
	// Pipelinen samler marker uden CVR-nummer under UDEN_CVR. Nummeret vises,
	// så man kan søge på det, men det er ikke en rigtig bedrift.
	const UDEN_CVR = '00000000';
	const visCvr = (cvr) => (cvr === UDEN_CVR ? `${cvr} · uden CVR-nummer` : cvr);
	const farve = (gruppe) =>
		gruppeKnapper.find((knap) => knap.dataset.gruppe === gruppe)?.dataset.farve ?? '';

	// Sætter tekst ind i elementerne med `data-felt`. `farve` er en baggrund,
	// alt andet er tekst, så intet fra databasen fortolkes som HTML.
	const udfyld = (element, felter) => {
		for (const [navn, vaerdi] of Object.entries(felter)) {
			const felt = element.querySelector(`[data-felt="${navn}"]`);
			if (!felt) continue;
			if (navn === 'farve') {
				felt.style.background = vaerdi;
			} else {
				felt.textContent = vaerdi ?? '';
			}
		}
	};

	// --- Den valgte mark ---

	// Et klik kan nå at blive afløst af det næste før serveren svarer. Kun
	// svaret på det seneste må vises.
	let valgNr = 0;

	const vaelgMark = (mark, { zoom = false } = {}) => {
		valgNr += 1;
		valgt = mark;
		marker.changed();
		markerValgtIListen();

		if (mark === null) {
			infoEl.hidden = true;
			// Står den fravalgte marks nummer i søgefeltet, går det med den.
			if (soegEl.value === markINavn) soegEl.value = '';
			markINavn = null;
			return;
		}

		udfyld(infoEl, {
			farve: farve(mark.gruppe),
			marknr: mark.marknr,
			afgroede: mark.afgroede,
			afgroedekode: mark.afgroedekode === null ? '' : `(${mark.afgroedekode})`,
			gruppe: mark.gruppe_navn,
			afsnit: mark.afsnit ?? '–',
			areal: areal(mark.areal) || '–',
			landsdel: mark.landsdel,
			cvr: visCvr(mark.cvr),
		});
		infoBedriftKnap.disabled = mark.cvr === '';
		infoEl.hidden = false;
		holdInde(infoEl);

		if (zoom) vis(fraGrader(mark.udstraekning), 500);
	};

	const lagFilter = { layerFilter: (lag) => lag === marker, hitTolerance: 2 };

	kort.on('singleclick', async (haendelse) => {
		// I oversigten er markerne et billede og kan ikke vælges. Et klik
		// zoomer ind til dem.
		if (kort.getView().getZoom() <= MARKER_FRA) {
			kort.getView().animate({
				center: haendelse.coordinate,
				zoom: MARKER_FRA + 1,
				duration: 500,
			});
			return;
		}
		const ramt =kort.forEachFeatureAtPixel(haendelse.pixel, (mark) => mark, lagFilter);
		if (!ramt) {
			vaelgMark(null);
			return;
		}
		const nr = ++valgNr;
		try {
			const mark = await hentJson(`/mark/${ramt.getId()}`);
			if (nr !== valgNr) return;
			vaelgMark(mark);
			// Søgefeltet må ikke blive ved med at vise en anden mark end den
			// valgte.
			if (bedrift?.marker.some((egen) => egen.id === mark.id)) {
				soegEl.value = mark.marknr;
				markINavn = mark.marknr;
				skjulForslag();
			} else if (soegEl.value === markINavn) {
				soegEl.value = '';
				markINavn = null;
			}
		} catch (fejl) {
			console.error(fejl);
		}
	});

	kort.on('pointermove', (haendelse) => {
		if (haendelse.dragging) return;
		kortEl.style.cursor = kort.hasFeatureAtPixel(haendelse.pixel, lagFilter) ? 'pointer' : '';
	});

	infoEl.querySelector('[data-luk]').addEventListener('click', () => vaelgMark(null));

	infoBedriftKnap.addEventListener('click', () => {
		if (!valgt?.cvr) return;
		minimer(false);
		vaelgBedrift(valgt.cvr, { zoom: false });
	});

	// --- Søgning ---
	//
	// Først findes bedriften på sit CVR-nummer, så marken blandt bedriftens
	// egne. Marknumre er landmandens egne og går igen på tværs af landet, så
	// det er kun inden for en bedrift de siger noget.

	// Den valgte bedrift og dens marker, eller null mens der søges på CVR.
	let bedrift = null;
	// Det listen viser lige nu, og hvilken linje piletasterne står på.
	let forslag = [];
	let markeret = -1;
	let soegNr = 0;
	// Marknummeret i søgefeltet når det er sat af et valg i listen, og ikke
	// skrevet.
	let markINavn = null;
	let ventende = null;

	const besked = (tekst) => {
		soegBeskedEl.textContent = tekst;
	};

	const visForslag = (liste) => {
		forslag = liste;
		markeret = -1;
		soegEl.removeAttribute('aria-activedescendant');
		forslagEl.replaceChildren(
			...liste.map((valg, nr) => {
				const linje = (valg.slags === 'bedrift' ? skabelonBedrift : skabelonMark).content
					.firstElementChild.cloneNode(true);
				linje.id = `forslag-${nr}`;
				linje.setAttribute('aria-selected', 'false');
				if (valg.slags === 'bedrift') {
					udfyld(linje, {
						cvr: visCvr(valg.cvr),
						marker: `${heltal.format(valg.marker)} ${valg.marker === 1 ? 'mark' : 'marker'}`,
					});
				} else {
					udfyld(linje, {
						farve: farve(valg.mark.gruppe),
						marknr: valg.mark.marknr,
						afgroede: valg.mark.afgroede,
						areal: areal(valg.mark.areal),
					});
					linje.title = valg.mark.afgroede;
				}
				// Musen må ikke tage fokus fra søgefeltet, så piletasterne
				// virker videre efter et klik.
				linje.addEventListener('mousedown', (haendelse) => haendelse.preventDefault());
				linje.addEventListener('click', () => vaelgForslag(nr));
				return linje;
			}),
		);
		forslagEl.hidden = liste.length === 0;
		soegEl.setAttribute('aria-expanded', String(liste.length > 0));
		markerValgtIListen();
	};

	const markerValgtIListen = () => {
		forslagEl.querySelectorAll('[data-valgt]').forEach((linje) => linje.removeAttribute('data-valgt'));
		if (valgt === null) return;
		const nr = forslag.findIndex((valg) => valg.slags === 'mark' && valg.mark.id === valgt.id);
		if (nr < 0) return;
		const linje = forslagEl.children[nr];
		linje.setAttribute('data-valgt', '');
		linje.scrollIntoView({ block: 'nearest' });
	};

	const pegPaa = (nr) => {
		forslagEl.children[markeret]?.setAttribute('aria-selected', 'false');
		markeret = nr;
		const linje = forslagEl.children[nr];
		if (!linje) {
			soegEl.removeAttribute('aria-activedescendant');
			return;
		}
		linje.setAttribute('aria-selected', 'true');
		linje.scrollIntoView({ block: 'nearest' });
		soegEl.setAttribute('aria-activedescendant', linje.id);
	};

	const vaelgForslag = (nr) => {
		const valg = forslag[nr];
		if (!valg) return;
		if (valg.slags === 'bedrift') {
			vaelgBedrift(valg.cvr, { zoom: true });
		} else {
			vaelgMark(valg.mark, { zoom: true });
			// Marken er fundet; listen lukkes, så kortet og info-panelet
			// kan ses, og feltet viser hvilken mark det blev. Listen kommer
			// igen når man går tilbage til søgefeltet.
			soegEl.value = valg.mark.marknr;
			markINavn = valg.mark.marknr;
			skjulForslag();
		}
	};

	const skjulForslag = () => {
		forslagEl.hidden = true;
		soegEl.setAttribute('aria-expanded', 'false');
		soegEl.removeAttribute('aria-activedescendant');
	};

	const soegCvr = async (tekst) => {
		const nr = ++soegNr;
		const cifre = tekst.replace(/\s/g, '');
		if (cifre === '') {
			visForslag([]);
			besked('');
			return;
		}
		if (!/^\d+$/.test(cifre)) {
			visForslag([]);
			besked('Et CVR-nummer er otte cifre.');
			return;
		}
		try {
			const bedrifter = await hentJson(`/soeg?q=${encodeURIComponent(cifre)}`);
			if (nr !== soegNr) return;
			visForslag(bedrifter.map((b) => ({ slags: 'bedrift', ...b })));
			besked(bedrifter.length === 0 ? 'Ingen bedrift har et CVR-nummer der begynder sådan.' : '');
		} catch (fejl) {
			if (nr !== soegNr) return;
			console.error(fejl);
			besked('Søgningen mislykkedes. Prøv igen.');
		}
	};

	const filtrerMarker = (tekst) => {
		const soeg = tekst.trim().toLocaleLowerCase('da');
		const passer = (mark) =>
			soeg === '' ||
			mark.marknr.toLocaleLowerCase('da').startsWith(soeg) ||
			mark.afgroede.toLocaleLowerCase('da').includes(soeg);
		const fundne = bedrift.marker.filter(passer);
		visForslag(fundne.map((mark) => ({ slags: 'mark', mark })));
		const antal = bedrift.marker.length;
		besked(
			fundne.length === antal
				? `${heltal.format(antal)} ${antal === 1 ? 'mark' : 'marker'}`
				: `${heltal.format(fundne.length)} af ${heltal.format(antal)} marker`,
		);
	};

	const vaelgBedrift = async (cvr, { zoom }) => {
		const nr = ++soegNr;
		clearTimeout(ventende);
		besked('Henter bedriftens marker …');
		try {
			const marker = await hentJson(`/bedrift/${encodeURIComponent(cvr)}`);
			if (nr !== soegNr) return;
			bedrift = { cvr, marker };
		} catch (fejl) {
			if (nr !== soegNr) return;
			console.error(fejl);
			besked('Bedriftens marker kunne ikke hentes. Prøv igen.');
			return;
		}
		udfyld(bedriftEl, { cvr: visCvr(cvr) });
		bedriftEl.hidden = false;
		soegEl.value = '';
		markINavn = null;
		soegEl.placeholder = 'Marknr eller afgrøde';
		soegEl.focus();
		filtrerMarker('');

		if (zoom) {
			const samlet = bedrift.marker
				.map((mark) => fraGrader(mark.udstraekning))
				.reduce((a, b) => ol.extent.extend(a, b), ol.extent.createEmpty());
			vis(samlet, 500);
		}
	};

	const rydBedrift = () => {
		soegNr += 1;
		bedrift = null;
		bedriftEl.hidden = true;
		soegEl.value = '';
		markINavn = null;
		soegEl.placeholder = 'CVR-nummer';
		visForslag([]);
		besked('');
		soegEl.focus();
	};

	soegEl.addEventListener('input', () => {
		clearTimeout(ventende);
		if (bedrift !== null) {
			filtrerMarker(soegEl.value);
		} else {
			// Vent til der er en pause i tastningen, så hvert ciffer ikke
			// bliver sin egen forespørgsel.
			ventende = setTimeout(() => soegCvr(soegEl.value), 150);
		}
	});

	// Går man tilbage til feltet mens det viser den valgte mark, er det for
	// at vælge en anden: hele listen vises, og teksten markeres, så det
	// næste man skriver erstatter den.
	const genaabnForslag = () => {
		if (bedrift === null || !forslagEl.hidden) return;
		if (soegEl.value === markINavn) {
			filtrerMarker('');
			soegEl.select();
		} else {
			filtrerMarker(soegEl.value);
		}
	};

	soegEl.addEventListener('focus', genaabnForslag);
	// Et klik i feltet mens det har fokus, udløser ikke `focus` igen.
	soegEl.addEventListener('click', genaabnForslag);

	soegEl.addEventListener('keydown', (haendelse) => {
		switch (haendelse.key) {
			case 'ArrowDown':
				pegPaa(Math.min(markeret + 1, forslag.length - 1));
				break;
			case 'ArrowUp':
				pegPaa(Math.max(markeret - 1, 0));
				break;
			case 'Enter':
				// Er der kun ét forslag, er det det man mener.
				vaelgForslag(markeret >= 0 ? markeret : forslag.length === 1 ? 0 : -1);
				break;
			case 'Escape':
				if (soegEl.value === '' && bedrift !== null) {
					rydBedrift();
				} else {
					soegEl.value = '';
					soegEl.dispatchEvent(new Event('input'));
				}
				break;
			case 'Backspace':
				// Et tomt felt og én tast mere tilbage slipper bedriften.
				if (soegEl.value === '' && bedrift !== null) {
					rydBedrift();
					break;
				}
				return;
			default:
				return;
		}
		haendelse.preventDefault();
	});

	bedriftRydKnap.addEventListener('click', rydBedrift);

	// --- Panelerne ---
	//
	// Begge paneler kan trækkes i deres hoved og holdes inden for vinduet.
	// Hovedet skal altid kunne nås, så et panel kan ikke trækkes længere ned
	// end at det stadig ses.

	const MARGEN = 12;
	const SYNLIG = 48;

	const placer = (panel, venstre, top) => {
		const bredde = panel.offsetWidth;
		const x = Math.min(Math.max(venstre, MARGEN), Math.max(MARGEN, innerWidth - bredde - MARGEN));
		const y = Math.min(Math.max(top, MARGEN), Math.max(MARGEN, innerHeight - SYNLIG));
		panel.dataset.flyttet = '';
		Object.assign(panel.style, {
			left: `${x}px`,
			top: `${y}px`,
			right: 'auto',
			bottom: 'auto',
			// Et panel der er trukket langt ned, ruller hellere end at gå ud
			// over kanten.
			maxHeight: `calc(100dvh - ${y + MARGEN}px)`,
		});
	};

	// Et panel står i sit hjørne indtil det flyttes første gang. Derefter står
	// det hvor det blev sluppet, og trækkes ind igen hvis vinduet bliver
	// mindre eller panelet bredere.
	const holdInde = (panel) => {
		if (panel.hidden || !('flyttet' in panel.dataset)) return;
		const { left, top } = panel.getBoundingClientRect();
		placer(panel, left, top);
	};

	const traekbar = (panel) => {
		const haandtag = panel.querySelector('[data-haandtag]');
		// Kun den finger der tog fat, flytter panelet.
		let greb = null;

		haandtag.addEventListener('pointerdown', (haendelse) => {
			if (greb !== null || haendelse.button !== 0) return;
			if (haendelse.target.closest('button')) return;
			const { left, top } = panel.getBoundingClientRect();
			greb = {
				id: haendelse.pointerId,
				dx: haendelse.clientX - left,
				dy: haendelse.clientY - top,
			};
			haandtag.setPointerCapture(haendelse.pointerId);
			haendelse.preventDefault();
		});

		haandtag.addEventListener('pointermove', (haendelse) => {
			if (greb?.id !== haendelse.pointerId) return;
			placer(panel, haendelse.clientX - greb.dx, haendelse.clientY - greb.dy);
		});

		const slip = (haendelse) => {
			if (greb?.id !== haendelse.pointerId) return;
			haandtag.releasePointerCapture(haendelse.pointerId);
			greb = null;
		};
		haandtag.addEventListener('pointerup', slip);
		haandtag.addEventListener('pointercancel', slip);
	};

	traekbar(panelEl);
	traekbar(infoEl);

	addEventListener('resize', () => {
		holdInde(panelEl);
		holdInde(infoEl);
	});

	const minimer = (minimeret) => {
		panelIndholdEl.hidden = minimeret;
		minimerKnap.setAttribute('aria-expanded', String(!minimeret));
		minimerKnap.title = minimeret ? 'Fold panelet ud' : 'Minimér panelet';
		holdInde(panelEl);
	};

	minimerKnap.addEventListener('click', () => minimer(!panelIndholdEl.hidden));

	// På en telefon fylder panelet det meste af skærmen, så det starter
	// minimeret og kortet ses først. Grænsen er Tailwinds `sm`.
	if (matchMedia('(max-width: 639px)').matches) minimer(true);
})();
