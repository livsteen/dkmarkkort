//! Sprøjtningen: pesticidforbruget fra sprøjtejournalerne, fordelt ud på de
//! marker det er brugt på.
//!
//! Landmændene indberetter forbruget til Miljøstyrelsen for hele bedriften,
//! pr. afgrøde og planperiode (1. august til 31. juli). Landbruget.dk har
//! fordelt det ud på bedriftens marker med den afgrøde og udgiver resultatet
//! på Zenodo som Parquet-filer i én zip, en fil pr. planperiode. Datasættets
//! år er det år planperioden begynder, og markerne er fra Fællesskemaet året
//! efter, hvor afgrøden høstes.
//!
//! Debians GDAL kan ikke læse Parquet, så Rust læser filerne direkte fra
//! zip'en ind i en arbejdsdatabase, med geometrien som WKB. GDAL laver den
//! derfra om til en GeoPackage og vektortiles. Resultatet er:
//!
//! - [`SPROEJTNING_DATABASE_FIL`]: `sproejtemarker` med geometri,
//!   planperiode, afgrødekode, areal, belastning pr. hektar, antal midler og
//!   om der er brugt PFAS-midler; `sproejtninger` med hvad der er brugt på
//!   hver mark; `midler` og `datakilde`.
//! - [`sproejtning_tiles_fil`] for hver planperiode.
//!
//! Datasættet hentes altid i nyeste version. Landbruget.dk udgiver en ny,
//! når der er kommet en planperiode til, så det sker sjældent. Med
//! `genbrug` bygges sprøjtningen kun igen, hvis der er kommet en ny version,
//! siden de data der ligger, blev bygget.

