//! Kortsiden og siden med kilder og vilkår.
//!
//! Serveren giver browseren alt den skal bruge i markup'en: landsdelenes
//! udstrækning, gruppernes farver, sprøjtelagets planperioder og klasser,
//! baggrundskortenes adresser og krediteringerne. `markkort.js` læser det
//! fra data-attributter.

use dkmarkkort_core::{
    gruppe::Gruppe,
    kilder::{self, Kilde},
};
use topcoat::{
    Result,
    asset::{Asset, asset},
    context::{Cx, app_context},
    router::page,
    tailwind,
    view::{Child, View, component, view},
};

use crate::{
    data::{Data, Kortdata, Landsdel, SproejtningUdgave, Udgave},
    sproejtning,
};

const OL_JS: Asset = asset!("assets/vendor/openlayers/ol.js");
const OL_CSS: Asset = asset!("assets/vendor/openlayers/ol.css");
const OL_LICENS: Asset = asset!(
    "assets/vendor/openlayers/LICENSE.md",
    content_type: "text/plain; charset=utf-8"
);
const KORT_JS: Asset = asset!("assets/markkort.js");

struct Baggrund {
    navn: &'static str,
    tiles: &'static str,
    kilde: &'static Kilde,
}

const BAGGRUNDE: [Baggrund; 2] = [
    Baggrund {
        navn: "Kort",
        tiles: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        kilde: &kilder::OPENSTREETMAP,
    },
    Baggrund {
        navn: "Satellit",
        tiles: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
        kilde: &kilder::ESRI_WORLD_IMAGERY,
    },
];

const OVERSKRIFT: &str = "mb-1.5 block text-xs font-medium text-stone-500";

/// En klasse i sprøjtelaget.
struct Klasse {
    /// Laveste belastning pr. hektar i klassen.
    fra: f64,
    navn: &'static str,
    farve: &'static str,
}

/// Sprøjtelagets klasser. Grænserne fordobles, så hver klasse rummer en
/// tydelig del af markerne (i 2024/25 36, 24, 30, 8 og 2,5 %), og de få
/// ekstreme værdier samles i den øverste. Farverne er ColorBrewers
/// "Oranges": én farvetone fra lys til mørk, som ingen afgrødegruppe bruger.
const BELASTNING: [Klasse; 5] = [
    Klasse {
        fra: 0.0,
        navn: "Under 1",
        farve: "#feedde",
    },
    Klasse {
        fra: 1.0,
        navn: "1–2",
        farve: "#fdbe85",
    },
    Klasse {
        fra: 2.0,
        navn: "2–4",
        farve: "#fd8d3c",
    },
    Klasse {
        fra: 4.0,
        navn: "4–8",
        farve: "#e6550d",
    },
    Klasse {
        fra: 8.0,
        navn: "8 eller mere",
        farve: "#a63603",
    },
];

/// De to paneler over kortet. De kan trækkes rundt i deres hoved, og deres
/// placering står derfor i `style` frem for i en klasse: `markkort.js`
/// overtager den første gang panelet flyttes.
const PANEL: &str = "absolute z-10 flex max-h-[calc(100dvh-1.5rem)] w-80 max-w-[calc(100vw-1.5rem)] flex-col rounded-xl border border-stone-200 bg-white/95 text-sm text-stone-800 shadow-md backdrop-blur";
const HOVED: &str =
    "flex cursor-grab touch-none items-center gap-2 px-4 py-3 select-none active:cursor-grabbing";
const HOVEDKNAP: &str = "-my-1 -mr-2 rounded-md px-2 py-1 text-stone-500 hover:bg-stone-100 hover:text-stone-900 aria-[expanded=false]:-rotate-90";
const KROP: &str = "min-h-0 space-y-4 overflow-y-auto border-t border-stone-200 px-4 py-3";
const FORSLAG: &str = "flex cursor-pointer items-center gap-2 px-2 py-1 aria-selected:bg-stone-100 data-valgt:font-semibold";

#[page("/")]
async fn kort(cx: &Cx) -> Result<impl View> {
    let kortdata = app_context::<Kortdata>(cx);
    let data = kortdata.hent();
    let fejlet = data.is_none() && kortdata.fejlet().await;
    Ok(view! {
        match data.as_deref() {
            Some(data) => kortside(data: data),
            None if fejlet => kunne_ikke_bygges(),
            None => bygges(),
        }
    })
}

