//! Opslag som kortet laver mens man bruger det: søgning på CVR-nummer, en
//! bedrifts marker og oplysningerne om én mark.
//!
//! Tiles'ene bærer kun det kortet tegner, og de kender kun de marker der er
//! hentet. Alt andet om en mark står i databasen og slås op her, på markens
//! id (samme id som i tiles'ene) eller på bedriftens CVR-nummer. Svarene er
//! JSON, som `markkort.js` sætter ind i sidens skabeloner.

use dkmarkkort_core::gruppe::Gruppe;
use serde::Serialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::not_found, path_param, query_params, route},
};

use crate::data::{Bedrift, Data, kortdata};

path_param!(id: i64, error = not_found);
path_param!(cvr: String, error = not_found);

#[query_params(error = bad_request)]
struct Soegning {
    q: String,
}

/// Loft over forslagene. 26.000 bedrifter kan ikke overskues i en liste, og
/// hvert ciffer mere skærer ned til en tiendedel.
const MAKS_FORSLAG: usize = 20;

/// Kolonnerne fra `marker` i den rækkefølge [`mark`] tager dem.
type MarkRaekke = (
    i64,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<f64>,
    Option<String>,
    String,
    Option<String>,
    f64,
    f64,
    f64,
    f64,
);

#[derive(Serialize)]
struct Forslag {
    cvr: String,
    marker: i64,
}

#[derive(Serialize)]
struct Mark {
    id: i64,
    marknr: String,
    /// `UDEN_CVR` for de marker der er indberettet uden CVR-nummer.
    cvr: String,
    afgroedekode: Option<i64>,
    afgroede: String,
    /// Hektar.
    areal: Option<f64>,
    afsnit: Option<String>,
    gruppe: &'static str,
    gruppe_navn: &'static str,
    landsdel: String,
    /// Vest, syd, øst, nord i grader (EPSG:4326).
    udstraekning: [f64; 4],
}

/// Bedrifter hvis CVR-nummer begynder med de cifre der er skrevet.
#[route(GET "/soeg")]
async fn soeg(cx: &Cx) -> Result<Json<Vec<Forslag>>> {
    let data = kortdata(cx)?;
    let soegning = query_params::<Soegning>(cx)?;
    let forslag = bedrifter_med_praefiks(&data.bedrifter, &soegning.q)
        .iter()
        .take(MAKS_FORSLAG)
        .map(|b| Forslag {
            cvr: b.cvr.clone(),
            marker: b.marker,
        })
        .collect();
    Ok(Json(forslag))
}

/// Alle marker under ét CVR-nummer, ordnet efter marknummer.
#[route(GET "/bedrift/{cvr}")]
async fn bedrift(cx: &Cx) -> Result<Json<Vec<Mark>>> {
    let data = kortdata(cx)?;
    let cvr = path_param::<Cvr>(cx)?;
    if !er_cvr(cvr) {
        return Err(not_found().into());
    }
    let raekker: Vec<MarkRaekke> = sqlx::query_as(
        "SELECT fid, Marknr, CVR, Afgkode, Afgroede, IMK_areal, afsnit, gruppe, nuts3,
                vest, syd, oest, nord
         FROM marker WHERE CVR = ?",
    )
    .bind(cvr.as_str())
    .fetch_all(&data.database)
    .await?;
    if raekker.is_empty() {
        return Err(not_found().into());
    }
    let mut marker: Vec<Mark> = raekker.into_iter().map(|r| mark(&data, r)).collect();
    marker.sort_by(|a, b| sammenlign_marknr(&a.marknr, &b.marknr));
    Ok(Json(marker))
}

/// Én mark, som når der er klikket på den.
#[route(GET "/mark/{id}")]
async fn en_mark(cx: &Cx) -> Result<Json<Mark>> {
    let data = kortdata(cx)?;
    let id = *path_param::<Id>(cx)?;
    let raekke: Option<MarkRaekke> = sqlx::query_as(
        "SELECT fid, Marknr, CVR, Afgkode, Afgroede, IMK_areal, afsnit, gruppe, nuts3,
                vest, syd, oest, nord
         FROM marker WHERE fid = ?",
    )
    .bind(id)
    .fetch_optional(&data.database)
    .await?;
    match raekke {
        Some(raekke) => Ok(Json(mark(&data, raekke))),
        None => Err(not_found().into()),
    }
}

