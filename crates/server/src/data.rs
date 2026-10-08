//! Serverens adgang til databasen, tiles'ene og oversigten. Alle er
//! SQLite-filer som pipelinen har bygget, og alle åbnes skrivebeskyttet.
//!
//! Serveren starter også uden data og holder øje med pipelinens `bygget`-fil.
//! Når den ændrer sig, åbnes den nye udgave, og den gamle lukkes, når den
//! sidste forespørgsel har sluppet den.
//!
//! Det der ikke ændrer sig og bruges ved hvert opslag, læses én gang ved
//! start: landsdelene, optællingerne, listen over bedrifter og
//! afgrødekodernes navne.
//!
//! Når en ny version af serveren deployes, ligger de gamle data der, mens
//! pipelinen bygger nye. Serveren viser dem så godt den kan:
//!
//! - Kernen skal kunne læses: markerne, deres tiles, oversigten,
//!   landsdelene og bedrifterne. Uden den er der intet kort.
//! - Alt andet er tilvalg, der læses med [`tilvalg`]: afgrødernes navne og
//!   sprøjtelaget. Mangler et tilvalg, eller er det bygget af en ældre
//!   pipeline, logges det, og kortet vises uden, indtil pipelinen har bygget
//!   det. Nyt der kommer til, skal læses sådan.
//! - Er data bygget i en anden [`DATAVERSION`], er der en breaking change,
//!   og de gamle data vises ikke. Siden siger så, at kortdata opdateres.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
    time::Duration,
};

use dkmarkkort_core::{
    BYGGET_FIL, DATABASE_FIL, DATAVERSION, DATAVERSION_FIL, FEJLET_FIL, OVERBLIK_FIL,
    SPROEJTNING_DATABASE_FIL, TILES_FIL, gruppe::Gruppe, kilder, sproejtning_tiles_fil,
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::error::service_unavailable,
};

/// Hvor ofte serveren ser efter nye data.
const KIG_EFTER: Duration = Duration::from_secs(30);

/// Hvor længe en browser bedes vente, før den prøver igen uden data.
const PROEV_IGEN: u64 = 60;

/// De data serveren viser lige nu, eller ingen, indtil pipelinen har bygget
/// dem første gang.
#[derive(Clone)]
pub struct Kortdata {
    data: Arc<RwLock<Option<Arc<Data>>>>,
    mappe: Arc<Path>,
}

impl Kortdata {
    /// Åbner data i `mappe`, hvis de findes, og ser derefter efter nye hvert
    /// [`KIG_EFTER`].
    pub async fn hold_opdateret(mappe: PathBuf) -> Self {
        let kortdata = Kortdata {
            data: Arc::default(),
            mappe: Arc::from(mappe),
        };
        let mut opdatering = Opdatering::default();
        opdatering.koer(&kortdata).await;

        let baggrund = kortdata.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(KIG_EFTER).await;
                opdatering.koer(&baggrund).await;
            }
        });
        kortdata
    }

    pub fn hent(&self) -> Option<Arc<Data>> {
        self.data
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Om pipelinens seneste forsøg på at bygge data fejlede.
    pub async fn fejlet(&self) -> bool {
        tokio::fs::try_exists(self.mappe.join(FEJLET_FIL))
            .await
            .unwrap_or(false)
    }

    /// Om der ligger data i en anden version end serverens, så nye er på
    /// vej.
    pub async fn opdateres(&self) -> bool {
        let bygget = tokio::fs::try_exists(self.mappe.join(BYGGET_FIL))
            .await
            .unwrap_or(false);
        bygget && dataversion(&self.mappe).await != Ok(DATAVERSION)
    }

    fn saet(&self, data: Data) {
        *self.data.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(data));
    }
}

/// Versionen af data i `mappe`.
async fn dataversion(mappe: &Path) -> std::result::Result<u32, String> {
    let indhold = tokio::fs::read_to_string(mappe.join(DATAVERSION_FIL))
        .await
        .ok();
    fortolk_dataversion(indhold.as_deref())
}