/// Vises indtil pipelinen har bygget data første gang.
#[component]
async fn bygges() -> Result<impl View> {
    Ok(view! {
        dokument(
            titel: "Markkort",
            <main class="mx-auto max-w-2xl px-5 py-10 leading-relaxed text-stone-800">
                <h1 class="text-3xl font-semibold">"Kortdata bygges"</h1>
                <p class="mt-3 text-stone-600">
                    "Markerne hentes og gøres klar til kortet. Det tager et stykke tid "
                    "første gang. Prøv igen om lidt."
                </p>
            </main>
        )
    })
}

/// Vises i stedet for [`bygges`], når der ingen data er, og pipelinens
/// seneste forsøg fejlede.
#[component]
async fn kunne_ikke_bygges() -> Result<impl View> {
    Ok(view! {
        dokument(
            titel: "Markkort",
            <main class="mx-auto max-w-2xl px-5 py-10 leading-relaxed text-stone-800">
                <h1 class="text-3xl font-semibold">"Kortdata kunne ikke bygges"</h1>
                <p class="mt-3 text-stone-600">
                    "Det seneste forsøg på at gøre markerne klar til kortet fejlede. "
                    "Der prøves automatisk igen hver time."
                </p>
            </main>
        )
    })
}

#[component]
async fn kortside(data: &Data) -> Result<impl View> {
    let aar = data.udgave.as_ref().map_or("", |u| u.aar.as_str());
    let perioder = sproejtning::perioder(data);
    let kreditering = [
        kilder::MARKER.kreditering,
        kilder::AFGROEDEKODER.kreditering,
        kilder::LANDSDELE.kreditering,
    ]
    .join("|");

    Ok(view! {
        dokument(
            titel: "Markkort",
            <main class="relative h-dvh w-full overflow-hidden bg-stone-100">
                <div
                    id="kort"
                    class="absolute inset-0"
                    data-tiles="/tiles/{z}/{x}/{y}"
                    data-overblik="/overblik/{z}/{x}/{y}"
                    data-danmark=(udstraekning(&samlet(&data.landsdele)))
                    data-kreditering=(kreditering)
                    data-sproejtning=(if perioder.is_empty() { "" } else { "/sproejtning/{aar}/{z}/{x}/{y}" })
                    data-sproejtning-kreditering=(kilder::SPROEJTNING.kreditering)
                ></div>

                <section
                    id="panel"
                    class=(PANEL)
                    style="top: 0.75rem; right: 0.75rem"
                    aria-label="Markkort"
                >
                    <header class=(HOVED) data-haandtag="" title="Træk for at flytte panelet">
                        <h1 class="grow text-base font-semibold">"Markkort " (aar)</h1>
                        <span class="text-xs text-stone-500">(tal(data.marker_i_alt)) " marker"</span>
                        <button
                            type="button"
                            class=(HOVEDKNAP)
                            data-minimer=""
                            aria-expanded="true"
                            aria-controls="panel-indhold"
                            title="Minimér panelet"
                        >
                            <span aria-hidden="true">"⌄"</span>
                        </button>
                    </header>

                    <div id="panel-indhold" class=(KROP)>
                        <section>
                            <label for="soeg" class=(OVERSKRIFT)>"Find en bedrift og dens marker"</label>
                            <div
                                id="bedrift"
                                hidden=""
                                class="mb-1.5 flex items-center gap-2 rounded-md bg-stone-100 py-1 pr-1 pl-2"
                            >
                                <span class="grow">"CVR " <span class="tabular-nums" data-felt="cvr"></span></span>
                                <button
                                    type="button"
                                    id="bedrift-ryd"
                                    class="rounded px-1.5 text-stone-500 hover:bg-stone-200 hover:text-stone-900"
                                    title="Søg efter en anden bedrift"
                                >
                                    "✕"
                                </button>
                            </div>
                            <input
                                id="soeg"
                                type="search"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="CVR-nummer"
                                role="combobox"
                                aria-autocomplete="list"
                                aria-controls="forslag"
                                aria-expanded="false"
                                class="block w-full rounded-md border border-stone-300 bg-white px-2 py-1.5"
                            >
                            <ul
                                id="forslag"
                                role="listbox"
                                hidden=""
                                class="mt-1 max-h-64 overflow-y-auto rounded-md border border-stone-200 bg-white py-1"
                            ></ul>
                            <p id="soeg-besked" class="mt-1 text-xs text-stone-500 empty:hidden"></p>
                        </section>

                        <label class="block">
                            <span class=(OVERSKRIFT)>"Landsdel"</span>
                            <select
                                id="landsdel"
                                class="block w-full rounded-md border border-stone-300 bg-white px-2 py-1.5"
                            >
                                <option value="">"Hele landet"</option>
                                for landsdel in data.landsdele.iter() {
                                    <option
                                        value=(landsdel.nuts3.as_str())
                                        data-nr=(landsdel.nr)
                                        data-udstraekning=(udstraekning(&landsdel.udstraekning))
                                    >
                                        (landsdel.navn.as_str())
                                    </option>
                                }
                            </select>
                        </label>

                        <section>
                            <h2 class=(OVERSKRIFT)>"Afgrødegrupper"</h2>
                            <ul class="space-y-0.5">
                                for gruppe in Gruppe::ALLE {
                                    <li>
                                        <button
                                            type="button"
                                            class="group/knap flex w-full items-center gap-2 rounded-md px-2 py-1 text-left hover:bg-stone-100 aria-[pressed=false]:text-stone-400"
                                            aria-pressed="true"
                                            data-gruppe=(gruppe.noegle())
                                            data-nr=(gruppe.nr())
                                            data-farve=(gruppe.farve())
                                        >
                                            <span
                                                class="size-3.5 shrink-0 rounded-full ring-1 ring-black/20 group-aria-[pressed=false]/knap:opacity-20"
                                                style=(format!("background: {}", gruppe.farve()))
                                            ></span>
                                            <span class="grow">(gruppe.navn())</span>
                                            <span class="text-xs text-stone-500 tabular-nums">
                                                (tal(data.marker_pr_gruppe.get(&gruppe).copied().unwrap_or(0)))
                                            </span>
                                        </button>
                                    </li>
                                }
                            </ul>
                        </section>

                        if !perioder.is_empty() {
                            <section>
                                <div class="flex items-center justify-between gap-2">
                                    <h2 id="sproejtning-titel" class="text-xs font-medium text-stone-500">"Sprøjtning"</h2>
                                    <button
                                        type="button"
                                        id="sproejtning-kontakt"
                                        role="switch"
                                        aria-checked="false"
                                        aria-labelledby="sproejtning-titel"
                                        class="group/kontakt inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full bg-stone-300 transition-colors aria-checked:bg-[#a63603]"
                                    >
                                        <span class="size-4 translate-x-0.5 rounded-full bg-white shadow-sm transition-transform group-aria-checked/kontakt:translate-x-4.5"></span>
                                    </button>
                                </div>
                                <div id="sproejtning-indhold" hidden="" class="mt-2 space-y-2">
                                    <select
                                        id="sproejtning-aar"
                                        aria-label="Planperiode"
                                        class="block w-full rounded-md border border-stone-300 bg-white px-2 py-1.5"
                                    >
                                        for (aar, navn) in perioder.iter() {
                                            <option value=(aar)>"Planperiode " (navn.as_str())</option>
                                        }
                                    </select>
                                    <div>
                                        <p class="mb-1 text-xs text-stone-500">"Belastning pr. hektar"</p>
                                        <ul class="space-y-0.5">
                                            for klasse in BELASTNING.iter() {
                                                <li
                                                    class="flex items-center gap-2 px-2"
                                                    data-belastning-fra=(klasse.fra)
                                                    data-farve=(klasse.farve)
                                                >
                                                    <span
                                                        class="size-3.5 shrink-0 rounded-sm ring-1 ring-black/20"
                                                        style=(format!("background: {}", klasse.farve))
                                                    ></span>
                                                    (klasse.navn)
                                                </li>
                                            }
                                        </ul>
                                    </div>
                                    <p id="sproejtning-zoom" class="text-xs text-stone-700">
                                        "Zoom ind for at se de sprøjtede marker."
                                    </p>
                                    <p class="text-xs text-stone-500">
                                        "Bedrifterne indberetter deres forbrug pr. afgrøde, ikke pr. mark. "
                                        "Tallene er en bedrifts forbrug fordelt på dens marker med afgrøden, "
                                        "et skøn og ikke målinger."
                                    </p>
                                </div>
                            </section>
                        }

                        <section>
                            <h2 class=(OVERSKRIFT)>"Baggrund"</h2>
                            <div class="grid grid-cols-2 gap-1 rounded-lg bg-stone-100 p-1">
                                for (nr, baggrund) in BAGGRUNDE.iter().enumerate() {
                                    <button
                                        type="button"
                                        class="rounded-md px-2 py-1 text-stone-600 aria-pressed:bg-white aria-pressed:text-stone-900 aria-pressed:shadow-sm"
                                        aria-pressed=(if nr == 0 { "true" } else { "false" })
                                        data-baggrund=(baggrund.tiles)
                                        data-kreditering=(baggrund.kilde.kreditering)
                                        data-kreditering-url=(baggrund.kilde.url)
                                    >
                                        (baggrund.navn)
                                    </button>
                                }
                            </div>
                        </section>

                        <p class="text-xs text-stone-500">
                            "Zoom ind, og klik på en mark for at se hvad der dyrkes på den. "
                            <a class="underline hover:text-stone-800" href="/kilder">"Kilder og vilkår"</a>
                        </p>
                    </div>
                </section>

                <section
                    id="info"
                    hidden=""
                    class=(PANEL)
                    style="bottom: 0.75rem; left: 0.75rem"
                    aria-live="polite"
                    aria-label="Den valgte mark"
                >
                    <header class=(HOVED) data-haandtag="" title="Træk for at flytte panelet">
                        <span
                            class="size-3.5 shrink-0 rounded-full ring-1 ring-black/20"
                            data-felt="farve"
                        ></span>
                        <h2 class="grow text-base font-semibold">"Mark " <span data-felt="marknr"></span></h2>
                        <button type="button" class=(HOVEDKNAP) data-luk="" title="Fravælg marken">"✕"</button>
                    </header>
                    <div class=(KROP)>
                        <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 [&_dt]:text-stone-500">
                            <dt>"Afgrøde"</dt>
                            <dd>
                                <span data-felt="afgroede"></span>
                                <span class="text-xs text-stone-500 tabular-nums" data-felt="afgroedekode"></span>
                            </dd>
                            <dt>"Gruppe"</dt>
                            <dd data-felt="gruppe"></dd>
                            <dt>"Afsnit"</dt>
                            <dd data-felt="afsnit"></dd>
                            <dt>"Areal"</dt>
                            <dd class="tabular-nums" data-felt="areal"></dd>
                            <dt>"Landsdel"</dt>
                            <dd data-felt="landsdel"></dd>
                            <dt>"Bedrift"</dt>
                            <dd>
                                <button
                                    type="button"
                                    id="info-bedrift"
                                    class="text-left tabular-nums underline hover:text-stone-600 disabled:no-underline"
                                    title="Vis bedriftens marker"
                                    data-felt="cvr"
                                ></button>
                            </dd>
                        </dl>
                        <section id="info-sproejtning" hidden="" class="border-t border-stone-200 pt-3">
                            <h3 class=(OVERSKRIFT)>"Sprøjtet her"</h3>
                            <p class="mb-2 text-xs text-stone-500 empty:hidden" data-felt="besked"></p>
                            <ol id="historik" class="space-y-0.5"></ol>
                            <p class="mt-2 text-xs text-stone-500">
                                "Hver planperiode viser marken der lå her, med den afgrøde der voksede "
                                "der. Afgrødernes navne er fra kodelisten for " (aar) ". "
                                "Belastningen er pr. hektar, og tallene er et skøn, ikke målinger."
                            </p>
                        </section>
                    </div>
                </section>

                <template id="skabelon-bedrift">
                    <li role="option" class=(FORSLAG)>
                        <span class="grow">"CVR " <span class="tabular-nums" data-felt="cvr"></span></span>
                        <span class="text-xs text-stone-500 tabular-nums" data-felt="marker"></span>
                    </li>
                </template>
                <template id="skabelon-periode">
                    <li>
                        <details class="group/periode rounded-md data-valgt:bg-orange-50">
                            <summary class="flex cursor-pointer list-none items-center gap-2 rounded-md px-1.5 py-1 hover:bg-stone-100 [&::-webkit-details-marker]:hidden">
                                <span class="text-stone-400 transition-transform group-open/periode:rotate-90" aria-hidden="true">"›"</span>
                                <span class="size-3 shrink-0 rounded-sm ring-1 ring-black/20" data-felt="farve"></span>
                                <span class="shrink-0 tabular-nums" data-felt="planperiode"></span>
                                <span class="grow truncate" data-felt="afgroede"></span>
                                <span class="shrink-0 text-xs text-stone-500 tabular-nums" data-felt="belastning"></span>
                            </summary>
                            <div class="space-y-1 px-1.5 pt-1 pb-2 text-xs">
                                <p class="text-stone-500" data-felt="detaljer"></p>
                                <table class="w-full">
                                    <thead class="text-stone-500">
                                        <tr>
                                            <th class="text-left font-normal">"Middel"</th>
                                            <th class="text-right font-normal">"Pr. ha"</th>
                                            <th class="pl-2 text-right font-normal">"Belastning"</th>
                                        </tr>
                                    </thead>
                                    <tbody data-midler=""></tbody>
                                </table>
                            </div>
                        </details>
                    </li>
                </template>
                <template id="skabelon-middel">
                    <tr class="align-top">
                        <td class="py-0.5 pr-2">
                            <span data-felt="navn"></span>
                            " "
                            <span hidden="" class="rounded bg-stone-200 px-1 text-[10px] font-medium" data-pfas="">"PFAS"</span>
                        </td>
                        <td class="py-0.5 text-right whitespace-nowrap tabular-nums" data-felt="maengde"></td>
                        <td class="py-0.5 pl-2 text-right tabular-nums" data-felt="belastning"></td>
                    </tr>
                </template>
                <template id="skabelon-mark">
                    <li role="option" class=(FORSLAG)>
                        <span class="size-3 shrink-0 rounded-full ring-1 ring-black/20" data-felt="farve"></span>
                        <span class="w-12 shrink-0 font-medium tabular-nums" data-felt="marknr"></span>
                        <span class="grow truncate" data-felt="afgroede"></span>
                        <span class="text-xs text-stone-500 tabular-nums" data-felt="areal"></span>
                    </li>
                </template>
            </main>

            <script src=(OL_JS)></script>
            <script src=(KORT_JS)></script>
        )
    })
}