use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use dkmarkkort_core::{
    ENHED_KG, ENHED_LITER, SPROEJTNING_DATABASE_FIL, SPROEJTNING_LAG, kilder, planperiode,
    sproejtning_tiles_fil,
};
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::Field,
    schema::types::Type,
};
use serde::Deserialize;
use sqlx::{
    AssertSqlSafe, Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use zip::ZipArchive;

use crate::{
    hent::{Hentet, hent, klient},
    slet, utf8,
    vaerktoej::{ogr2ogr, ogrinfo_sql, tippecanoe},
};

/// Datasættets id på Zenodo på tværs af versioner. Hver version har
/// desuden sit eget id.
const ZENODO_DATASAET: &str = "21072130";

/// En version af datasættet, som Zenodo beskriver den.
#[derive(Deserialize)]
struct Version {
    /// Versionens eget id.
    id: u64,
    metadata: Metadata,
    files: Vec<Fil>,
}

#[derive(Deserialize)]
struct Metadata {
    version: String,
}

#[derive(Deserialize)]
struct Fil {
    key: String,
    links: Links,
}

#[derive(Deserialize)]
struct Links {
    #[serde(rename = "self")]
    adresse: String,
}

/// Filerne pipelinen har bygget i `work`.
pub struct Bygget {
    pub database: PathBuf,
    /// Planperiodens første år og tiles'ene for den.
    pub tiles: Vec<(u16, PathBuf)>,
}

/// En mark som datasættet har den: id'et skal kun være unikt inden for sin
/// planperiode.
struct Mark {
    uuid: String,
    afgkode: Option<i64>,
    areal_ha: f64,
    wkb: Vec<u8>,
}

/// Et middels samlede mængde på én mark i én planperiode.
struct Sproejtning {
    uuid: String,
    regnr: String,
    /// Bruges kun til midler som produktlisten ikke kender.
    navn: String,
    maengde: f64,
    enhed: i64,
    behandlet_ha: f64,
    tillid: f64,
}

struct Middel {
    regnr: String,
    navn: String,
    /// Ukendt for nogle få husholdningsmidler, som ikke bruges på marker.
    pfas: Option<bool>,
    belastning: f64,
}

/// Henter nyeste version af datasættet og bygger databasen og tiles'ene i
/// `work`. Med `genbrug` bygges intet, hvis dataene i `data` allerede er
/// fra den version.
pub async fn byg(data: &Path, raw: &Path, work: &Path, genbrug: bool) -> Result<Option<Bygget>> {
    let (version, url) = nyeste_version().await?;
    println!(
        "    nyeste version er {} (Zenodo {})",
        version.metadata.version, version.id
    );
    let eksisterende = data.join(SPROEJTNING_DATABASE_FIL);
    if genbrug && bygget_fra(&eksisterende).await?.as_ref() == Some(&url) {
        println!("    de data der ligger, er bygget fra den og genbruges");
        return Ok(None);
    }

    let zip = raw.join(format!("sproejtning-{}.zip", version.id));
    let hentet = hent(&url, &zip).await?;
    slet_andre_versioner(raw, &zip).await?;
    let perioder = laes_blokerende(&zip, planperioder).await?;
    if perioder.is_empty() {
        bail!("{} har ingen planperioder", zip.display());
    }

    // Arbejdsdatabasen er en almindelig SQLite-fil, som kun bruges her og
    // bygges forfra hver gang, så den skrives uden journal.
    let arbejd = work.join("sproejtning-raa.sqlite");
    slet(&arbejd).await?;
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&arbejd)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off)
            .synchronous(SqliteSynchronous::Off),
    )
    .await?;
    opret_tabeller(&mut db).await?;
    skriv_datakilde(&mut db, &hentet).await?;

    let midler = laes_blokerende(&zip, |arkiv| {
        laes_midler(&laes_parquet(arkiv, "products/products.parquet")?)
    })
    .await?;
    skriv_midler(&mut db, &midler).await?;
    println!("    {} midler", midler.len());

    for &aar in &perioder {
        let marker = laes_blokerende(&zip, move |arkiv| {
            laes_marker(&laes_parquet(
                arkiv,
                &format!("fields/year={aar}/part-000.parquet"),
            )?)
        })
        .await?;
        let dubletter = skriv_marker(&mut db, aar, &marker).await?;
        if dubletter > 0 {
            println!(
                "    {}: {dubletter} af {} marker er dubletter",
                planperiode(aar),
                marker.len()
            );
        }
        drop(marker);

        let sproejtninger = laes_blokerende(&zip, move |arkiv| {
            laes_sproejtninger(&laes_parquet(
                arkiv,
                &format!("use_allocations/year={aar}/part-000.parquet"),
            )?)
        })
        .await?;
        skriv_sproejtninger(&mut db, aar, &sproejtninger).await?;
        drop(sproejtninger);
    }

    knyt_til_marker(&mut db, &perioder).await?;
    db.close().await?;

    let database = work.join(SPROEJTNING_DATABASE_FIL);
    byg_database(&arbejd, &database).await?;
    // Arbejdsdatabasen fylder flere GB og bruges ikke igen.
    slet(&arbejd).await?;

    let mut tiles = Vec::new();
    for &aar in &perioder {
        tiles.push((aar, byg_tiles(&database, work, aar).await?));
    }
    Ok(Some(Bygget { database, tiles }))
}

/// Nyeste version af datasættet og adressen på dens zip.
async fn nyeste_version() -> Result<(Version, String)> {
    let adresse = format!("https://zenodo.org/api/records/{ZENODO_DATASAET}/versions/latest");
    let svar = klient()?
        .get(&adresse)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .with_context(|| format!("kunne ikke slå nyeste version op på {adresse}"))?;
    version_fra_json(&svar.text().await?)
}

fn version_fra_json(json: &str) -> Result<(Version, String)> {
    let version: Version = serde_json::from_str(json).context("Zenodo svarede ikke som ventet")?;
    let url = version
        .files
        .iter()
        .find(|fil| fil.key.ends_with(".zip"))
        .map(|fil| fil.links.adresse.clone())
        .with_context(|| format!("version {} har ingen zip", version.id))?;
    Ok((version, url))
}

/// Adressen på den zip, `database` er bygget fra, hvis den findes.
async fn bygget_fra(database: &Path) -> Result<Option<String>> {
    if !tokio::fs::try_exists(database).await? {
        return Ok(None);
    }
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(database)
            .read_only(true),
    )
    .await?;
    let url = sqlx::query_scalar("SELECT url FROM datakilde WHERE id = ?")
        .bind(kilder::SPROEJTNING.id)
        .fetch_optional(&mut db)
        .await?;
    db.close().await?;
    Ok(url)
}