/// Data uden versionsfil er version 1, fra før pipelinen skrev den.
fn fortolk_dataversion(indhold: Option<&str>) -> std::result::Result<u32, String> {
    match indhold {
        None => Ok(1),
        Some(tekst) => tekst
            .trim()
            .parse()
            .map_err(|_| format!("{DATAVERSION_FIL} indeholder {tekst:?}, ikke et tal")),
    }
}

/// Læser et tilvalg: noget kortet kan vises uden. Mangler det, eller er det
/// bygget af en ældre pipeline, logges hvorfor, og kortet vises uden det,
/// indtil pipelinen har bygget data igen.
async fn tilvalg<T>(
    hvad: &str,
    laes: impl Future<Output = std::result::Result<T, String>>,
) -> Option<T> {
    match laes.await {
        Ok(vaerdi) => Some(vaerdi),
        Err(fejl) => {
            eprintln!(
                "dkmarkkort: {hvad} vises ikke: {fejl}; \
                 det kommer, når pipelinen har bygget data igen"
            );
            None
        }
    }
}

/// Data til en forespørgsel, eller 503, mens de første data bygges.
pub fn kortdata(cx: &Cx) -> Result<Arc<Data>> {
    app_context::<Kortdata>(cx)
        .hent()
        .ok_or_else(|| service_unavailable(PROEV_IGEN).into())
}

/// Hvad serveren sidst så i datamappen.
#[derive(Default)]
struct Opdatering {
    /// Indholdet af `bygget`, da de viste data blev åbnet.
    bygget: Option<String>,
    /// Den sidste fejl, så den kun logges én gang, selvom den gentager sig.
    fejl: Option<String>,
}

impl Opdatering {
    /// Åbner data, hvis der ingen er, eller hvis pipelinen har bygget nye.
    /// Mislykkes det, beholdes de data der vises.
    async fn koer(&mut self, kortdata: &Kortdata) {
        let mappe = &*kortdata.mappe;
        let bygget = tokio::fs::read_to_string(mappe.join(BYGGET_FIL)).await.ok();
        if kortdata.hent().is_some() && bygget == self.bygget {
            return;
        }
        match Data::aabn(mappe).await {
            Ok(data) => {
                println!("dkmarkkort: data åbnet fra {}", mappe.display());
                kortdata.saet(data);
                self.bygget = bygget;
                self.fejl = None;
            }
            Err(fejl) => {
                if self.fejl.as_ref() != Some(&fejl) {
                    eprintln!("dkmarkkort: {fejl}");
                    self.fejl = Some(fejl);
                }
            }
        }
    }
}

pub struct Landsdel {
    /// Landsdelens nummer i oversigtens pixels.
    pub nr: i64,
    pub nuts3: String,
    pub navn: String,
    /// Vest, syd, øst, nord i grader (EPSG:4326).
    pub udstraekning: [f64; 4],
}

/// Hvilken udgave af markdata der vises.
pub struct Udgave {
    pub aar: String,
    pub url: String,
    pub hentet: String,
    pub sidst_aendret: String,
}

/// Hvilken version af sprøjtedata der vises.
pub struct SproejtningUdgave {
    pub url: String,
    pub hentet: String,
}

/// Sprøjtedata: de sprøjtede marker for hver planperiode og deres tiles.
pub struct Sproejtning {
    pub database: SqlitePool,
    /// Tiles'ene for hver planperiode, efter det år perioden begynder.
    pub tiles: BTreeMap<u16, SqlitePool>,
    pub udgave: Option<SproejtningUdgave>,
}

