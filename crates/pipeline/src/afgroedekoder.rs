//! Afgrødekoderne fra `data/afgroedekoder-<år>.csv` og deres gruppe.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result, bail};
use dkmarkkort_core::gruppe::{Gruppe, gruppe_for_afsnit};

/// Koder som oversigten placerer i afsnit med hver sin gruppe, og hvor
/// valget er truffet i hånden.
const AFKLAREDE: [(u32, Gruppe); 1] = [
    // "Klima-lavbundsprojekt, national ordning" står i 2026 både under
    // særlige koder for tilsagn og miljøtiltag og under energiafgrøder.
    // Lavbundsprojekter er et klimatiltag.
    (567, Gruppe::NaturOgMiljoetilsagn),
];

pub struct Afgroedekode {
    pub kode: u32,
    /// Står koden under flere afsnit, kan navnet være skrevet forskelligt;
    /// det første i oversigten bruges.
    pub afgroede: String,
    /// Afsnittene koden står under, adskilt af " / " hvis flere.
    pub afsnit: String,
    pub gruppe: Gruppe,
}

pub fn laes(sti: &Path) -> Result<Vec<Afgroedekode>> {
    let mut laeser = csv::Reader::from_path(sti).with_context(|| {
        format!(
            "mangler {} — lav den med `--bin afgroedekoder` ud fra årets oversigt",
            sti.display()
        )
    })?;

    let mut pr_kode = BTreeMap::<u32, (String, BTreeSet<String>)>::new();
    for raekke in laeser.records() {
        let raekke = raekke?;
        let (Some(kode), Some(afgroede), Some(afsnit)) =
            (raekke.get(0), raekke.get(1), raekke.get(2))
        else {
            bail!("{}: række med for få kolonner", sti.display());
        };
        let kode: u32 = kode
            .parse()
            .with_context(|| format!("ugyldig afgrødekode {kode:?}"))?;
        pr_kode
            .entry(kode)
            .or_insert_with(|| (afgroede.to_owned(), BTreeSet::new()))
            .1
            .insert(afsnit.to_owned());
    }

    let mut koder = Vec::with_capacity(pr_kode.len());
    for (kode, (afgroede, afsnit)) in pr_kode {
        let mut grupper = BTreeSet::new();
        for navn in &afsnit {
            let gruppe = gruppe_for_afsnit(navn).with_context(|| {
                format!("afsnittet {navn:?} har ingen gruppe — tilføj det i core/src/gruppe.rs")
            })?;
            grupper.insert(gruppe.noegle());
        }
        let afklaret = AFKLAREDE.iter().find(|(k, _)| *k == kode).map(|(_, g)| *g);
        let gruppe = match (grupper.len(), afklaret) {
            (_, Some(gruppe)) => gruppe,
            (1, None) => gruppe_for_afsnit(afsnit.first().expect("mindst ét afsnit"))
                .expect("tjekket ovenfor"),
            _ => bail!(
                "afgrødekode {kode} står under afsnit med forskellige grupper ({}) — \
                 afklar den i AFKLAREDE",
                afsnit.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        };
        koder.push(Afgroedekode {
            kode,
            afgroede,
            afsnit: afsnit.into_iter().collect::<Vec<_>>().join(" / "),
            gruppe,
        });
    }
    Ok(koder)
}

/// Én række pr. kode, klar til at joine markerne mod. Navnet kommer med,
/// så sprøjtedata fra tidligere år kan vise afgrødens navn ud fra koden.
pub fn skriv_opslag(koder: &[Afgroedekode], sti: &Path) -> Result<()> {
    let mut skriver = csv::Writer::from_path(sti)?;
    skriver.write_record(["afgroedekode", "afgroede", "afsnit", "gruppe"])?;
    for kode in koder {
        skriver.write_record([
            &kode.kode.to_string(),
            &kode.afgroede,
            &kode.afsnit,
            kode.gruppe.noegle(),
        ])?;
    }
    skriver.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv_fil(indhold: &str) -> tempfil::Tempfil {
        tempfil::Tempfil::ny(indhold)
    }

    #[test]
    fn samme_gruppe_under_to_afsnit_er_i_orden() {
        let fil = csv_fil(
            "afgroedekode,afgroede,afsnit\n\
             319,A,Arealer med tilsagn under miljøordningerne\n\
             319,B,Særlige afgrødekoder i forbindelse med tilsagn eller miljøtiltag\n",
        );
        let koder = laes(fil.sti()).unwrap();
        assert_eq!(koder.len(), 1);
        assert_eq!(koder[0].gruppe, Gruppe::NaturOgMiljoetilsagn);
        assert!(koder[0].afsnit.contains(" / "));
        assert_eq!(koder[0].afgroede, "A");
    }

    #[test]
    fn forskellige_grupper_kraever_afklaring() {
        let fil = csv_fil(
            "afgroedekode,afgroede,afsnit\n\
             9999,A,Kartofler\n\
             9999,B,Frøgræs\n",
        );
        assert!(laes(fil.sti()).is_err());
    }

    #[test]
    fn afklarede_koder_bruger_det_valgte() {
        let fil = csv_fil(
            "afgroedekode,afgroede,afsnit\n\
             567,A,Særlige afgrødekoder i forbindelse med tilsagn eller miljøtiltag\n\
             567,B,Energiafgrøder og anden særlig produktion\n",
        );
        assert_eq!(
            laes(fil.sti()).unwrap()[0].gruppe,
            Gruppe::NaturOgMiljoetilsagn
        );
    }

    #[test]
    fn ukendt_afsnit_stopper() {
        let fil = csv_fil("afgroedekode,afgroede,afsnit\n1,A,Nyt afsnit\n");
        assert!(laes(fil.sti()).is_err());
    }

    /// En midlertidig fil der slettes igen, uden en ekstra afhængighed.
    mod tempfil {
        use std::path::{Path, PathBuf};

        pub struct Tempfil(PathBuf);

        impl Tempfil {
            pub fn ny(indhold: &str) -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static NR: AtomicU32 = AtomicU32::new(0);
                let sti = std::env::temp_dir().join(format!(
                    "dkmarkkort-test-{}-{}.csv",
                    std::process::id(),
                    NR.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::write(&sti, indhold).unwrap();
                Tempfil(sti)
            }

            pub fn sti(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Tempfil {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }
}
