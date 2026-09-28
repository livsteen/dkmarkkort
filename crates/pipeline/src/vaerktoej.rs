//! De eksterne værktøjer pipelinen kalder: GDAL (ogr2ogr, ogrinfo) og
//! tippecanoe. Rust styrer rækkefølgen og tjekker hvert trin; den tunge
//! geobehandling overlades til værktøjer der har gjort den i årevis.

use std::{ffi::OsStr, path::Path, process::Stdio};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// Stopper pipelinen med det samme, hvis et værktøj mangler, frem for at
/// fejle efter en download på flere hundrede MB.
pub async fn tjek_installeret() -> Result<()> {
    for (navn, flag) in [
        ("ogr2ogr", "--version"),
        ("ogrinfo", "--version"),
        ("tippecanoe", "--version"),
    ] {
        let status = Command::new(navn)
            .arg(flag)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        if !matches!(status, Ok(s) if s.success()) {
            bail!("{navn} blev ikke fundet. Pipelinen kræver GDAL og tippecanoe i PATH.");
        }
    }
    Ok(())
}

pub async fn ogr2ogr<I, S>(argumenter: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    koer("ogr2ogr", argumenter).await
}

/// Kører én SQL-sætning mod en GeoPackage gennem GDAL, så GeoPackage'ens
/// egne funktioner og triggere (R-træ, featuretælling) er registreret.
pub async fn ogrinfo_sql(database: &Path, sql: &str) -> Result<()> {
    koer(
        "ogrinfo",
        [
            database.as_os_str(),
            OsStr::new("-q"),
            OsStr::new("-sql"),
            OsStr::new(sql),
        ],
    )
    .await
}

/// Som [`ogrinfo_sql`], men i GDAL's SQLite-dialekt med
/// SpatiaLite-funktionerne (`ST_Centroid`, `ST_Distance` …).
pub async fn ogrinfo_sqlite(database: &Path, sql: &str) -> Result<()> {
    koer(
        "ogrinfo",
        [
            database.as_os_str(),
            OsStr::new("-q"),
            OsStr::new("-dialect"),
            OsStr::new("SQLITE"),
            OsStr::new("-sql"),
            OsStr::new(sql),
        ],
    )
    .await
}

pub async fn tippecanoe<I, S>(argumenter: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    koer("tippecanoe", argumenter).await
}

async fn koer<I, S>(program: &str, argumenter: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(program)
        .args(argumenter)
        .status()
        .await
        .with_context(|| format!("kunne ikke starte {program}"))?;
    if !status.success() {
        bail!("{program} fejlede ({status})");
    }
    Ok(())
}