impl Sproejtning {
    /// Åbner sprøjtedata i `mappe`, eller `None` hvis pipelinen ikke har
    /// bygget dem.
    async fn aabn(mappe: &Path) -> std::result::Result<Option<Self>, String> {
        let sti = mappe.join(SPROEJTNING_DATABASE_FIL);
        if !sti.exists() {
            return Ok(None);
        }
        let database = pool(&sti).await?;
        let fejl = |hvad: &'static str| move |e: sqlx::Error| format!("{hvad}: {e}");

        // Et klik slås op i R-træet i længde og bredde, så markerne skal
        // ligge i WGS84.
        let srs: i64 = sqlx::query_scalar(
            "SELECT srs_id FROM gpkg_geometry_columns WHERE table_name = 'sproejtemarker'",
        )
        .fetch_one(&database)
        .await
        .map_err(fejl("sprøjtemarkernes projektion"))?;
        if srs != 4326 {
            return Err(format!(
                "sprøjtemarkerne ligger i EPSG:{srs} og ikke i WGS84 (EPSG:4326)"
            ));
        }

        let perioder: Vec<u16> =
            sqlx::query_scalar("SELECT DISTINCT aar FROM sproejtemarker ORDER BY aar")
                .fetch_all(&database)
                .await
                .map_err(fejl("planperioder"))?;
        let mut tiles = BTreeMap::new();
        for aar in perioder {
            tiles.insert(aar, pool(&mappe.join(sproejtning_tiles_fil(aar))).await?);
        }

        let udgave =
            sqlx::query_as::<_, (String, String)>("SELECT url, hentet FROM datakilde WHERE id = ?")
                .bind(kilder::SPROEJTNING.id)
                .fetch_optional(&database)
                .await
                .map_err(fejl("sprøjtningens datakilde"))?
                .map(|(url, hentet)| SproejtningUdgave { url, hentet });

        Ok(Some(Sproejtning {
            database,
            tiles,
            udgave,
        }))
    }
}

/// En bedrift med marker på kortet.
pub struct Bedrift {
    pub cvr: String,
    pub marker: i64,
}

pub struct Data {
    pub database: SqlitePool,
    pub tiles: SqlitePool,
    pub overblik: SqlitePool,
    /// Sorteret efter CVR-nummer, så en søgning på de første cifre er et
    /// udsnit af listen.
    pub bedrifter: Vec<Bedrift>,
    pub landsdele: Vec<Landsdel>,
    pub marker_i_alt: i64,
    pub marker_pr_gruppe: HashMap<Gruppe, i64>,
    pub udgave: Option<Udgave>,
    /// Afgrødens navn for hver kode i årets kodeliste.
    pub afgroeder: HashMap<i64, String>,
    pub sproejtning: Option<Sproejtning>,
}

impl Data {
    /// Læser det der ikke ændrer sig mens serveren kører, og holder
    /// forbindelserne åbne til opslagene.
    pub async fn aabn(mappe: &Path) -> std::result::Result<Self, String> {
        let version = dataversion(mappe).await?;
        if version != DATAVERSION {
            return Err(format!(
                "data er bygget i version {version}, men serveren viser version \
                 {DATAVERSION}; venter på at pipelinen bygger nye"
            ));
        }

        let database = pool(&mappe.join(DATABASE_FIL)).await?;
        let tiles = pool(&mappe.join(TILES_FIL)).await?;
        let overblik = pool(&mappe.join(OVERBLIK_FIL)).await?;
        let fejl = |hvad: &'static str| move |e: sqlx::Error| format!("{hvad}: {e}");

        let landsdele = sqlx::query_as::<_, (i64, String, String, f64, f64, f64, f64)>(
            "SELECT fid, nuts3, navn, vest, syd, oest, nord FROM landsdele ORDER BY navn",
        )
        .fetch_all(&database)
        .await
        .map_err(fejl("landsdele"))?
        .into_iter()
        .map(|(nr, nuts3, navn, vest, syd, oest, nord)| Landsdel {
            nr,
            nuts3,
            navn,
            udstraekning: [vest, syd, oest, nord],
        })
        .collect();

        let mut marker_pr_gruppe = HashMap::new();
        let mut marker_i_alt = 0;
        for (noegle, antal) in sqlx::query_as::<_, (String, i64)>(
            "SELECT gruppe, COUNT(*) FROM marker GROUP BY gruppe",
        )
        .fetch_all(&database)
        .await
        .map_err(fejl("marker pr. gruppe"))?
        {
            marker_i_alt += antal;
            let gruppe = Gruppe::fra_noegle(&noegle)
                .ok_or_else(|| format!("ukendt gruppe {noegle:?} i databasen"))?;
            marker_pr_gruppe.insert(gruppe, antal);
        }

        let udgave = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT aar, url, hentet, sidst_aendret FROM datakilde WHERE id = ?",
        )
        .bind(kilder::MARKER.id)
        .fetch_optional(&database)
        .await
        .map_err(fejl("datakilde"))?
        .map(|(aar, url, hentet, sidst_aendret)| Udgave {
            aar,
            url,
            hentet,
            sidst_aendret,
        });