#[page("/kilder")]
async fn kilder_og_vilkaar(cx: &Cx) -> Result<impl View> {
    let data = app_context::<Kortdata>(cx).hent();
    Ok(view! {
        kildeside(
            udgave: data.as_ref().and_then(|d| d.udgave.as_ref()),
            sproejtning: data
                .as_ref()
                .and_then(|d| d.sproejtning.as_ref())
                .and_then(|s| s.udgave.as_ref()),
        )
    })
}

/// Kilderne med udgaven af markdata og sprøjtedata, hvis der er nogen endnu.
#[component]
async fn kildeside(
    udgave: Option<&Udgave>,
    sproejtning: Option<&SproejtningUdgave>,
) -> Result<impl View> {
    Ok(view! {
        dokument(
            titel: "Kilder og vilkår – Markkort",
            <main class="mx-auto max-w-2xl px-5 py-10 leading-relaxed text-stone-800 [&_a]:underline">
                <a class="text-sm" href="/">"Tilbage til kortet"</a>
                <h1 class="mt-6 text-3xl font-semibold">"Kilder og vilkår"</h1>
                <p class="mt-3 text-stone-600">
                    "Alt på kortet kommer fra kilderne nedenfor. For hver kilde står hvem der "
                    "udgiver den, hvilke vilkår der gælder, og hvordan vi har bearbejdet den."
                </p>

                for kilde in kilder::ALLE {
                    <section class="mt-10">
                        <h2 class="text-xl font-semibold">(kilde.titel)</h2>
                        <table class="mt-3 w-full text-sm">
                            <tbody class="[&_td]:py-1 [&_th]:w-32 [&_th]:py-1 [&_th]:pr-4 [&_th]:text-left [&_th]:align-top [&_th]:font-normal [&_th]:text-stone-500">
                                <tr>
                                    <th>"Udgiver"</th>
                                    <td><a href=(kilde.url)>(kilde.udgiver)</a></td>
                                </tr>
                                <tr>
                                    <th>"Licens"</th>
                                    <td>
                                        match kilde.licens_url {
                                            Some(url) => <a href=(url)>(kilde.licens)</a>,
                                            None => (kilde.licens),
                                        }
                                    </td>
                                </tr>
                                <tr>
                                    <th>"Kreditering"</th>
                                    <td>(kilde.kreditering)</td>
                                </tr>
                                if let Some(bearbejdning) = kilde.bearbejdning {
                                    <tr>
                                        <th>"Bearbejdning"</th>
                                        <td>(bearbejdning)</td>
                                    </tr>
                                }
                                if let Some(note) = kilde.note {
                                    <tr>
                                        <th>"Note"</th>
                                        <td>(note)</td>
                                    </tr>
                                }
                                if kilde.id == kilder::MARKER.id {
                                    if let Some(udgave) = udgave {
                                        <tr>
                                            <th>"Udgave"</th>
                                            <td>
                                                <a href=(udgave.url.as_str())>"Marker " (udgave.aar.as_str())</a>
                                                ", hentet " (dato(&udgave.hentet))
                                                if !udgave.sidst_aendret.is_empty() {
                                                    " (ændret " (udgave.sidst_aendret.as_str()) ")"
                                                }
                                            </td>
                                        </tr>
                                    }
                                }
                                if kilde.id == kilder::SPROEJTNING.id {
                                    if let Some(udgave) = sproejtning {
                                        <tr>
                                            <th>"Udgave"</th>
                                            <td>
                                                <a href=(udgave.url.as_str())>"Datasættet"</a>
                                                ", hentet " (dato(&udgave.hentet))
                                            </td>
                                        </tr>
                                    }
                                }
                            </tbody>
                        </table>
                    </section>
                }

                <section class="mt-10">
                    <h2 class="text-xl font-semibold">"Software"</h2>
                    <p class="mt-3 text-sm">
                        "Kortet tegnes af "
                        <a href="https://openlayers.org/">"OpenLayers"</a>
                        " 10.9.0, © OpenLayers Contributors, under BSD 2-Clause ("
                        <a href=(OL_LICENS)>"licens"</a>
                        "). Stylesheetet er genereret med "
                        <a href="https://tailwindcss.com/">"Tailwind CSS"</a>
                        " (MIT). Serveren bruger "
                        <a href="https://github.com/tokio-rs/topcoat">"topcoat"</a>
                        " og "
                        <a href="https://tokio.rs/">"tokio"</a>
                        " (MIT), og data er behandlet med "
                        <a href="https://gdal.org/">"GDAL"</a>
                        " og "
                        <a href="https://github.com/felt/tippecanoe">"tippecanoe"</a>
                        "."
                    </p>
                </section>
            </main>
        )
    })
}