fn mark(
    data: &Data,
    (
        id,
        marknr,
        cvr,
        afgroedekode,
        afgroede,
        areal,
        afsnit,
        gruppe,
        nuts3,
        vest,
        syd,
        oest,
        nord,
    ): MarkRaekke,
) -> Mark {
    let gruppe = Gruppe::fra_noegle(&gruppe).unwrap_or(Gruppe::UkendtKode);
    let landsdel = nuts3
        .and_then(|nuts3| data.landsdele.iter().find(|l| l.nuts3 == nuts3))
        .map(|l| l.navn.clone())
        .unwrap_or_default();
    Mark {
        id,
        marknr: marknr.unwrap_or_default(),
        cvr: cvr.unwrap_or_default(),
        afgroedekode,
        afgroede: afgroede.unwrap_or_default(),
        areal,
        afsnit,
        gruppe: gruppe.noegle(),
        gruppe_navn: gruppe.navn(),
        landsdel,
        udstraekning: [vest, syd, oest, nord],
    }
}

/// Udsnittet af de sorterede bedrifter hvis CVR-nummer begynder med
/// `praefiks`. Mellemrum tæller ikke med, så "1234 5678" også findes; alt
/// andet end cifre giver ingen forslag.
fn bedrifter_med_praefiks<'a>(bedrifter: &'a [Bedrift], praefiks: &str) -> &'a [Bedrift] {
    let praefiks: String = praefiks.chars().filter(|c| !c.is_whitespace()).collect();
    if praefiks.is_empty() || !praefiks.chars().all(|c| c.is_ascii_digit()) {
        return &[];
    }
    let start = bedrifter.partition_point(|b| b.cvr.as_str() < praefiks.as_str());
    let laengde = bedrifter[start..]
        .iter()
        .take_while(|b| b.cvr.starts_with(&praefiks))
        .count();
    &bedrifter[start..start + laengde]
}

fn er_cvr(tekst: &str) -> bool {
    tekst.len() == 8 && tekst.bytes().all(|b| b.is_ascii_digit())
}

/// Marknumre er landmandens egne ("1-0", "12-3", "2a"), så de sorteres med
/// tallene som tal: "2-0" før "10-0".
fn sammenlign_marknr(a: &str, b: &str) -> std::cmp::Ordering {
    fn dele(tekst: &str) -> Vec<(u64, &str)> {
        let mut dele = Vec::new();
        let mut rest = tekst;
        while !rest.is_empty() {
            let cifre = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            let (tal, efter) = rest.split_at(cifre);
            let bogstaver = efter.len()
                - efter
                    .trim_start_matches(|c: char| !c.is_ascii_digit())
                    .len();
            let (tekst, efter) = efter.split_at(bogstaver);
            dele.push((tal.parse().unwrap_or(0), tekst));
            rest = efter;
        }
        dele
    }
    dele(a).cmp(&dele(b)).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bedrifter(cvr: &[&str]) -> Vec<Bedrift> {
        cvr.iter()
            .map(|cvr| Bedrift {
                cvr: cvr.to_string(),
                marker: 1,
            })
            .collect()
    }

    fn cvr(bedrifter: &[Bedrift]) -> Vec<&str> {
        bedrifter.iter().map(|b| b.cvr.as_str()).collect()
    }

    #[test]
    fn praefiks_finder_de_bedrifter_der_begynder_saadan() {
        let alle = bedrifter(&["10000000", "12340000", "12345678", "12350000", "99999999"]);
        assert_eq!(
            cvr(bedrifter_med_praefiks(&alle, "1234")),
            ["12340000", "12345678"]
        );
        assert_eq!(cvr(bedrifter_med_praefiks(&alle, "1234 5")), ["12345678"]);
        assert_eq!(cvr(bedrifter_med_praefiks(&alle, "99999999")), ["99999999"]);
        assert!(bedrifter_med_praefiks(&alle, "5").is_empty());
    }

    #[test]
    fn praefiks_uden_cifre_giver_ingen_forslag() {
        let alle = bedrifter(&["12345678"]);
        assert!(bedrifter_med_praefiks(&alle, "").is_empty());
        assert!(bedrifter_med_praefiks(&alle, "  ").is_empty());
        assert!(bedrifter_med_praefiks(&alle, "12a").is_empty());
    }

    #[test]
    fn cvr_er_otte_cifre() {
        assert!(er_cvr("12345678"));
        assert!(!er_cvr("1234567"));
        assert!(!er_cvr("1234567a"));
        assert!(!er_cvr("123456789"));
    }

    #[test]
    fn marknumre_sorteres_som_tal() {
        let mut numre = ["10-0", "2-0", "1-1", "1-0", "2a", "2"];
        numre.sort_by(|a, b| sammenlign_marknr(a, b));
        assert_eq!(numre, ["1-0", "1-1", "2", "2-0", "2a", "10-0"]);
    }
}
