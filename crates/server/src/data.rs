//! Serverens adgang til databasen og tiles'ene. Begge er SQLite-filer som
//! pipelinen har bygget, og begge åbnes skrivebeskyttet: en ny udgave af data
//! kræver at serveren startes igen.

use std::{collections::HashMap, path::Path};

use dkmarkkort_core::{DATABASE_FIL, TILES_FIL, gruppe::Gruppe, kilder};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

pub struct Landsdel {
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

pub struct Data {
    pub tiles: SqlitePool,
    pub landsdele: Vec<Landsdel>,
    pub marker_i_alt: i64,
    pub marker_pr_gruppe: HashMap<Gruppe, i64>,
    pub udgave: Option<Udgave>,
}

impl Data {
    /// Læser det der ikke ændrer sig mens serveren kører, og holder
    /// forbindelsen til tiles'ene åben.
    pub async fn aabn(mappe: &Path) -> Result<Self, String> {
        let database = pool(&mappe.join(DATABASE_FIL)).await?;
        let tiles = pool(&mappe.join(TILES_FIL)).await?;
        let fejl = |hvad: &'static str| move |e: sqlx::Error| format!("{hvad}: {e}");

        let landsdele = sqlx::query_as::<_, (String, String, f64, f64, f64, f64)>(
            "SELECT nuts3, navn, vest, syd, oest, nord FROM landsdele ORDER BY navn",
        )
        .fetch_all(&database)
        .await
        .map_err(fejl("landsdele"))?
        .into_iter()
        .map(|(nuts3, navn, vest, syd, oest, nord)| Landsdel {
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

        database.close().await;
        Ok(Data {
            tiles,
            landsdele,
            marker_i_alt,
            marker_pr_gruppe,
            udgave,
        })
    }
}

async fn pool(sti: &Path) -> Result<SqlitePool, String> {
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
