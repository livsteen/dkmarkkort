//! Hvad der er sprøjtet på et sted på kortet, planperiode for planperiode.
//!
//! Markerne i sprøjtedata er dem der var det år, og ikke årets marker. Et
//! klik slås derfor op på stedet: for hver planperiode findes den mark der
//! dengang lå hvor der er klikket, med den afgrøde der voksede der, og hvad
//! der blev brugt på den.

use dkmarkkort_core::{ENHED_KG, ENHED_LITER, planperiode};
use serde::Serialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::not_found, query_params, route},
};

use crate::{
    data::{Data, Sproejtning, kortdata},
    geometri,
};

#[query_params(error = bad_request)]
struct Sted {
    /// Længde og bredde i grader (EPSG:4326).
    lon: f64,
    lat: f64,
}

#[derive(Serialize)]
struct Periode {
    aar: u16,
    planperiode: String,
    afgroedekode: Option<i64>,
    /// Navnet fra årets kodeliste; koderne er stort set de samme fra år til
    /// år.
    afgroede: Option<String>,
    /// Hektar.
    areal: f64,
    /// Belastning pr. hektar.
    belastning: Option<f64>,
    pfas: bool,
    midler: Vec<Middel>,
}

#[derive(Serialize)]
struct Middel {
    navn: String,
    regnr: String,
    /// Den samlede mængde i planperioden pr. hektar af marken.
    maengde_pr_ha: f64,
    /// "kg" eller "l", eller ingen for de få midler der er indberettet i
    /// andre enheder.
    enhed: Option<&'static str>,
    /// Middelets del af markens belastning pr. hektar, hvis middelets
    /// belastning er kendt.
    belastning_pr_ha: Option<f64>,
    pfas: Option<bool>,
}

/// Kolonnerne fra `sproejtemarker` i den rækkefølge [`sted`] tager dem.
type MarkRaekke = (i64, u16, Option<i64>, f64, Option<f64>, i64, Vec<u8>);

/// Kolonnerne fra en marks sprøjtninger: middel, registreringsnummer,
/// mængde, enhed, middelets belastning og PFAS.
type MiddelRaekke = (String, String, f64, i64, Option<f64>, Option<bool>);

/// De sprøjtede marker der har ligget på stedet, nyeste planperiode først.
#[route(GET "/sproejtning/sted")]
async fn sted(cx: &Cx) -> Result<Json<Vec<Periode>>> {
    let data = kortdata(cx)?;
    let Some(sproejtning) = &data.sproejtning else {
        return Err(not_found().into());
    };
    let &Sted { lon, lat } = query_params::<Sted>(cx)?;
    if !(lon.is_finite() && lat.is_finite()) {
        return Err(not_found().into());
    }

    // R-træet finder de marker hvis udstrækning rammer punktet; om punktet
    // ligger i selve marken, afgøres bagefter.
    let kandidater: Vec<MarkRaekke> = sqlx::query_as(
        "SELECT m.fid, m.aar, m.afgkode, m.areal_ha, m.belastning, m.pfas, m.geom
         FROM rtree_sproejtemarker_geom r JOIN sproejtemarker m ON m.fid = r.id
         WHERE r.minx <= ?1 AND r.maxx >= ?1 AND r.miny <= ?2 AND r.maxy >= ?2
         ORDER BY m.aar DESC",
    )
    .bind(lon)
    .bind(lat)
    .fetch_all(&sproejtning.database)
    .await?;

    let mut perioder = Vec::new();
    for (fid, aar, afgroedekode, areal, belastning, pfas, geom) in kandidater {
        if geometri::indeholder(&geom, lon, lat) != Some(true) {
            continue;
        }
        perioder.push(Periode {
            aar,
            planperiode: planperiode(aar),
            afgroedekode,
            afgroede: afgroedekode.and_then(|kode| data.afgroeder.get(&kode).cloned()),
            areal,
            belastning,
            pfas: pfas != 0,
            midler: midler(sproejtning, fid, areal).await?,
        });
    }
    Ok(Json(perioder))
}

/// Det der er brugt på en sprøjtet mark, det der belaster mest først.
async fn midler(sproejtning: &Sproejtning, mark: i64, areal: f64) -> Result<Vec<Middel>> {
    let raekker: Vec<MiddelRaekke> = sqlx::query_as(
        "SELECT mi.navn, s.regnr, s.maengde, s.enhed, mi.belastning, mi.pfas
         FROM sproejtninger s JOIN midler mi ON mi.regnr = s.regnr
         WHERE s.mark = ?",
    )
    .bind(mark)
    .fetch_all(&sproejtning.database)
    .await?;
    let mut midler: Vec<Middel> = raekker
        .into_iter()
        .map(|(navn, regnr, maengde, enhed, belastning, pfas)| {
            let enhed = enhed_navn(enhed);
            Middel {
                navn,
                regnr,
                maengde_pr_ha: maengde / areal,
                enhed,
                belastning_pr_ha: enhed.and(belastning).map(|b| maengde * b / areal),
                pfas,
            }
        })
        .collect();
    midler.sort_by(|a, b| {
        b.belastning_pr_ha
            .unwrap_or(0.0)
            .total_cmp(&a.belastning_pr_ha.unwrap_or(0.0))
    });
    Ok(midler)
}

/// Enhedens navn, hvis mængden kan regnes om til belastning.
fn enhed_navn(enhed: i64) -> Option<&'static str> {
    match enhed {
        ENHED_KG => Some("kg"),
        ENHED_LITER => Some("l"),
        _ => None,
    }
}

/// Planperioderne der kan vælges på kortet, nyeste først.
pub fn perioder(data: &Data) -> Vec<(u16, String)> {
    data.sproejtning
        .iter()
        .flat_map(|s| s.tiles.keys().rev())
        .map(|&aar| (aar, planperiode(aar)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kun_kg_og_liter_har_et_navn() {
        assert_eq!(enhed_navn(ENHED_KG), Some("kg"));
        assert_eq!(enhed_navn(ENHED_LITER), Some("l"));
        assert_eq!(enhed_navn(5), None);
    }
}
