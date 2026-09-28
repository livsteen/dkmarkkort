use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// Pakker en zip ud i `mappe`.
///
/// Stierne i zip'en bruges kun hvis de holder sig inden for mappen: en post
/// der hedder `../noget` er ikke en fil vi vil skrive.
pub async fn udpak(zip: &Path, mappe: &Path) -> Result<()> {
    let zip = zip.to_owned();
    let mappe = mappe.to_owned();
    tokio::task::spawn_blocking(move || udpak_blokerende(&zip, &mappe)).await?
}

fn udpak_blokerende(zip: &Path, mappe: &Path) -> Result<()> {
    let fil = File::open(zip).with_context(|| format!("kunne ikke åbne {}", zip.display()))?;
    let mut arkiv = zip::ZipArchive::new(fil)?;
    fs::create_dir_all(mappe)?;

    for i in 0..arkiv.len() {
        let mut post = arkiv.by_index(i)?;
        let Some(relativ) = post.enclosed_name() else {
            bail!("{} indeholder en sti uden for arkivet", zip.display());
        };
        let ud: PathBuf = mappe.join(relativ);
        if post.is_dir() {
            fs::create_dir_all(&ud)?;
            continue;
        }
        if let Some(foraelder) = ud.parent() {
            fs::create_dir_all(foraelder)?;
        }
        let mut ud_fil = File::create(&ud)?;
        io::copy(&mut post, &mut ud_fil)?;
    }
    Ok(())
}