        // Marker uden CVR-nummer står under UDEN_CVR og kan søges frem som
        // en bedrift på linje med de andre.
        let bedrifter = sqlx::query_as::<_, (String, i64)>(
            "SELECT CVR, COUNT(*) FROM marker GROUP BY CVR ORDER BY CVR",
        )
        .fetch_all(&database)
        .await
        .map_err(fejl("bedrifter"))?
        .into_iter()
        .map(|(cvr, marker)| Bedrift { cvr, marker })
        .collect();

        // Sprøjtedata fra tidligere år har kun afgrødekoden, og navnet slås
        // op her. Uden navnene viser historikken kun koderne.
        let afgroeder = tilvalg("afgrødernes navne", afgroeder(&database))
            .await
            .unwrap_or_default();
        let sproejtning = tilvalg("sprøjtelaget", Sproejtning::aabn(mappe))
            .await
            .flatten();

        // Udstrækningen pr. mark kom til i en senere udgave af pipelinen. En
        // database uden den skal bygges igen, og det er bedre at sige det nu
        // end ved første klik.
        sqlx::query("SELECT vest, syd, oest, nord FROM marker LIMIT 1")
            .fetch_optional(&database)
            .await
            .map_err(|e| {
                format!(
                    "{DATABASE_FIL} mangler markernes udstrækning ({e}) — \
                     kør `cargo run -p dkmarkkort-pipeline` igen"
                )
            })?;

        Ok(Data {
            database,
            tiles,
            overblik,
            bedrifter,
            landsdele,
            marker_i_alt,
            marker_pr_gruppe,
            udgave,
            afgroeder,
            sproejtning,
        })
    }
}

async fn afgroeder(database: &SqlitePool) -> std::result::Result<HashMap<i64, String>, String> {
    let raekker =
        sqlx::query_as::<_, (i64, String)>("SELECT afgroedekode, afgroede FROM afgroedekode")
            .fetch_all(database)
            .await
            .map_err(|e| format!("{DATABASE_FIL}: {e}"))?;
    Ok(raekker.into_iter().collect())
}

async fn pool(sti: &Path) -> std::result::Result<SqlitePool, String> {
    if !sti.exists() {
        return Err(format!(
            "{} findes ikke — kør `cargo run -p dkmarkkort-pipeline` først",
            sti.display()
        ));
    }
    SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::new()
                .filename(sti)
                .read_only(true)
                .immutable(true),
        )
        .await
        .map_err(|e| format!("kunne ikke åbne {}: {e}", sti.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_uden_versionsfil_er_version_1() {
        assert_eq!(fortolk_dataversion(None), Ok(1));
    }

    #[test]
    fn versionsfilen_er_et_tal() {
        assert_eq!(fortolk_dataversion(Some("2\n")), Ok(2));
        assert!(fortolk_dataversion(Some("to")).is_err());
        assert!(fortolk_dataversion(Some("")).is_err());
    }
}
