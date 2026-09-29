# dkmarkkort

Kort over de marker landmændene har indberettet til Landbrugsstyrelsen, farvet
efter afgrødegruppe og med filter på landsdel og gruppe.

## Sådan hænger det sammen

```text
crates/core       fælles for pipeline og server: afgrødegrupper, datakilder, filnavne
crates/pipeline   henter markdata og bygger data/markkort.gpkg og data/marker.mbtiles
crates/server     webserveren (tokio + topcoat + Tailwind)
data/             dagi-landsdele.geojson og afgroedekoder-<år>.csv ligger i repoet
```

**Pipelinen** henter årets markkort fra LandbrugsGIS og bygger tre SQLite-filer:

- `markkort.gpkg` (GeoPackage) med alle marker, deres afgrødekode, afsnit og
  afgrødegruppe og den landsdel de ligger i, samt landsdelene, kodelisten og
  hvornår data er hentet.
- `marker.mbtiles` med markerne som vektortiles fra zoom 10 til 14, med
  alle marker i hver tile. En mark bærer kun sit id, sin gruppe og sin
  landsdel; resten står i databasen.
- `overblik.mbtiles` med markerne som PNG-tiles fra zoom 5 til 11 til kortet
  zoomet ud. Hver pixel er et tal for gruppe og landsdel, ikke en farve.

Rust henter, pakker ud og styrer. GDAL og tippecanoe gør det geografiske
arbejde.

**Afgrødegrupperne** kommer fra Landbrugsstyrelsens egen oversigt over
afgrødekoder, hvor hver kode står under et af 24 afsnit. Oversigten findes kun
som PDF, så den trækkes ud én gang om året til `data/afgroedekoder-<år>.csv`,
som gennemgås og committes:

```bash
cargo run -p dkmarkkort-pipeline --bin afgroedekoder -- oversigt.pdf data/afgroedekoder-2026.csv
```

De 24 afsnit er lagt sammen til seks grupper i `crates/core/src/gruppe.rs`.
Koder som oversigten ikke kender, vises som "Ukendt kode". Står en kode under
to afsnit med hver sin gruppe, stopper pipelinen, indtil valget er truffet i
`crates/pipeline/src/afgroedekoder.rs`.

**Serveren** læser de tre filer skrivebeskyttet, renderer siderne og leverer
vektortiles på `/tiles/{z}/{x}/{y}` og oversigten på `/overblik/{z}/{x}/{y}`.
Kortet i browseren er OpenLayers, som ligger i
`crates/server/assets/vendor/openlayers/`.

Zoomet ud er markerne for mange til at tegne hver for sig, så kortet viser
oversigten og farver dens pixels efter gruppe i et WebGL-lag. Fra zoom 11
tegnes markerne selv. Filtrene virker på begge, og et klik i oversigten
zoomer ind til markerne.

Klikker man på en mark, slår kortet den op på `/mark/{id}` og viser den i et
panel for sig. En bedrift findes på de første cifre af sit CVR-nummer
(`/soeg?q=`), og dens marker hentes på `/bedrift/{cvr}`, hvor de kan søges på
marknummer eller afgrøde. Begge paneler kan trækkes rundt i deres hoved.
Marker indberettet uden CVR-nummer kan vælges på kortet, men ikke søges frem.

## Kom i gang

Kræver Rust, GDAL, tippecanoe og topcoat-CLI'en. `pdftotext` (poppler) skal
kun bruges til at trække afgrødekoderne ud.

```bash
brew install gdal tippecanoe poppler
cargo install topcoat-cli --version 0.9.0 --locked

cargo run -p dkmarkkort-pipeline      # henter ~350 MB og bygger data/
sh run.sh                             # udviklingsserver på 0.0.0.0:3000, genstarter en kørende
```

Pipelinen tager `--aar` og `--data`. Serveren læser data fra `MARKKORT_DATA`
(standard `data`) og lytter på `HOST` og `PORT`.

Før push:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Docker

Imaget indeholder serveren. Data bygges med pipelinen og mountes:

```bash
docker build -t dkmarkkort .
docker run --rm -p 3000:3000 -v "$PWD/data:/data:ro" dkmarkkort
```

## Kilder og vilkår

| Data | Udgiver | Licens |
| --- | --- | --- |
| Markkort | [Landbrugsstyrelsen](https://landbrugsgeodata.fvm.dk/) | Ingen licens angivet |
| Oversigt over afgrødekoder | [Landbrugsstyrelsen](https://lbst.dk/tilskud/tast-selv/afgroedekoder) | Ingen licens angivet |
| Landsdele (DAGI) | [Klimadatastyrelsen](https://datafordeler.dk/vejledning/brugervilkaar/kds-geografiske-data/) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/deed.da) |
| Baggrundskort | [OpenStreetMap-bidragydere](https://www.openstreetmap.org/copyright) | [ODbL](https://opendatacommons.org/licenses/odbl/) |
| Satellitbilleder | [Esri](https://goto.arcgisonline.com/maps/World_Imagery) | Esris brugsvilkår |

Krediteringer, bearbejdning og noter står i `crates/core/src/kilder.rs` og
vises på kortet og på siden `/kilder`. Landbrugsstyrelsen oplyser ingen licens
for hverken markkortet eller kodeoversigten; det bør afklares med styrelsen.

## Licens

Koden er MIT-licenseret, se [LICENSE](LICENSE). OpenLayers 10.9.0 er BSD
2-Clause og ligger uændret fra npm-pakken `ol@10.9.0`.