/// Sletter tidligere versioner af datasættet i `raw`. De fylder 2–3 GB
/// hver og bruges ikke igen.
async fn slet_andre_versioner(raw: &Path, zip: &Path) -> Result<()> {
    let zip_navn = zip
        .file_name()
        .and_then(|navn| navn.to_str())
        .context("zip'en har intet navn")?;
    let behold = [zip_navn.to_owned(), format!("{zip_navn}.hentet")];
    let mut indhold = tokio::fs::read_dir(raw).await?;
    while let Some(post) = indhold.next_entry().await? {
        let navn = post.file_name().to_string_lossy().into_owned();
        let tidligere = navn.starts_with("sproejtning-")
            && (navn.ends_with(".zip") || navn.ends_with(".zip.hentet"))
            && !behold.contains(&navn);
        if tidligere {
            println!("    sletter {navn}, som er en tidligere version");
            slet(&post.path()).await?;
        }
    }
    Ok(())
}

/// Kører `laes` med zip'en åben på en tråd hvor den må blokere: Parquet
/// læses og pakkes ud synkront.
async fn laes_blokerende<T, F>(zip: &Path, laes: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut ZipArchive<File>) -> Result<T> + Send + 'static,
{
    let zip = zip.to_owned();
    tokio::task::spawn_blocking(move || {
        let fil = File::open(&zip).with_context(|| format!("kunne ikke åbne {}", zip.display()))?;
        let mut arkiv = ZipArchive::new(fil)?;
        laes(&mut arkiv).with_context(|| format!("kunne ikke læse {}", zip.display()))
    })
    .await?
}

/// Planperioderne der både har marker og sprøjtninger i zip'en, i rækkefølge.
fn planperioder<R: Read + Seek>(arkiv: &mut ZipArchive<R>) -> Result<Vec<u16>> {
    let aar_i = |mappe: &str| -> Vec<u16> {
        arkiv
            .file_names()
            .filter_map(|navn| {
                navn.strip_prefix(mappe)?
                    .strip_suffix("/part-000.parquet")?
                    .parse()
                    .ok()
            })
            .collect()
    };
    let marker = aar_i("fields/year=");
    let mut perioder: Vec<u16> = aar_i("use_allocations/year=")
        .into_iter()
        .filter(|aar| marker.contains(aar))
        .collect();
    perioder.sort_unstable();
    Ok(perioder)
}

/// En Parquet-fil fra zip'en. Den pakkes ud i hukommelsen, fordi Parquet
/// skal læses bagfra, og en komprimeret post i en zip kun kan læses forfra.
/// Den største fil er omkring 150 MB.
fn laes_parquet(arkiv: &mut ZipArchive<File>, navn: &str) -> Result<SerializedFileReader<Bytes>> {
    let mut post = arkiv
        .by_name(navn)
        .with_context(|| format!("{navn} mangler"))?;
    let mut indhold = Vec::with_capacity(usize::try_from(post.size()).unwrap_or(0));
    post.read_to_end(&mut indhold)?;
    SerializedFileReader::new(Bytes::from(indhold))
        .with_context(|| format!("{navn} er ikke Parquet"))
}

/// Læser kolonnerne `navne` fra hver række i `fil`, i den rækkefølge.
fn laes_raekker(
    fil: &SerializedFileReader<Bytes>,
    navne: &[&str],
    mut raekke: impl FnMut(Vec<Field>) -> Result<()>,
) -> Result<()> {
    let rod = fil.metadata().file_metadata().schema();
    let kolonner = navne
        .iter()
        .map(|navn| {
            rod.get_fields()
                .iter()
                .find(|kolonne| kolonne.name() == *navn)
                .cloned()
                .with_context(|| format!("kolonnen {navn} mangler"))
        })
        .collect::<Result<Vec<_>>>()?;
    let projektion = Type::group_type_builder(rod.name())
        .with_fields(kolonner)
        .build()?;

    for (nr, naeste) in fil.get_row_iter(Some(projektion))?.enumerate() {
        let kolonner = naeste?.into_columns();
        // Rækkefølgen følger projektionen. Det tjekkes én gang, så en
        // ændring i parquet-crate'en ikke stille bytter om på værdierne.
        if nr == 0 {
            let fundet: Vec<&str> = kolonner.iter().map(|(navn, _)| navn.as_str()).collect();
            if fundet != navne {
                bail!("kolonnerne kom som {fundet:?}, ikke {navne:?}");
            }
        }
        raekke(kolonner.into_iter().map(|(_, felt)| felt).collect())
            .with_context(|| format!("række {nr}"))?;
    }
    Ok(())
}

