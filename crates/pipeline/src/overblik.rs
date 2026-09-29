//! Oversigten: markerne som ét billede til kortet zoomet ud.
//!
//! Zoomet ud er markerne for mange til at en vektortile kan rumme dem alle,
//! og for mange til at browseren kan tegne dem hver for sig. De brændes i
//! stedet ind i et raster, hvor hver pixel er en kode for den marks gruppe og
//! landsdel (se [`OVERBLIK_FIL`]). Kortet farver pixels efter koden og kan
//! derfor filtrere oversigten ligesom markerne.
//!
//! Rasteret har pixelstørrelsen fra kortets tiles ved det højeste zoom i
//! oversigten. Hvert lavere zoomniveau får i hver pixel den kode der fylder
//! mest i den. Pixels uden mark tæller ikke med, ellers ville et landskab med
//! spredte marker stå hullet, når man zoomer ud.

use std::{
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use dkmarkkort_core::{OVERBLIK_FIL, gruppe::Gruppe};
use sqlx::{
    Connection, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tokio::fs;

use crate::{
    slet, utf8,
    vaerktoej::{gdal_rasterize, gdal_translate, gdalwarp, ogr2ogr},
};

/// Zoomniveauerne i oversigten, i kortets 256-pixel-tiles. Vektortiles'ene
/// er 512 pixel og begynder ved zoom 10, som kortet viser ved sit zoom 11;
/// dér tager markerne over.
const ZOOM: RangeInclusive<u8> = 5..=11;

/// Webmercators bredde i meter (EPSG:3857).
const VERDENS_BREDDE: f64 = 2.0 * 20_037_508.342_789_244;

/// Bygger oversigten fra markerne i `database` og lægger den i `work`.
pub async fn byg(database: &Path, work: &Path) -> Result<PathBuf> {
    tjek_landsdele(database).await?;

    let marker = work.join("overblik.gpkg");
    slet(&marker).await?;
    let sql = kode_sql();
    ogr2ogr([
        "-f",
        "GPKG",
        utf8(&marker),
        utf8(database),
        "-nln",
        "overblik",
        "-t_srs",
        "EPSG:3857",
        "-dialect",
        "SQLITE",
        "-sql",
        sql.as_str(),
    ])
    .await?;

    // Kortets tilegitter begynder i -20037508,34 m, som er et helt antal
    // pixels på alle zoomniveauer. `-tap` lægger rasterets kanter på hele
    // pixels, så pixels og tiles flugter, og intet skal flyttes bagefter.
    let grund = work.join("overblik.tif");
    slet(&grund).await?;
    let stoerrelse = pixelstoerrelse(*ZOOM.end()).to_string();
    gdal_rasterize([
        "-q",
        "-l",
        "overblik",
        "-a",
        "kode",
        "-tr",
        stoerrelse.as_str(),
        stoerrelse.as_str(),
        "-tap",
        "-ot",
        "Byte",
        "-a_nodata",
        "0",
        "-init",
        "0",
        "-co",
        "COMPRESS=DEFLATE",
        "-co",
        "TILED=YES",
        utf8(&marker),
        utf8(&grund),
    ])
    .await?;

    // GDAL skriver ét zoomniveau pr. MBTiles-fil. Dens egne oversigtsniveauer
    // tæller pixels uden mark med, så hvert niveau bygges for sig fra
    // grundrasteret og samles bagefter.
    let mut niveauer = Vec::new();
    for zoom in ZOOM {
        let raster = work.join(format!("overblik-{zoom}.tif"));
        let tiles = work.join(format!("overblik-{zoom}.mbtiles"));
        slet(&tiles).await?;
        let stoerrelse = pixelstoerrelse(zoom).to_string();
        gdalwarp([
            "-q",
            "-overwrite",
            "-r",
            "mode",
            "-tr",
            stoerrelse.as_str(),
            stoerrelse.as_str(),
            "-tap",
            utf8(&grund),
            utf8(&raster),
        ])
        .await?;
        // En pixel er en kode og ikke en farve, så intet må blandes.
        gdal_translate([
            "-q",
            "-of",
            "MBTILES",
            "-co",
            "TILE_FORMAT=PNG",
            "-co",
            "RESAMPLING=NEAREST",
            "-co",
            "NAME=dkmarkkort-overblik",
            "-co",
            "DESCRIPTION=dkmarkkort-overblik",
            utf8(&raster),
            utf8(&tiles),
        ])
        .await?;
        niveauer.push(tiles);
    }

    let overblik = work.join(OVERBLIK_FIL);
    slet(&overblik).await?;
    saml(&niveauer, &overblik).await?;
    Ok(overblik)
}

/// Landsdelens fid er dens nummer i koden, og en pixel har kun plads til
/// 1–15.
async fn tjek_landsdele(database: &Path) -> Result<()> {
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(database)
            .read_only(true),
    )
    .await?;
    let (mindst, stoerst): (i64, i64) = sqlx::query_as("SELECT MIN(fid), MAX(fid) FROM landsdele")
        .fetch_one(&mut db)
        .await?;
    db.close().await?;
    if mindst < 1 || stoerst > 15 {
        bail!("landsdelene har numrene {mindst}–{stoerst}, men oversigten har kun plads til 1–15");
    }
    Ok(())
}

/// Markerne med deres kode: gruppens nummer gange 16 plus landsdelens fid.
fn kode_sql() -> String {
    let grupper: String = Gruppe::ALLE
        .iter()
        .map(|g| format!(" WHEN '{}' THEN {}", g.noegle(), g.nr()))
        .collect();
    format!(
        "SELECT m.geom, (CASE m.gruppe{grupper} END) * 16 + l.fid AS kode
         FROM marker m JOIN landsdele l ON l.nuts3 = m.nuts3"
    )
}

/// Pixelstørrelsen i meter for kortets 256-pixel-tiles på et zoomniveau.
fn pixelstoerrelse(zoom: u8) -> f64 {
    VERDENS_BREDDE / (256.0 * f64::from(1u32 << zoom))
}

/// Lægger zoomniveauerne sammen i én MBTiles-fil. Tabellerne har samme
/// skema, fordi GDAL har skrevet dem alle.
async fn saml(niveauer: &[PathBuf], overblik: &Path) -> Result<()> {
    let (foerste, resten) = niveauer
        .split_first()
        .context("oversigten har ingen zoomniveauer")?;
    fs::rename(foerste, overblik).await?;

    // Serveren åbner filen som uforanderlig, så den skrives uden WAL.
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(overblik)
            .journal_mode(SqliteJournalMode::Delete),
    )
    .await?;
    for niveau in resten {
        sqlx::query("ATTACH DATABASE ? AS niveau")
            .bind(utf8(niveau))
            .execute(&mut db)
            .await?;
        sqlx::query(
            "INSERT INTO tiles (zoom_level, tile_column, tile_row, tile_data)
             SELECT zoom_level, tile_column, tile_row, tile_data FROM niveau.tiles",
        )
        .execute(&mut db)
        .await?;
        sqlx::query("DETACH DATABASE niveau")
            .execute(&mut db)
            .await?;
    }
    for (navn, zoom) in [("minzoom", ZOOM.start()), ("maxzoom", ZOOM.end())] {
        sqlx::query("UPDATE metadata SET value = ? WHERE name = ?")
            .bind(zoom.to_string())
            .bind(navn)
            .execute(&mut db)
            .await?;
    }
    db.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixelstoerrelse_foelger_kortets_tiles() {
        assert!((pixelstoerrelse(0) - 156_543.033_928_041).abs() < 1e-6);
        assert!((pixelstoerrelse(11) - 76.437_028_285_176).abs() < 1e-9);
    }

    #[test]
    fn tilegitterets_begyndelse_er_hele_pixels() {
        for zoom in ZOOM {
            let pixels = VERDENS_BREDDE / 2.0 / pixelstoerrelse(zoom);
            assert!((pixels - pixels.round()).abs() < 1e-6, "zoom {zoom}");
        }
    }

    #[test]
    fn koden_kender_alle_grupper() {
        let sql = kode_sql();
        for gruppe in Gruppe::ALLE {
            assert!(
                sql.contains(&format!("WHEN '{}' THEN {}", gruppe.noegle(), gruppe.nr())),
                "{sql}"
            );
        }
    }
}