#[component]
async fn dokument(titel: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="da">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(titel)</title>
                // Uden for Tailwinds lag, så knapperne i kortets kontroller
                // beholder OpenLayers' egen stil.
                <link rel="stylesheet" href=(OL_CSS)>
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
                topcoat::dev::script()
            </head>
            <body class="bg-white font-sans text-stone-900 antialiased">(child)</body>
        </html>
    })
}

/// Det mindste rektangel der rummer alle landsdelene.
fn samlet(landsdele: &[Landsdel]) -> [f64; 4] {
    landsdele.iter().fold(
        [f64::MAX, f64::MAX, f64::MIN, f64::MIN],
        |[v, s, o, n], l| {
            let [lv, ls, lo, ln] = l.udstraekning;
            [v.min(lv), s.min(ls), o.max(lo), n.max(ln)]
        },
    )
}

fn udstraekning([vest, syd, oest, nord]: &[f64; 4]) -> String {
    format!("{vest} {syd} {oest} {nord}")
}

/// Heltal med punktum som tusindtalsseparator.
fn tal(n: i64) -> String {
    let cifre = n.unsigned_abs().to_string();
    let mut ud = String::from(if n < 0 { "-" } else { "" });
    for (i, c) in cifre.chars().enumerate() {
        if i > 0 && (cifre.len() - i).is_multiple_of(3) {
            ud.push('.');
        }
        ud.push(c);
    }
    ud
}

/// Datodelen af et tidsstempel som pipelinen har skrevet ("2026-09-28T…").
fn dato(tidsstempel: &str) -> &str {
    tidsstempel.split('T').next().unwrap_or(tidsstempel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tal_faar_tusindtalsseparator() {
        assert_eq!(tal(7), "7");
        assert_eq!(tal(1000), "1.000");
        assert_eq!(tal(604793), "604.793");
        assert_eq!(tal(-12345), "-12.345");
    }

    #[test]
    fn dato_uden_klokkeslaet() {
        assert_eq!(dato("2026-09-28T13:09:01Z"), "2026-09-28");
        assert_eq!(dato("ukendt"), "ukendt");
    }

    #[test]
    fn samlet_udstraekning() {
        let l = |udstraekning| Landsdel {
            nr: 0,
            nuts3: String::new(),
            navn: String::new(),
            udstraekning,
        };
        assert_eq!(
            samlet(&[l([8.0, 55.0, 10.0, 56.0]), l([9.0, 54.5, 12.0, 57.0])]),
            [8.0, 54.5, 12.0, 57.0]
        );
    }
}