fn laes_midler(fil: &SerializedFileReader<Bytes>) -> Result<Vec<Middel>> {
    let mut midler = Vec::new();
    laes_raekker(
        fil,
        &[
            "pesticide_registration_number",
            "product_name",
            "pfas_flag",
            "samlet_belastning",
        ],
        |felter| {
            let [regnr, navn, pfas, belastning] = felter_som(felter)?;
            midler.push(Middel {
                regnr: tekst(regnr)?,
                navn: tekst(navn)?,
                pfas: sandhed_eller_ukendt(pfas)?,
                belastning: tal(belastning)?,
            });
            Ok(())
        },
    )?;
    Ok(midler)
}

fn laes_marker(fil: &SerializedFileReader<Bytes>) -> Result<Vec<Mark>> {
    let mut marker = Vec::new();
    laes_raekker(
        fil,
        &["field_uuid", "crop_code", "area_ha", "geometry"],
        |felter| {
            let [uuid, afgkode, areal_ha, geometri] = felter_som(felter)?;
            marker.push(Mark {
                uuid: tekst(uuid)?,
                // Afgrødekoden er tekst i datasættet, men et tal i
                // Fællesskemaet og i resten af kortet.
                afgkode: tekst(afgkode)?.trim().parse().ok(),
                areal_ha: tal(areal_ha)?,
                wkb: binaer(geometri)?,
            });
            Ok(())
        },
    )?;
    Ok(marker)
}

fn laes_sproejtninger(fil: &SerializedFileReader<Bytes>) -> Result<Vec<Sproejtning>> {
    let mut sproejtninger = Vec::new();
    laes_raekker(
        fil,
        &[
            "field_uuid",
            "pesticide_registration_number",
            "pesticide_name",
            "allocated_quantity",
            "allocated_quantity_unit",
            "allocated_cumulative_treated_area_ha",
            "match_confidence",
        ],
        |felter| {
            let [uuid, regnr, navn, maengde, enhed, behandlet_ha, tillid] = felter_som(felter)?;
            let enhed = tekst(enhed)?;
            sproejtninger.push(Sproejtning {
                uuid: tekst(uuid)?,
                regnr: tekst(regnr)?,
                navn: tekst(navn)?,
                maengde: tal(maengde)?,
                enhed: enhed
                    .trim()
                    .parse()
                    .with_context(|| format!("enheden {enhed:?} er ikke en kode"))?,
                behandlet_ha: tal(behandlet_ha)?,
                tillid: tal(tillid)?,
            });
            Ok(())
        },
    )?;
    Ok(sproejtninger)
}

fn felter_som<const N: usize>(felter: Vec<Field>) -> Result<[Field; N]> {
    felter
        .try_into()
        .map_err(|felter: Vec<Field>| anyhow::anyhow!("{} kolonner, ikke {N}", felter.len()))
}

fn tekst(felt: Field) -> Result<String> {
    match felt {
        Field::Str(tekst) => Ok(tekst),
        andet => bail!("forventede tekst, fik {andet}"),
    }
}

fn tal(felt: Field) -> Result<f64> {
    match felt {
        Field::Double(tal) => Ok(tal),
        Field::Float(tal) => Ok(tal.into()),
        Field::Int(tal) => Ok(tal.into()),
        andet => bail!("forventede et tal, fik {andet}"),
    }
}

fn sandhed_eller_ukendt(felt: Field) -> Result<Option<bool>> {
    match felt {
        Field::Bool(sandhed) => Ok(Some(sandhed)),
        Field::Null => Ok(None),
        andet => bail!("forventede sand eller falsk, fik {andet}"),
    }
}

fn binaer(felt: Field) -> Result<Vec<u8>> {
    match felt {
        Field::Bytes(data) => Ok(data.data().to_vec()),
        andet => bail!("forventede binære data, fik {andet}"),
    }
}

