//! Hentning af kildefiler.
//!
//! Filerne er store — marker-zip'en er ~350 MB — så de streames til disk og
//! hentes kun hvis de ikke ligger der i forvejen. Hvornår en fil er hentet,
//! og hvad serveren sagde den sidst var ændret, gemmes i en fil ved siden af:
//! markdata opdateres dagligt, så "hvilken udgave viser kortet" skal kunne
//! besvares bagefter.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use jiff::Timestamp;
use reqwest::header::LAST_MODIFIED;
use tokio::{fs, io::AsyncWriteExt};

/// En HTTP-klient der siger hvem den er. Zenodo afviser forespørgsler uden
/// User-Agent.
pub fn klient() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "dkmarkkort-pipeline/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/livsteen/dkmarkkort)"
        ))
        .build()?)
}

/// Hvor og hvornår en kildefil er hentet.
pub struct Hentet {
    pub url: String,
    pub hentet: String,
    /// Serverens `Last-Modified`, hvis den sendte en.
    pub sidst_aendret: Option<String>,
}

/// Henter `url` til `destination`, med mindre filen allerede findes.
///
/// Downloaden skrives til en `.part`-fil og omdøbes først når den er
/// komplet, så en afbrudt kørsel aldrig efterlader en halv fil der ligner en
/// hel.
pub async fn hent(url: &str, destination: &Path) -> Result<Hentet> {
    let oplysninger = oplysningsfil(destination);

    if fs::try_exists(destination).await? {
        println!("    {} findes allerede", destination.display());
        return laes_oplysninger(&oplysninger, url).await;
    }

    let svar = klient()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("kunne ikke hente {url}"))?;
    if !svar.status().is_success() {
        bail!("{url} svarede {}", svar.status());
    }

    let sidst_aendret = svar
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let total = svar.content_length();

    let delvis = destination.with_extension("part");
    let mut fil = fs::File::create(&delvis).await?;
    let mut stroem = svar.bytes_stream();
    let mut hentet_bytes: u64 = 0;
    let mut naeste_melding: u64 = 0;

    while let Some(stykke) = stroem.next().await {
        let stykke = stykke.with_context(|| format!("download af {url} blev afbrudt"))?;
        fil.write_all(&stykke).await?;
        hentet_bytes += stykke.len() as u64;
        if hentet_bytes >= naeste_melding {
            match total {
                Some(total) => println!("    {} / {} MB", mb(hentet_bytes), mb(total)),
                None => println!("    {} MB", mb(hentet_bytes)),
            }
            naeste_melding += 50 * 1024 * 1024;
        }
    }
    fil.flush().await?;
    drop(fil);
    fs::rename(&delvis, destination).await?;
    println!("    {} MB hentet", mb(hentet_bytes));

    let hentet = Hentet {
        url: url.to_owned(),
        hentet: Timestamp::now().to_string(),
        sidst_aendret,
    };
    skriv_oplysninger(&oplysninger, &hentet).await?;
    Ok(hentet)
}

fn oplysningsfil(destination: &Path) -> PathBuf {
    let mut navn = destination.as_os_str().to_owned();
    navn.push(".hentet");
    PathBuf::from(navn)
}

async fn skriv_oplysninger(sti: &Path, hentet: &Hentet) -> Result<()> {
    let mut tekst = format!("url={}\nhentet={}\n", hentet.url, hentet.hentet);
    if let Some(sidst_aendret) = &hentet.sidst_aendret {
        tekst.push_str(&format!("sidst_aendret={sidst_aendret}\n"));
    }
    fs::write(sti, tekst).await?;
    Ok(())
}

/// Læser oplysningerne om en fil der er hentet tidligere. Mangler de — fordi
/// filen er lagt der i hånden — er hentetidspunktet ukendt, og det siges
/// ligeud frem for at blive gættet.
async fn laes_oplysninger(sti: &Path, url: &str) -> Result<Hentet> {
    let mut hentet = Hentet {
        url: url.to_owned(),
        hentet: "ukendt".to_owned(),
        sidst_aendret: None,
    };
    let Ok(tekst) = fs::read_to_string(sti).await else {
        return Ok(hentet);
    };
    for linje in tekst.lines() {
        match linje.split_once('=') {
            Some(("url", v)) => hentet.url = v.to_owned(),
            Some(("hentet", v)) => hentet.hentet = v.to_owned(),
            Some(("sidst_aendret", v)) => hentet.sidst_aendret = Some(v.to_owned()),
            _ => {}
        }
    }
    Ok(hentet)
}

fn mb(bytes: u64) -> u64 {
    bytes / 1024 / 1024
}
