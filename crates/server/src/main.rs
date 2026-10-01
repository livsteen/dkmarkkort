//! Serveren bag markkortet: siderne, vektortiles'ene og kortets egne filer.
//!
//! Data bygges af `dkmarkkort-pipeline` og læses fra mappen i
//! `MARKKORT_DATA` (standard: `data`). Serveren starter også uden data og
//! viser dem, når pipelinen har bygget dem. Serveren lytter på `HOST` og `PORT`
//! (standard: 127.0.0.1:3000).

mod data;
mod opslag;
mod sider;
mod tiles;

use std::{path::PathBuf, process::exit};

use topcoat::{
    asset::{AssetBundle, RouterBuilderAssetExt},
    router::{Router, RouterBuilderDiscoverExt},
};

use crate::data::Kortdata;

#[tokio::main]
async fn main() {
    let mappe = std::env::var_os("MARKKORT_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));

    let data = Kortdata::hold_opdateret(mappe).await;

    let assets = match AssetBundle::load() {
        Ok(assets) => assets,
        Err(fejl) => {
            eprintln!(
                "dkmarkkort: kunne ikke læse asset-bundtet ({fejl}). \
                 Kør med `topcoat dev`, eller byg det med `topcoat asset bundle`."
            );
            exit(1);
        }
    };

    let router = Router::builder()
        .discover()
        .assets(assets)
        .app_context(data)
        .build();

    if let Err(fejl) = topcoat::start(router).await {
        eprintln!("dkmarkkort: {fejl}");
        exit(1);
    }
}
