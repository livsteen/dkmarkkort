//! Bygger kortets data fra Landbrugsstyrelsens markkort.
//!
//! ```text
//! cargo run -p dkmarkkort-pipeline -- [--aar 2026] [--data data]
//! ```
//!
//! Resultatet er tre SQLite-filer i datamappen:
//!
//! - `markkort.gpkg`: markerne med afgrødekode, afsnit, afgrødegruppe,
//!   landsdel og udstrækning; landsdelene med deres udstrækning; afgrødekodelisten og
//!   hvornår markdata er hentet.
//! - `marker.mbtiles`: markerne som vektortiles med id, gruppe og landsdel.
//! - `overblik.mbtiles`: markerne som rastertiles til kortet zoomet ud, se
//!   `overblik.rs`.
//!
//! Input ud over det der hentes: `data/dagi-landsdele.geojson` og
//! `data/afgroedekoder-<år>.csv` (se `--bin afgroedekoder`). Kræver GDAL
//! (ogr2ogr, ogrinfo, gdal_rasterize, gdalwarp, gdal_translate) og tippecanoe.
//!
//! Alt mellemliggende ligger i `<data>/work`, og det hentede i `<data>/raw`.
//! Resultatfilerne bygges i `work` og flyttes først når alle er færdige, så
//! en kørende server aldrig åbner en halv fil. Til sidst skrives `bygget`
//! med tidspunktet, som tegn på at alle tre er på plads.

mod afgroedekoder;
mod hent;
mod overblik;
mod udpak;
mod vaerktoej;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use dkmarkkort_core::{
    BYGGET_FIL, DATABASE_FIL, OVERBLIK_FIL, TILES_FIL, TILES_LAG, gruppe::Gruppe, kilder,
};
use jiff::Timestamp;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use tokio::fs;

use crate::{
    hent::{Hentet, hent},
    udpak::udpak,
    vaerktoej::{ogr2ogr, ogrinfo_sql, ogrinfo_sqlite, tippecanoe},
};

struct Indstillinger {
    aar: u16,
    data: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let indstillinger = laes_argumenter()?;
    vaerktoej::tjek_installeret().await?;
    koer(&indstillinger).await
}

fn laes_argumenter() -> Result<Indstillinger> {
    let mut aar = 2026;
    let mut data = PathBuf::from("data");
    let mut argumenter = std::env::args().skip(1);
    while let Some(argument) = argumenter.next() {
        match argument.as_str() {
            "--aar" => {
                aar = argumenter
                    .next()
                    .context("--aar skal have et år")?
                    .parse()
                    .context("--aar skal være et årstal")?;
            }
            "--data" => {
                data = PathBuf::from(argumenter.next().context("--data skal have en mappe")?);
            }
            "-h" | "--help" => {
                println!("brug: dkmarkkort-pipeline [--aar 2026] [--data data]");
                std::process::exit(0);
            }
            andet => bail!("ukendt argument: {andet}"),
        }
    }
    Ok(Indstillinger { aar, data })
}