async fn opret_tabeller(db: &mut SqliteConnection) -> Result<()> {
    for sql in [
        // Midler som produktlisten ikke kender, har hverken PFAS eller
        // belastning.
        "CREATE TABLE midler (
             regnr TEXT PRIMARY KEY,
             navn TEXT NOT NULL,
             pfas INTEGER,
             belastning REAL)",
        // Markens id er dens fid i GeoPackage'en og går på tværs af
        // planperioderne.
        "CREATE TABLE marker (
             id INTEGER PRIMARY KEY,
             aar INTEGER NOT NULL,
             uuid TEXT NOT NULL,
             afgkode INTEGER,
             areal_ha REAL NOT NULL,
             wkb BLOB NOT NULL,
             UNIQUE (aar, uuid))",
        "CREATE TABLE raa_sproejtninger (
             aar INTEGER NOT NULL,
             uuid TEXT NOT NULL,
             regnr TEXT NOT NULL,
             navn TEXT NOT NULL,
             maengde REAL NOT NULL,
             enhed INTEGER NOT NULL,
             behandlet_ha REAL NOT NULL,
             tillid REAL NOT NULL)",
        "CREATE TABLE datakilde (
             id TEXT NOT NULL,
             url TEXT NOT NULL,
             hentet TEXT NOT NULL,
             sidst_aendret TEXT NOT NULL)",
    ] {
        sqlx::query(sql).execute(&mut *db).await?;
    }
    Ok(())
}

async fn skriv_datakilde(db: &mut SqliteConnection, hentet: &Hentet) -> Result<()> {
    sqlx::query("INSERT INTO datakilde VALUES (?, ?, ?, ?)")
        .bind(kilder::SPROEJTNING.id)
        .bind(&hentet.url)
        .bind(&hentet.hentet)
        .bind(hentet.sidst_aendret.as_deref().unwrap_or(""))
        .execute(db)
        .await?;
    Ok(())
}

