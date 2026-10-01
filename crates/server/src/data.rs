//! Serverens adgang til databasen, tiles'ene og oversigten. Alle er
//! SQLite-filer som pipelinen har bygget, og alle åbnes skrivebeskyttet.
//!
//! Serveren starter også uden data og holder øje med pipelinens `bygget`-fil.
//! Når den ændrer sig, åbnes den nye udgave, og den gamle lukkes, når den
//! sidste forespørgsel har sluppet den.
//!
//! Det der ikke ændrer sig og bruges ved hvert opslag, læses én gang ved
//! start: landsdelene, optællingerne og listen over bedrifter.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
    time::Duration,
};

use dkmarkkort_core::{BYGGET_FIL, DATABASE_FIL, OVERBLIK_FIL, TILES_FIL, gruppe::Gruppe, kilder};
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
#[derive(Clone, Default)]
pub struct Kortdata(Arc<RwLock<Option<Arc<Data>>>>);

impl Kortdata {
    /// Åbner data i `mappe`, hvis de findes, og ser derefter efter nye hvert
    /// [`KIG_EFTER`].
    pub async fn hold_opdateret(mappe: PathBuf) -> Self {
        let kortdata = Kortdata::default();
        let mut opdatering = Opdatering::default();
        opdatering.koer(&kortdata, &mappe).await;

        let baggrund = kortdata.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(KIG_EFTER).await;
                opdatering.koer(&baggrund, &mappe).await;
            }
        });
        kortdata
    }

    pub fn hent(&self) -> Option<Arc<Data>> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn saet(&self, data: Data) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(data));
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
    async fn koer(&mut self, kortdata: &Kortdata, mappe: &Path) {
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
}

impl Data {
    /// Læser det der ikke ændrer sig mens serveren kører, og holder
    /// forbindelserne åbne til opslagene.
    pub async fn aabn(mappe: &Path) -> std::result::Result<Self, String> {
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

        // Marker uden CVR-nummer hører ikke til nogen bedrift, man kan søge
        // frem. De kan stadig vælges på kortet.
        let bedrifter = sqlx::query_as::<_, (String, i64)>(
            "SELECT CVR, COUNT(*) FROM marker WHERE CVR <> '' GROUP BY CVR ORDER BY CVR",
        )
        .fetch_all(&database)
        .await
        .map_err(fejl("bedrifter"))?
        .into_iter()
        .map(|(cvr, marker)| Bedrift { cvr, marker })
        .collect();

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
        })
    }
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
