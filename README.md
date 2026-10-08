# dkmarkkort

Kort over de marker landmændene har indberettet til Landbrugsstyrelsen, farvet
efter afgrødegruppe og med filter på landsdel og gruppe, og over hvad der er
sprøjtet på markerne.

## Sådan hænger det sammen

```text
crates/core       fælles for pipeline og server: afgrødegrupper, datakilder, filnavne
crates/pipeline   henter markdata og sprøjtedata og bygger filerne i data/
crates/server     webserveren (tokio + topcoat + Tailwind)
data/             dagi-landsdele.geojson og afgroedekoder-<år>.csv ligger i repoet
```

**Pipelinen** har to dele, markkortet og sprøjtedata, og begge bygger deres
filer i `data/`. Markdelen henter årets markkort fra LandbrugsGIS og bygger tre
SQLite-filer:

- `markkort.gpkg` (GeoPackage) med alle marker, deres afgrødekode, afsnit og
  afgrødegruppe og den landsdel de ligger i, samt landsdelene, kodelisten med
  afgrødernes navne og hvornår data er hentet.
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

**Sprøjtedelen** henter pesticidforbruget fra landmændenes sprøjtejournaler,
som Landbruget.dk har fordelt ud på markerne og udgivet på
[Zenodo](https://zenodo.org/records/21072130). Landmændene indberetter
forbruget for hele bedriften pr. afgrøde, så tallene for en mark er en
beregnet fordeling og ikke målinger. Datasættet dækker planperioderne fra
2010/11 til 2024/25, dog ikke 2014/15. En planperiode går fra 1. august til
31. juli, og markerne er fra Fællesskemaet året efter, hvor afgrøden høstes.

Pipelinen slår nyeste version af datasættet op på Zenodo, henter dens zip
(2,6 GB) til `data/raw/sproejtning-<versionens id>.zip` og sletter tidligere
versioner. Den bygger:

- `sproejtning.gpkg` med de sprøjtede marker for hver planperiode, hvad der
  er brugt på dem, og midlerne. Hver mark har sin belastning pr. hektar
  (mængden af hvert middel gange middelets belastning, lagt sammen og delt
  med arealet), antal midler og om der er brugt PFAS-midler. Markerne ligger
  i WGS84, så serveren kan slå et klik på kortet op i GeoPackage'ens R-træ.
- `sproejtning-<år>.mbtiles` for hver planperiode, med markernes id og
  belastning fra zoom 10 til 14. Året er det år planperioden begynder.

Datasættet er Parquet, som Debians GDAL ikke kan læse. Pipelinen læser det
derfor selv fra zip'en og lader GDAL bygge geometrien ud fra WKB. Undervejs
rettes datasættets kendte fejl: geometrien er mærket WGS84 men er UTM32, og de
ældste år har samme mark flere gange. Pipelinen skriver for hver planperiode,
hvor mange marker og sprøjtninger der kom med, og hvad der blev sprunget over.

**Serveren** læser filerne skrivebeskyttet, renderer siderne og leverer
vektortiles på `/tiles/{z}/{x}/{y}`, oversigten på `/overblik/{z}/{x}/{y}` og de
sprøjtede marker for en planperiode på `/sproejtning/{aar}/{z}/{x}/{y}`. Kortet
i browseren er OpenLayers, som ligger i
`crates/server/assets/vendor/openlayers/`. Har pipelinen ikke bygget
sprøjtedata, vises kortet uden sprøjtelaget.

Zoomet ud er markerne for mange til at tegne hver for sig, så kortet viser
oversigten og farver dens pixels efter gruppe i et WebGL-lag. Fra zoom 11
tegnes markerne selv. Filtrene virker på begge, og et klik i oversigten
zoomer ind til markerne.

Klikker man på en mark, slår kortet den op på `/mark/{id}` og viser den i et
panel for sig. En bedrift findes på de første cifre af sit CVR-nummer
(`/soeg?q=`), og dens marker hentes på `/bedrift/{cvr}`, hvor de kan søges på
marknummer eller afgrøde. Begge paneler kan trækkes rundt i deres hoved.
Marker indberettet uden CVR-nummer samler pipelinen under CVR `00000000`, så de
kan søges frem som én bedrift. Kortet viser nummeret som "uden CVR-nummer".

**Sprøjtelaget** slås til med en kontakt i panelet og viser de sprøjtede marker
for den planperiode, der er valgt, farvet efter belastning pr. hektar i fem
klasser. Markerne bliver så grå flader under laget. Laget tegnes fra zoom 11,
ligesom markerne. Er det tændt, viser et klik på en mark også, hvad der er
sprøjtet på stedet i hver planperiode, med den afgrøde der voksede der, og
midlerne med mængde og belastning pr. hektar. Kortet spørger
`/sproejtning/sted?lon=&lat=`, og serveren finder markerne i R-træet og afgør
selv, om punktet ligger inde i dem (`crates/server/src/geometri.rs`).
Afgrødernes navne er fra årets kodeliste, som kender næsten alle tidligere
koder.

## Kom i gang

Kræver Rust, GDAL, tippecanoe og topcoat-CLI'en. `pdftotext` (poppler) skal
kun bruges til at trække afgrødekoderne ud.

```bash
brew install gdal tippecanoe poppler
cargo install topcoat-cli --version 0.9.0 --locked

cargo run -p dkmarkkort-pipeline      # henter ~3 GB og bygger data/ på omkring 20 minutter
sh run.sh                             # udviklingsserver på 0.0.0.0:3000, genstarter en kørende
```

Pipelinen tager `--aar` og `--data`. Med `--genbrug-sproejtning` springer
den sprøjtedelen over, hvis dataene i `data/` allerede er bygget fra nyeste
version af datasættet; så tager en kørsel et par minutter. Serveren læser data
fra `MARKKORT_DATA` (standard `data`) og lytter på `HOST` og `PORT`.

Før push:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Docker

Dockerfile'en bygger to images: serveren og pipelinen. I `compose.yaml` deler
de data i et volume, som serveren kun kan læse.

```bash
docker build -t dkmarkkort .
docker build --target pipeline -t dkmarkkort-pipeline .
```

Serveren starter også uden data og viser "Kortdata bygges", indtil de findes.
Den ser hvert 30. sekund efter filen `bygget`, som pipelinen skriver til
sidst, og skifter selv til de nye data uden genstart.

Pipeline-containeren tjekker én gang i døgnet, om data skal bygges, se
`deploy/pipeline.sh`:

- Når der ingen data er, bygges det hele. Det sker første gang containeren
  starter.
- Når pipelinens fingeraftryk (pipeline, core, inputfilerne i `data/` og
  `Cargo.lock`) har ændret sig, bygges det hele igen.
- Når data er mere end 30 dage gamle, hentes markkortet forfra, fordi
  Landbrugsstyrelsen opdaterer det løbende. Sprøjtedata bygges kun igen, hvis
  Landbruget.dk har udgivet en ny version af datasættet, hvilket sker
  sjældnere end én gang om året.

Fejler en kørsel, prøves der igen om en time, og kortet viser imens de data
det har.

**Nye versioner og gamle data.** Når en ny version deployes, ligger de gamle
data, mens pipelinen bygger nye, og serveren viser dem så godt den kan.
Kernen skal kunne læses: markerne, deres tiles, oversigten, landsdelene og
bedrifterne. Alt andet er tilvalg. Mangler et tilvalg, eller er det bygget af
en ældre pipeline, vises kortet uden det, og årsagen logges. Nye lag og
kolonner skal derfor læses som tilvalg i `crates/server/src/data.rs`.

En breaking change i data kræver, at `DATAVERSION` i `crates/core/src/lib.rs`
tælles op. Det er en ændring af noget serveren bruger, så data bygget før
ændringen ikke kan vises eller ville vise noget forkert, fx en kolonne der
får ny betydning, eller oversigtens pixels der kodes anderledes. Pipelinen
skriver versionen i `data/dataversion`, og serveren viser kun data i sin egen
version. Ellers står der "Kortdata opdateres", indtil pipelinen har bygget
nye data. Data uden versionsfil er version 1.

**Oprydning.** Det, de nye data erstatter, slettes, så volumet ikke vokser
fra kørsel til kørsel: et andet års markkort og tidligere versioner af
sprøjtedata i `raw/`, tiles for planperioder der ikke er med længere, og
`work/`, når en kørsel er lykkedes. Fejler den, bliver `work/` liggende til
fejlsøgning. Serveren slipper de gamle filer, når den har åbnet de nye. Efter
et vellykket deploy sletter workflowet de ældre images og byggecache der er
mere end en uge gammel.

Mellem kørslerne fylder volumet omkring 9 GB: de hentede zip-filer 3 GB,
markernes filer 1,2 GB og sprøjtningens 5,2 GB. Under en kørsel ligger de
gamle data, mens de nye bygges ved siden af, og volumet når op på omkring
25 GB. En kørsel tager omkring 20 minutter, og pipelinen selv bruger op til
2 GB hukommelse.

Data bygget lokalt kan også mountes direkte:

```bash
docker run --rm -p 3000:3000 -v "$PWD/data:/data:ro" dkmarkkort
```

## Kilder og vilkår

| Data | Udgiver | Licens |
| --- | --- | --- |
| Markkort | [Landbrugsstyrelsen](https://landbrugsgeodata.fvm.dk/) | Ingen licens angivet |
| Oversigt over afgrødekoder | [Landbrugsstyrelsen](https://lbst.dk/tilskud/tast-selv/afgroedekoder) | Ingen licens angivet |
| Landsdele (DAGI) | [Klimadatastyrelsen](https://datafordeler.dk/vejledning/brugervilkaar/kds-geografiske-data/) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/deed.da) |
| Sprøjtedata fordelt på marker | [Landbruget.dk](https://zenodo.org/records/21072130) efter Miljøstyrelsen og Landbrugsstyrelsen | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/deed.da) |
| Baggrundskort | [OpenStreetMap-bidragydere](https://www.openstreetmap.org/copyright) | [ODbL](https://opendatacommons.org/licenses/odbl/) |
| Satellitbilleder | [Esri](https://goto.arcgisonline.com/maps/World_Imagery) | Esris brugsvilkår |

Krediteringer, bearbejdning og noter står i `crates/core/src/kilder.rs` og
vises på kortet og på siden `/kilder`. Landbrugsstyrelsen oplyser ingen licens
for hverken markkortet eller kodeoversigten; det bør afklares med styrelsen.

## Licens

Koden er MIT-licenseret, se [LICENSE](LICENSE). OpenLayers 10.9.0 er BSD
2-Clause og ligger uændret fra npm-pakken `ol@10.9.0`.