/// To produkter kan dele registreringsnummer. Det første beholdes; i
/// datasættet gælder det kun halsbånd til kæledyr.
async fn skriv_midler(db: &mut SqliteConnection, midler: &[Middel]) -> Result<()> {
    let mut tx = db.begin().await?;
    for middel in midler {
        sqlx::query("INSERT INTO midler VALUES (?, ?, ?, ?) ON CONFLICT (regnr) DO NOTHING")
            .bind(&middel.regnr)
            .bind(&middel.navn)
            .bind(middel.pfas)
            .bind(middel.belastning)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Skriver markerne og returnerer hvor mange der var dubletter. I de
/// ældste år står mange marker flere gange med samme id og geometri; den
/// første beholdes.
async fn skriv_marker(db: &mut SqliteConnection, aar: u16, marker: &[Mark]) -> Result<u64> {
    let mut tx = db.begin().await?;
    let mut dubletter = 0;
    for mark in marker {
        let skrevet = sqlx::query(
            "INSERT INTO marker (aar, uuid, afgkode, areal_ha, wkb) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (aar, uuid) DO NOTHING",
        )
        .bind(aar)
        .bind(&mark.uuid)
        .bind(mark.afgkode)
        .bind(mark.areal_ha)
        .bind(&mark.wkb)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if skrevet == 0 {
            dubletter += 1;
        }
    }
    tx.commit().await?;
    Ok(dubletter)
}

async fn skriv_sproejtninger(
    db: &mut SqliteConnection,
    aar: u16,
    sproejtninger: &[Sproejtning],
) -> Result<()> {
    let mut tx = db.begin().await?;
    for sproejtning in sproejtninger {
        sqlx::query("INSERT INTO raa_sproejtninger VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(aar)
            .bind(&sproejtning.uuid)
            .bind(&sproejtning.regnr)
            .bind(&sproejtning.navn)
            .bind(sproejtning.maengde)
            .bind(sproejtning.enhed)
            .bind(sproejtning.behandlet_ha)
            .bind(sproejtning.tillid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Sætter markens id på hver sprøjtning og regner markens belastning, antal
/// midler og PFAS ud. En sprøjtning på en mark uden geometri kan ikke vises
/// og tælles kun. De ældste år har også marker uden sprøjtninger; de kommer
/// ikke med, for datasættet siger ikke, at de er usprøjtede.
///
/// Et middel som produktlisten ikke kender, får sit navn fra sprøjtningerne
/// og tæller ikke med i belastningen, fordi den er ukendt.
async fn knyt_til_marker(db: &mut SqliteConnection, perioder: &[u16]) -> Result<()> {
    sqlx::query(
        "INSERT INTO midler (regnr, navn)
         SELECT regnr, MIN(navn) FROM raa_sproejtninger
         WHERE regnr NOT IN (SELECT regnr FROM midler)
         GROUP BY regnr",
    )
    .execute(&mut *db)
    .await?;
    sqlx::query(
        "CREATE TABLE sproejtninger AS
         SELECT m.id AS mark, s.regnr, s.maengde, s.enhed, s.behandlet_ha, s.tillid
         FROM raa_sproejtninger s JOIN marker m ON m.aar = s.aar AND m.uuid = s.uuid",
    )
    .execute(&mut *db)
    .await?;
    // Enhederne er konstanter i koden, ikke input.
    sqlx::query(AssertSqlSafe(format!(
        "CREATE TABLE pr_mark AS
         SELECT s.mark,
                SUM(CASE WHEN s.enhed IN ({ENHED_KG}, {ENHED_LITER})
                         THEN s.maengde * COALESCE(mi.belastning, 0) ELSE 0 END) AS belastning,
                COUNT(DISTINCT s.regnr) AS antal_midler,
                COALESCE(MAX(mi.pfas), 0) AS pfas
         FROM sproejtninger s LEFT JOIN midler mi ON mi.regnr = s.regnr
         GROUP BY s.mark"
    )))
    .execute(&mut *db)
    .await?;

    for &aar in perioder {
        let (marker, uden_sproejtning, sproejtninger, uden_mark, uden_belastning): (
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM pr_mark p
                           JOIN marker m ON m.id = p.mark WHERE m.aar = ?1),
                        (SELECT COUNT(*) FROM marker m WHERE m.aar = ?1
                           AND m.id NOT IN (SELECT mark FROM pr_mark)),
                        (SELECT COUNT(*) FROM sproejtninger s
                           JOIN marker m ON m.id = s.mark WHERE m.aar = ?1),
                        (SELECT COUNT(*) FROM raa_sproejtninger s WHERE s.aar = ?1
                           AND NOT EXISTS (SELECT 1 FROM marker m
                                           WHERE m.aar = s.aar AND m.uuid = s.uuid)),
                        (SELECT COUNT(*) FROM raa_sproejtninger s WHERE s.aar = ?1
                           AND s.regnr IN (SELECT regnr FROM midler
                                           WHERE belastning IS NULL))",
        )
        .bind(aar)
        .fetch_one(&mut *db)
        .await?;
        println!(
            "    {}: {marker} marker, {sproejtninger} sprøjtninger \
             ({uden_sproejtning} marker uden sprøjtning, {uden_mark} sprøjtninger uden mark, \
             {uden_belastning} med middel uden belastning)",
            planperiode(aar)
        );
        if marker == 0 || sproejtninger == 0 {
            bail!(
                "planperioden {} har ingen sprøjtede marker",
                planperiode(aar)
            );
        }
    }
    Ok(())
}

/// Lægger arbejdsdatabasen over i en GeoPackage. Belastningen står pr.
/// hektar, så store og små marker kan sammenlignes.
async fn byg_database(arbejd: &Path, database: &Path) -> Result<()> {
    slet(database).await?;
    // Datasættet angiver ingen projektion, så GDAL ville gætte på WGS84,
    // men koordinaterne er UTM32 ligesom Fællesskemaets. De lægges i WGS84,
    // så serveren kan slå et klik på kortet op i R-træet uden selv at
    // omregne. Kolonnen fid bliver GeoPackage'ens fid, så sprøjtningerne
    // kan slå marken op.
    ogr2ogr([
        "-f",
        "GPKG",
        utf8(database),
        utf8(arbejd),
        "-nln",
        "sproejtemarker",
        "-nlt",
        "MULTIPOLYGON",
        "-s_srs",
        "EPSG:25832",
        "-t_srs",
        "EPSG:4326",
        "-lco",
        "GEOMETRY_NAME=geom",
        "-dialect",
        "SQLITE",
        "-sql",
        "SELECT m.id AS fid, GeomFromWKB(m.wkb) AS geom,
                m.aar, m.afgkode, m.areal_ha,
                p.belastning / NULLIF(m.areal_ha, 0) AS belastning,
                p.antal_midler, p.pfas
         FROM marker m JOIN pr_mark p ON p.mark = m.id",
    ])
    .await?;
    for tabel in ["sproejtninger", "midler", "datakilde"] {
        ogr2ogr([
            "-update",
            "-f",
            "GPKG",
            utf8(database),
            utf8(arbejd),
            tabel,
            "-nln",
            tabel,
        ])
        .await?;
    }
    // Kortet viser én planperiode ad gangen, og et klik på en mark slår
    // dens sprøjtninger op.
    ogrinfo_sql(
        database,
        "CREATE INDEX sproejtemarker_aar ON sproejtemarker (aar)",
    )
    .await?;
    ogrinfo_sql(
        database,
        "CREATE INDEX sproejtninger_mark ON sproejtninger (mark)",
    )
    .await?;
    Ok(())
}

/// Vektortiles for én planperiode. Markerne bærer kun deres id og
/// belastning; resten slås op i databasen. Zoomniveauerne er de samme som
/// for markerne.
async fn byg_tiles(database: &Path, work: &Path, aar: u16) -> Result<PathBuf> {
    let geojsonl = work.join(format!("sproejtning-{aar}.geojsonl"));
    slet(&geojsonl).await?;
    let sql = format!(
        "SELECT fid, ROUND(belastning, 2) AS belastning, geom
         FROM sproejtemarker WHERE aar = {aar}"
    );
    ogr2ogr([
        "-f",
        "GeoJSONSeq",
        utf8(&geojsonl),
        utf8(database),
        "-sql",
        sql.as_str(),
        "-t_srs",
        "EPSG:4326",
        "-preserve_fid",
    ])
    .await?;

    let tiles = work.join(sproejtning_tiles_fil(aar));
    let navn = format!("dkmarkkort-sproejtning-{aar}");
    tippecanoe([
        "-o",
        utf8(&tiles),
        "-l",
        SPROEJTNING_LAG,
        "-n",
        navn.as_str(),
        "-A",
        kilder::SPROEJTNING.kreditering,
        "-Z10",
        "-z14",
        "--detect-shared-borders",
        "--force",
        utf8(&geojsonl),
    ])
    .await?;
    slet(&geojsonl).await?;
    Ok(tiles)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    #[test]
    fn kun_planperioder_med_baade_marker_og_sproejtninger() {
        let mut skriver = ZipWriter::new(Cursor::new(Vec::new()));
        for navn in [
            "README.md",
            "fields/year=2024/part-000.parquet",
            "fields/year=2010/part-000.parquet",
            "use_allocations/year=2024/part-000.parquet",
            "use_allocations/year=2011/part-000.parquet",
            "use_allocations/year=2010/part-000.parquet",
            "quality/year=2024/coverage.json",
        ] {
            skriver
                .start_file(navn, SimpleFileOptions::default())
                .unwrap();
            skriver.write_all(b"-").unwrap();
        }
        let mut arkiv = ZipArchive::new(skriver.finish().unwrap()).unwrap();
        assert_eq!(planperioder(&mut arkiv).unwrap(), [2010, 2024]);
    }

    /// Et afkortet svar fra Zenodo med de felter pipelinen bruger.
    const ZENODO_SVAR: &str = r#"{
        "id": 21072131,
        "conceptrecid": "21072130",
        "metadata": {"title": "Pesticides on 2.7 Million Danish Fields", "version": "v1"},
        "files": [
            {"key": "README.txt", "links": {"self": "https://zenodo.org/api/records/21072131/files/README.txt/content"}},
            {"key": "pesticide-field-use-allocations-v1.zip", "size": 2624735611,
             "links": {"self": "https://zenodo.org/api/records/21072131/files/pesticide-field-use-allocations-v1.zip/content"}}
        ]
    }"#;

    #[test]
    fn nyeste_version_peger_paa_zippen() {
        let (version, url) = version_fra_json(ZENODO_SVAR).unwrap();
        assert_eq!(version.id, 21072131);
        assert_eq!(version.metadata.version, "v1");
        assert_eq!(
            url,
            "https://zenodo.org/api/records/21072131/files/pesticide-field-use-allocations-v1.zip/content"
        );
    }

    #[test]
    fn en_version_uden_zip_er_en_fejl() {
        let uden_zip = r#"{"id": 1, "metadata": {"version": "v9"}, "files": []}"#;
        assert!(version_fra_json(uden_zip).is_err());
    }
}