async fn koer(indstillinger: &Indstillinger) -> Result<()> {
    let aar = indstillinger.aar;
    let data = indstillinger.data.as_path();
    let raw = data.join("raw");
    let work = data.join("work");
    fs::create_dir_all(&raw).await?;
    fs::create_dir_all(&work).await?;

    let landsdele_kilde = data.join("dagi-landsdele.geojson");
    if !fs::try_exists(&landsdele_kilde).await? {
        bail!("mangler {}", landsdele_kilde.display());
    }

    println!("==> Afgrødekoder for {aar}");
    let koder = afgroedekoder::laes(&data.join(format!("afgroedekoder-{aar}.csv")))?;
    let koder_csv = work.join("afgroedekode.csv");
    afgroedekoder::skriv_opslag(&koder, &koder_csv)?;
    println!("    {} koder", koder.len());

    println!("==> Markdata for {aar}");
    let zip = raw.join(format!("Marker_{aar}.zip"));
    let hentet = hent(
        &format!("https://landbrugsgeodata.fvm.dk/Download/Marker/Marker_{aar}.zip"),
        &zip,
    )
    .await?;
    let udpakket = work.join(format!("marker-{aar}"));
    udpak(&zip, &udpakket).await?;
    let shapefil = find_shapefil(&udpakket).await?;

    // Alt råt samles i én arbejdsdatabase, så SQL'en nedenfor kan se både
    // marker, landsdele og koder på én gang.
    println!("==> Indlæser i arbejdsdatabasen");
    let arbejd = work.join("arbejd.gpkg");
    slet(&arbejd).await?;
    // Datasættets .cpg angiver tegnsættet på en måde GDAL ikke genkender;
    // filen er ISO-8859-1. En shapefile skelner ikke mellem Polygon og
    // MultiPolygon, det gør en GeoPackage.
    ogr2ogr([
        "--config",
        "SHAPE_ENCODING",
        "ISO-8859-1",
        "-f",
        "GPKG",
        utf8(&arbejd),
        utf8(&shapefil),
        "-nln",
        "raa_marker",
        "-nlt",
        "PROMOTE_TO_MULTI",
        "-select",
        "Marknr,CVR,Afgkode,Afgroede,IMK_areal",
    ])
    .await?;
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&arbejd),
        utf8(&landsdele_kilde),
        "-nln",
        "landsdele",
        "-select",
        "navn,nuts3",
    ])
    .await?;
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&arbejd),
        utf8(&koder_csv),
        "-nln",
        "afgroedekode",
        "-oo",
        "AUTODETECT_TYPE=YES",
    ])
    .await?;

    // Hver mark får den landsdel et punkt på dens flade ligger i. Et punkt
    // på fladen (ST_PointOnSurface) ligger altid inde i marken, også når
    // marken er L-formet eller har huller, så en mark aldrig tildeles en
    // landsdel den ikke rører.
    println!("==> Afgrødegruppe og landsdel pr. mark");
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&arbejd),
        utf8(&arbejd),
        "-nln",
        "marker",
        "-nlt",
        "MULTIPOLYGON",
        // Geometrikolonnen i et SQL-resultat navngives forskelligt fra
        // GDAL-version til GDAL-version, og resten af pipelinen slår op i
        // geom.
        "-lco",
        "GEOMETRY_NAME=geom",
        "-dialect",
        "SQLITE",
        "-sql",
        "SELECT m.geom,
                m.Marknr, m.CVR,
                CAST(m.Afgkode AS INTEGER) AS Afgkode,
                m.Afgroede, m.IMK_areal,
                k.afsnit,
                COALESCE(k.gruppe, 'ukendt') AS gruppe,
                (SELECT l.nuts3 FROM landsdele l
                  WHERE ST_Intersects(l.geom, ST_PointOnSurface(m.geom))) AS nuts3
         FROM raa_marker m
         LEFT JOIN afgroedekode k ON k.afgroedekode = CAST(m.Afgkode AS INTEGER)",
    ])
    .await?;

    // Landsdelsgrænserne er forenklede, så marker helt ude ved kysten kan
    // falde uden for dem. De får den landsdel der ligger nærmest. Den
    // nærmeste findes med MIN frem for ORDER BY, fordi SQLite 3.46, som
    // Debian trixie i pipeline-containeren har, ikke kan se den ydre tabel i
    // et underudtryks ORDER BY.
    ogrinfo_sqlite(
        &arbejd,
        "UPDATE marker
         SET nuts3 = (SELECT l.nuts3 FROM landsdele l
                      WHERE ST_Distance(l.geom, ST_PointOnSurface(marker.geom)) =
                            (SELECT MIN(ST_Distance(n.geom, ST_PointOnSurface(marker.geom)))
                             FROM landsdele n)
                      LIMIT 1)
         WHERE nuts3 IS NULL",
    )
    .await?;

    println!("==> Bygger {DATABASE_FIL}");
    let database = work.join(DATABASE_FIL);
    slet(&database).await?;
    ogr2ogr([
        "-f",
        "GPKG",
        utf8(&database),
        utf8(&arbejd),
        "marker",
        "-nln",
        "marker",
    ])
    .await?;
    // Serveren zoomer til en mark den har fundet i en søgning, og den mark er
    // måske ikke tegnet endnu. Udstrækningen lægges derfor i kolonner i
    // EPSG:4326 ligesom landsdelenes; kortet regner selv videre derfra.
    for kolonne in ["vest", "syd", "oest", "nord"] {
        ogrinfo_sql(
            &database,
            &format!("ALTER TABLE marker ADD COLUMN {kolonne} REAL"),
        )
        .await?;
    }
    ogrinfo_sql(
        &database,
        "UPDATE marker SET vest = ST_MinX(u), syd = ST_MinY(u),
                           oest = ST_MaxX(u), nord = ST_MaxY(u)
         FROM (SELECT fid AS f, ST_Transform(ST_Envelope(geom), 4326) AS u FROM marker)
         WHERE fid = f",
    )
    .await?;
    // En bedrifts marker slås op på CVR-nummeret.
    ogrinfo_sql(&database, "CREATE INDEX marker_cvr ON marker (CVR)").await?;
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&database),
        utf8(&arbejd),
        "landsdele",
        "-nln",
        "landsdele",
        "-t_srs",
        "EPSG:4326",
    ])
    .await?;
    // Serveren skal kunne zoome til en landsdel uden selv at regne på
    // geometri, så udstrækningen lægges i kolonner.
    for kolonne in ["vest", "syd", "oest", "nord"] {
        ogrinfo_sql(
            &database,
            &format!("ALTER TABLE landsdele ADD COLUMN {kolonne} REAL"),
        )
        .await?;
    }
    ogrinfo_sql(
        &database,
        "UPDATE landsdele SET vest = ST_MinX(geom), syd = ST_MinY(geom),
                              oest = ST_MaxX(geom), nord = ST_MaxY(geom)",
    )
    .await?;
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&database),
        utf8(&arbejd),
        "afgroedekode",
        "-nln",
        "afgroedekode",
    ])
    .await?;
    let datakilde_csv = work.join("datakilde.csv");
    skriv_datakilde(&datakilde_csv, &hentet, aar)?;
    ogr2ogr([
        "-update",
        "-f",
        "GPKG",
        utf8(&database),
        utf8(&datakilde_csv),
        "-nln",
        "datakilde",
    ])
    .await?;

    println!("==> Bygger {TILES_FIL}");
    let geojsonl = work.join("marker.geojsonl");
    slet(&geojsonl).await?;
    // Tiles'ene bærer kun det kortet tegner og filtrerer på. Alt andet om en
    // mark slås op i databasen på dens id.
    ogr2ogr([
        "-f",
        "GeoJSONSeq",
        utf8(&geojsonl),
        utf8(&database),
        "-sql",
        "SELECT fid, gruppe, nuts3, geom FROM marker",
        "-t_srs",
        "EPSG:4326",
        // Id'et skal stå som objektets eget id, ikke som en egenskab: GDAL
        // skriver ikke en kolonne der hedder id, og kortet finder en mark
        // på tilens id. tippecanoe tager id'et med af sig selv.
        "-preserve_fid",
    ])
    .await?;
    let tiles = work.join(TILES_FIL);
    let kreditering = format!(
        "{}; {}; {}",
        kilder::MARKER.kreditering,
        kilder::AFGROEDEKODER.kreditering,
        kilder::LANDSDELE.kreditering
    );
    // Fra zoom 10 er der plads til alle marker i hver tile, så ingen udelades
    // og kortet ser ens ud overalt. Længere ude viser kortet oversigten. Ved
    // 14 er den mindste mark tydelig, og længere inde forstørrer kortet selv.
    // Nabomarker deler kant, og forenklingen holder den kant fælles.
    tippecanoe([
        "-o",
        utf8(&tiles),
        "-l",
        TILES_LAG,
        "-n",
        "dkmarkkort",
        "-A",
        kreditering.as_str(),
        "-Z10",
        "-z14",
        "--detect-shared-borders",
        "--force",
        utf8(&geojsonl),
    ])
    .await?;

    println!("==> Bygger {OVERBLIK_FIL}");
    let overblik = overblik::byg(&database, &work).await?;

    fs::rename(&database, data.join(DATABASE_FIL)).await?;
    fs::rename(&tiles, data.join(TILES_FIL)).await?;
    fs::rename(&overblik, data.join(OVERBLIK_FIL)).await?;
    fs::write(data.join(BYGGET_FIL), Timestamp::now().to_string()).await?;

    opsummer(&data.join(DATABASE_FIL)).await?;
    Ok(())
}

