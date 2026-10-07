//! Tiles fra MBTiles-filerne: markerne som vektortiles og oversigten som
//! PNG'er.
//!
//! Browseren beder om `/tiles/{z}/{x}/{y}` og får markerne i den tile, som
//! tippecanoe har bygget og gzippet dem. Oversigten, som GDAL har tegnet,
//! ligger på `/overblik/{z}/{x}/{y}`, og de sprøjtede marker for en
//! planperiode på `/sproejtning/{aar}/{z}/{x}/{y}`. Filerne er SQLite-databaser, så et
//! opslag er én primærnøgle; serveren pakker hverken ud eller om.

use sqlx::SqlitePool;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, StatusCode, error::not_found, path_param, response::Response, route},
};

use crate::data::kortdata;

path_param!(aar: u16, error = not_found);
path_param!(z: u8, error = not_found);
path_param!(x: u32, error = not_found);
path_param!(y: u32, error = not_found);

/// Tiles'ene ændrer sig kun når pipelinen kører igen. En time er kort nok
/// til at en ny udgave slår igennem samme dag.
const CACHE: &str = "public, max-age=3600";

#[route(GET "/tiles/{z}/{x}/{y}")]
async fn tile(cx: &Cx) -> Result<Response> {
    let data = kortdata(cx)?;
    fra_mbtiles(cx, &data.tiles, "application/vnd.mapbox-vector-tile").await
}

#[route(GET "/overblik/{z}/{x}/{y}")]
async fn overblik(cx: &Cx) -> Result<Response> {
    let data = kortdata(cx)?;
    fra_mbtiles(cx, &data.overblik, "image/png").await
}

/// De sprøjtede marker i planperioden der begynder i `aar`.
#[route(GET "/sproejtning/{aar}/{z}/{x}/{y}")]
async fn sproejtning(cx: &Cx) -> Result<Response> {
    let data = kortdata(cx)?;
    let aar = *path_param::<Aar>(cx)?;
    let Some(tiles) = data.sproejtning.as_ref().and_then(|s| s.tiles.get(&aar)) else {
        return Err(not_found().into());
    };
    fra_mbtiles(cx, tiles, "application/vnd.mapbox-vector-tile").await
}

/// Tilen på stien `{z}/{x}/{y}` fra MBTiles-filen i `tiles`.
async fn fra_mbtiles(cx: &Cx, tiles: &SqlitePool, indholdstype: &'static str) -> Result<Response> {
    let z = *path_param::<Z>(cx)?;
    let x = *path_param::<X>(cx)?;
    let y = *path_param::<Y>(cx)?;

    // MBTiles nummererer rækkerne nedefra (TMS), kortet ovenfra (XYZ).
    let Some(tms_y) = tms_raekke(z, x, y) else {
        return Err(not_found().into());
    };

    let indhold: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT tile_data FROM tiles \
         WHERE zoom_level = ? AND tile_column = ? AND tile_row = ?",
    )
    .bind(z)
    .bind(x)
    .bind(tms_y)
    .fetch_optional(tiles)
    .await?;

    // En tile uden marker findes ikke i filen. Det er ikke en fejl, bare
    // hav eller by, og svaret er tomt frem for 404, så browseren ikke logger
    // det som en fejl.
    let Some(indhold) = indhold else {
        return Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header("Cache-Control", CACHE)
            .body(Body::empty())?);
    };

    let mut svar = Response::builder()
        .header("Content-Type", indholdstype)
        .header("Cache-Control", CACHE);
    if er_gzip(&indhold) {
        svar = svar.header("Content-Encoding", "gzip");
    }
    Ok(svar.body(Body::from(indhold))?)
}

/// Rækken i MBTiles for en XYZ-tile, eller `None` hvis tilen ligger uden for
/// verden på det zoomniveau.
fn tms_raekke(z: u8, x: u32, y: u32) -> Option<u32> {
    if z > 30 {
        return None;
    }
    let antal = 1u32 << z;
    (x < antal && y < antal).then(|| antal - 1 - y)
}

/// tippecanoe gzipper tiles som standard, men flaget kan slås fra. Kig på
/// indholdet frem for at antage.
fn er_gzip(data: &[u8]) -> bool {
    data.starts_with(&[0x1f, 0x8b])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tms_vender_raekkerne() {
        assert_eq!(tms_raekke(0, 0, 0), Some(0));
        assert_eq!(tms_raekke(1, 0, 0), Some(1));
        assert_eq!(tms_raekke(1, 0, 1), Some(0));
        assert_eq!(tms_raekke(14, 8700, 5000), Some(16383 - 5000));
    }

    #[test]
    fn tiles_uden_for_verden_findes_ikke() {
        assert_eq!(tms_raekke(1, 2, 0), None);
        assert_eq!(tms_raekke(1, 0, 2), None);
        assert_eq!(tms_raekke(31, 0, 0), None);
    }

    #[test]
    fn gzip_genkendes_paa_magiske_bytes() {
        assert!(er_gzip(&[0x1f, 0x8b, 0x08]));
        assert!(!er_gzip(&[0x1a, 0x00]));
        assert!(!er_gzip(&[]));
    }
}
