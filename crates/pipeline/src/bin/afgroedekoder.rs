//! Trækker afgrødekoderne ud af Landbrugsstyrelsens oversigt over
//! afgrødekoder til Fællesskemaet og skriver dem som CSV.
//!
//! ```text
//! cargo run -p dkmarkkort-pipeline --bin afgroedekoder -- <oversigt.pdf> data/afgroedekoder-2026.csv
//! ```
//!
//! Oversigten udgives kun som PDF. Den køres én gang pr. år, og CSV'en
//! gennemgås og committes; pipelinen læser kun CSV'en. Kræver `pdftotext`
//! (poppler).
//!
//! PDF'en er en tabel under overskrifter. Med `pdftotext -layout` står en
//! overskrift yderst til venstre, og en række er indrykket. En lang
//! afgrødenavn brækkes over flere linjer, og koden kan stå på enhver af dem,
//! så en række samles linje for linje, indtil kolonnerne med ja/nej og
//! kategorinummer er set.

use std::{collections::BTreeMap, path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

#[derive(Debug, PartialEq)]
struct Afgroedekode {
    kode: u32,
    afgroede: String,
    afsnit: String,
}

fn main() -> Result<()> {
    let argumenter: Vec<String> = std::env::args().skip(1).collect();
    let [pdf, csv] = argumenter.as_slice() else {
        bail!("brug: afgroedekoder <oversigt.pdf> <ud.csv>");
    };

    let tekst = pdf_til_tekst(&PathBuf::from(pdf))?;
    let koder = fortolk(&tekst)?;

    let mut skriver = csv::Writer::from_path(csv)?;
    skriver.write_record(["afgroedekode", "afgroede", "afsnit"])?;
    for kode in &koder {
        skriver.write_record([&kode.kode.to_string(), &kode.afgroede, &kode.afsnit])?;
    }
    skriver.flush()?;

    let mut pr_afsnit = BTreeMap::<&str, usize>::new();
    for kode in &koder {
        *pr_afsnit.entry(&kode.afsnit).or_default() += 1;
    }
    println!("{} afgrødekoder i {} afsnit:", koder.len(), pr_afsnit.len());
    for (afsnit, antal) in pr_afsnit {
        println!("  {antal:>3}  {afsnit}");
    }
    Ok(())
}

fn pdf_til_tekst(pdf: &PathBuf) -> Result<String> {
    let ud = Command::new("pdftotext")
        .args(["-layout", "-enc", "UTF-8"])
        .arg(pdf)
        .arg("-")
        .output()
        .context("kunne ikke starte pdftotext")?;
    if !ud.status.success() {
        bail!(
            "pdftotext fejlede: {}",
            String::from_utf8_lossy(&ud.stderr).trim()
        );
    }
    Ok(String::from_utf8(ud.stdout)?)
}

/// En række under opbygning: navnestykker og kode samles, til linjen med
/// ja/nej-kolonnerne afslutter den.
#[derive(Default)]
struct Delvis {
    kode: Option<u32>,
    navn: Vec<String>,
}

fn fortolk(tekst: &str) -> Result<Vec<Afgroedekode>> {
    let mut koder = Vec::new();
    let mut afsnit: Option<String> = None;
    let mut delvis = Delvis::default();

    for (nr, linje) in tekst.lines().enumerate() {
        // pdftotext markerer sideskift med et formfeed først på linjen.
        let linje = linje.trim_start_matches('\u{c}').trim_end();
        if linje.trim().is_empty() || linje.trim_start().starts_with("Afgrødekode") {
            continue;
        }

        if !linje.starts_with(char::is_whitespace) {
            if delvis.kode.is_some() || !delvis.navn.is_empty() {
                bail!("linje {}: ny overskrift midt i en række", nr + 1);
            }
            afsnit = Some(linje.trim().to_owned());
            continue;
        }

        let Some(afsnit) = &afsnit else {
            bail!("linje {}: række før første overskrift", nr + 1);
        };

        let kolonner: Vec<&str> = linje
            .split("  ")
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .collect();
        let mut resten = kolonner.as_slice();

        if let Some((foerste, efter)) = resten.split_first()
            && let Ok(kode) = foerste.parse::<u32>()
        {
            if delvis.kode.replace(kode).is_some() {
                bail!("linje {}: to koder i samme række", nr + 1);
            }
            resten = efter;
        }

        if let Some((navn, _)) = resten.split_first()
            && !er_ja_nej(navn)
            && navn.parse::<u32>().is_err()
        {
            delvis.navn.push((*navn).to_owned());
        }

        if har_ja_nej_kolonner(&kolonner) {
            let Some(kode) = delvis.kode else {
                bail!("linje {}: række uden afgrødekode", nr + 1);
            };
            koder.push(Afgroedekode {
                kode,
                afgroede: delvis.navn.join(" "),
                afsnit: afsnit.clone(),
            });
            delvis = Delvis::default();
        }
    }

    if delvis.kode.is_some() {
        bail!("teksten slutter midt i en række");
    }

    // Oversigten kan selv nævne en kode to gange (i 2026 står 319 under to
    // afsnit med to navne). Begge rækker beholdes, så CSV'en viser kilden som
    // den er; pipelinen afviser koden, hvis rækkerne giver hver sin gruppe.
    let mut set = std::collections::HashSet::new();
    for kode in &koder {
        if !set.insert(kode.kode) {
            eprintln!(
                "bemærk: afgrødekode {} står mere end én gang i oversigten",
                kode.kode
            );
        }
    }
    if koder.is_empty() {
        bail!("fandt ingen afgrødekoder");
    }
    Ok(koder)
}

fn er_ja_nej(tekst: &str) -> bool {
    matches!(tekst, "ja" | "nej")
}

/// Grundbetaling og landbrugsareal står som to ja/nej lige efter hinanden,
/// fulgt af kategorinummeret. Det er den del af rækken der aldrig brækkes.
fn har_ja_nej_kolonner(kolonner: &[&str]) -> bool {
    kolonner
        .windows(3)
        .any(|v| er_ja_nej(v[0]) && er_ja_nej(v[1]) && v[2].parse::<u32>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kode(kode: u32, afgroede: &str, afsnit: &str) -> Afgroedekode {
        Afgroedekode {
            kode,
            afgroede: afgroede.to_owned(),
            afsnit: afsnit.to_owned(),
        }
    }

    #[test]
    fn almindelige_raekker() {
        let tekst = "\
          Afgrødekode        Afgrøde        Grundbetaling Landbrugsareal   Afgrødekategori nr.    Afgrødekategori   Omdrift
Kartofler
              150          Stivelseskartofler              ja   ja    85   kartoffel        ja
              151          Læggekartofler                  ja   ja    85   kartoffel        ja
";
        assert_eq!(
            fortolk(tekst).unwrap(),
            vec![
                kode(150, "Stivelseskartofler", "Kartofler"),
                kode(151, "Læggekartofler", "Kartofler"),
            ]
        );
    }

    #[test]
    fn navn_brudt_foer_koden() {
        let tekst = "\
Græs, permanent
                            Permanent græs og kløvergræs uden norm,
              276           under 50 % kløver                     ja   ja    0    (afgrøden har ingen kategori)       nej
";
        assert_eq!(
            fortolk(tekst).unwrap(),
            vec![kode(
                276,
                "Permanent græs og kløvergræs uden norm, under 50 % kløver",
                "Græs, permanent"
            )]
        );
    }

    #[test]
    fn navn_brudt_efter_koden() {
        let tekst = "\
Græsmarksplanter, omdrift
              284           Græs med vikke og andre bælgplanter, under                            græs eller andet grøntfoder
                            50 % bælgpl.                                          ja   ja   56                                        ja
";
        assert_eq!(
            fortolk(tekst).unwrap(),
            vec![kode(
                284,
                "Græs med vikke og andre bælgplanter, under 50 % bælgpl.",
                "Græsmarksplanter, omdrift"
            )]
        );
    }

    #[test]
    fn kode_alene_paa_sin_linje() {
        let tekst = "\
Udyrkede arealer, vildtagre o.l.
                              Brak langs vandløb og søer, slåning (alternativ
           345
                              til efterafgrøder)                                ja    ja    53    brak                            ja
";
        assert_eq!(
            fortolk(tekst).unwrap(),
            vec![kode(
                345,
                "Brak langs vandløb og søer, slåning (alternativ til efterafgrøder)",
                "Udyrkede arealer, vildtagre o.l."
            )]
        );
    }

    #[test]
    fn sideskift_bryder_ikke_afsnittet() {
        let tekst = "\
Kartofler
              150          Stivelseskartofler              ja   ja    85   kartoffel        ja
\u{c}              152           Kartofler, spise- (pakkeri, vejsalg)    ja   ja    85   kartoffel   ja
";
        let koder = fortolk(tekst).unwrap();
        assert_eq!(
            koder[1],
            kode(152, "Kartofler, spise- (pakkeri, vejsalg)", "Kartofler")
        );
    }

    #[test]
    fn en_kode_der_staar_to_gange_beholdes_begge_steder() {
        let tekst = "\
Kartofler
              150          A              ja   ja    85   kartoffel        ja
Frugt og bær
              150          B              ja   ja    85   kartoffel        ja
";
        assert_eq!(
            fortolk(tekst).unwrap(),
            vec![kode(150, "A", "Kartofler"), kode(150, "B", "Frugt og bær")]
        );
    }
}
