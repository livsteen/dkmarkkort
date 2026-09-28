//! Vektortiles fra MBTiles-filen.
//!
//! Browseren beder om `/tiles/{z}/{x}/{y}` og får markerne i den tile, som
//! tippecanoe har bygget og gzippet dem. Filen er en SQLite-database, så et
//! opslag er én primærnøgle; serveren pakker hverken ud eller om.

use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{Body, StatusCode, error::not_found, path_param, response::Response, route},
};

use crate::data::Data;

path_param!(z: u8, error = not_found);
path_param!(x: u32, error = not_found);
path_param!(y: u32, error = not_found);

/// Tiles'ene ændrer sig kun når pipelinen kører igen, og det kræver en
/// genstart af serveren. En time er kort nok til at en ny udgave slår
/// igennem samme dag.
const CACHE: &str = "public, max-age=3600";

#[route(GET "/tiles/{z}/{x}/{y}")]
async fn tile(cx: &Cx) -> Result<Response> {
    let z = *path_param::<Z>(cx)?;
    let x = *path_param::<X>(cx)?;
    let y = *path_param::<Y>(cx)?;

    // MBTiles nummererer rækkerne nedefra (TMS), kortet ovenfra (XYZ).
    let Some(tms_y) = tms_raekke(z, x, y) else {
        return Err(not_found().into());
    };

    let data: &Data = app_context(cx);
    let tile: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT tile_data FROM tiles \
         WHERE zoom_level = ? AND tile_column = ? AND tile_row = ?",
    )
    .bind(z)
    .bind(x)
    .bind(tms_y)
    .fetch_optional(&data.tiles)
    .await?;

    // En tile uden marker findes ikke i filen. Det er ikke en fejl, bare
    // hav eller by, og svaret er tomt frem for 404, så browseren ikke logger
    // det som en fejl.
    let Some(tile) = tile else {
        return Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header("Cache-Control", CACHE)
            .body(Body::empty())?);
    };

    let mut svar = Response::builder()
        .header("Content-Type", "application/vnd.mapbox-vector-tile")
        .header("Cache-Control", CACHE);
    if er_gzip(&tile) {
        svar = svar.header("Content-Encoding", "gzip");
    }
    Ok(svar.body(Body::from(tile))?)
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