fn skriv_datakilde(sti: &Path, hentet: &Hentet, aar: u16) -> Result<()> {
    let mut skriver = csv::Writer::from_path(sti)?;
    skriver.write_record(["id", "aar", "url", "hentet", "sidst_aendret"])?;
    skriver.write_record([
        kilder::MARKER.id,
        &aar.to_string(),
        &hentet.url,
        &hentet.hentet,
        hentet.sidst_aendret.as_deref().unwrap_or(""),
    ])?;
    skriver.flush()?;
    Ok(())
}

/// Shapefilen har ikke altid ligget samme sted i zip'en.
async fn find_shapefil(mappe: &Path) -> Result<PathBuf> {
    let mut koe = vec![mappe.to_owned()];
    while let Some(mappe) = koe.pop() {
        let mut indhold = fs::read_dir(&mappe).await?;
        while let Some(post) = indhold.next_entry().await? {
            let sti = post.path();
            if post.file_type().await?.is_dir() {
                koe.push(sti);
            } else if sti
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("shp"))
            {
                return Ok(sti);
            }
        }
    }
    bail!("ingen shapefil i {}", mappe.display())
}

/// Tal fra den færdige database, så to kørsler kan sammenlignes.
async fn opsummer(database: &Path) -> Result<()> {
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(database)
            .read_only(true),
    )
    .await?;

    let i_alt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM marker")
        .fetch_one(&mut db)
        .await?;
    println!("==> {i_alt} marker");

    let pr_gruppe: BTreeMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT gruppe, COUNT(*) FROM marker GROUP BY gruppe")
            .fetch_all(&mut db)
            .await?
            .into_iter()
            .collect();
    for gruppe in Gruppe::ALLE {
        let antal = pr_gruppe.get(gruppe.noegle()).copied().unwrap_or(0);
        println!("    {antal:>7}  {}", gruppe.navn());
    }

    let ukendte: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT Afgkode, Afgroede, COUNT(*) FROM marker
         WHERE gruppe = 'ukendt' GROUP BY Afgkode, Afgroede ORDER BY 3 DESC",
    )
    .fetch_all(&mut db)
    .await?;
    for (kode, afgroede, antal) in ukendte {
        println!("    ukendt kode {kode} \"{afgroede}\": {antal} marker");
    }

    let uden_landsdel: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM marker WHERE nuts3 IS NULL")
        .fetch_one(&mut db)
        .await?;
    if uden_landsdel > 0 {
        bail!("{uden_landsdel} marker står uden landsdel");
    }
    db.close().await?;
    Ok(())
}

async fn slet(sti: &Path) -> Result<()> {
    match fs::remove_file(sti).await {
        Err(fejl) if fejl.kind() != std::io::ErrorKind::NotFound => {
            Err(fejl).with_context(|| format!("kunne ikke slette {}", sti.display()))
        }
        _ => Ok(()),
    }
}

fn utf8(sti: &Path) -> &str {
    sti.to_str().expect("stierne er UTF-8")
}
