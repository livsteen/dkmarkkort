//! Afgrødegrupperne kortet farver efter.
//!
//! Landbrugsstyrelsen ordner afgrødekoderne i sin oversigt til Fællesskemaet
//! under 24 afsnit. De er lagt sammen til seks grupper, så farverne kan
//! skelnes på et kort, plus en gruppe til koder som oversigten ikke kender.
//! Hvilket afsnit en kode hører til, står i `data/afgroedekoder-<år>.csv`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gruppe {
    Korn,
    GraesOgGrovfoder,
    FroeOlieOgBaelgsaed,
    KartoflerRoerOgHavebrug,
    NaturOgMiljoetilsagn,
    SkovEnergiOgOevrige,
    UkendtKode,
}

impl Gruppe {
    /// Rækkefølgen i signaturen. Farverne er valideret som sæt i netop
    /// denne rækkefølge, så den ændres ikke uden at validere igen.
    pub const ALLE: [Gruppe; 7] = [
        Gruppe::Korn,
        Gruppe::GraesOgGrovfoder,
        Gruppe::FroeOlieOgBaelgsaed,
        Gruppe::KartoflerRoerOgHavebrug,
        Gruppe::NaturOgMiljoetilsagn,
        Gruppe::SkovEnergiOgOevrige,
        Gruppe::UkendtKode,
    ];

    /// Værdien i databasen og i tiles'enes `gruppe`-attribut.
    pub fn noegle(self) -> &'static str {
        match self {
            Gruppe::Korn => "korn",
            Gruppe::GraesOgGrovfoder => "graes",
            Gruppe::FroeOlieOgBaelgsaed => "froe",
            Gruppe::KartoflerRoerOgHavebrug => "havebrug",
            Gruppe::NaturOgMiljoetilsagn => "natur",
            Gruppe::SkovEnergiOgOevrige => "skov",
            Gruppe::UkendtKode => "ukendt",
        }
    }

    pub fn fra_noegle(noegle: &str) -> Option<Gruppe> {
        Gruppe::ALLE.into_iter().find(|g| g.noegle() == noegle)
    }

    /// Gruppens nummer i oversigtens pixels. Det står i data som pipelinen
    /// har bygget, så et nummer skifter ikke betydning; en ny gruppe får et
    /// nyt. Der er plads til 1–15.
    pub fn nr(self) -> u8 {
        match self {
            Gruppe::Korn => 1,
            Gruppe::GraesOgGrovfoder => 2,
            Gruppe::FroeOlieOgBaelgsaed => 3,
            Gruppe::KartoflerRoerOgHavebrug => 4,
            Gruppe::NaturOgMiljoetilsagn => 5,
            Gruppe::SkovEnergiOgOevrige => 6,
            Gruppe::UkendtKode => 7,
        }
    }

    pub fn navn(self) -> &'static str {
        match self {
            Gruppe::Korn => "Korn til modenhed",
            Gruppe::GraesOgGrovfoder => "Græs og grovfoder",
            Gruppe::FroeOlieOgBaelgsaed => "Frø, olie og bælgsæd",
            Gruppe::KartoflerRoerOgHavebrug => "Kartofler, roer og havebrug",
            Gruppe::NaturOgMiljoetilsagn => "Natur og miljøtilsagn",
            Gruppe::SkovEnergiOgOevrige => "Skov, energi og øvrige",
            Gruppe::UkendtKode => "Ukendt kode",
        }
    }

    /// De seks farver er valgt og valideret så alle par kan skelnes med
    /// normalt syn (ΔE ≥ 15). For farveblinde er det svageste par tættere
    /// (ΔE 6,9), så gruppen står altid også som tekst i signaturen og kan
    /// slås til og fra. Ukendt kode er grå, fordi den ikke er en afgrøde.
    pub fn farve(self) -> &'static str {
        match self {
            Gruppe::Korn => "#eda100",
            Gruppe::GraesOgGrovfoder => "#008300",
            Gruppe::FroeOlieOgBaelgsaed => "#4a3aa7",
            Gruppe::KartoflerRoerOgHavebrug => "#e34948",
            Gruppe::NaturOgMiljoetilsagn => "#2a78d6",
            Gruppe::SkovEnergiOgOevrige => "#1baf7a",
            Gruppe::UkendtKode => "#898781",
        }
    }
}

/// Gruppen for et afsnit i Landbrugsstyrelsens oversigt over afgrødekoder.
///
/// `None` betyder at afsnittet er nyt. Pipelinen stopper så, frem for at
/// gætte, og afsnittet skal placeres her.
pub fn gruppe_for_afsnit(afsnit: &str) -> Option<Gruppe> {
    let gruppe = match afsnit {
        "Vårsæd til modenhed" | "Vintersæd til modenhed" => Gruppe::Korn,

        "Helsæd, vår"
        | "Helsæd, vinter"
        | "Græs, permanent"
        | "Græsmarksplanter, omdrift"
        | "Kløver og lucerne i renbestand"
        | "Andre foderafgrøder" => Gruppe::GraesOgGrovfoder,

        "Oliefrø og Bælgsæd" | "Hør og Hamp" | "Frøgræs" | "Havefrø" => {
            Gruppe::FroeOlieOgBaelgsaed
        }

        "Kartofler"
        | "Rodfrugter til fabrik"
        | "Grøntsager, friland"
        | "Frugt og bær"
        | "Småplanteproduktion og planteskoleplanter"
        | "Medicinplanter" => Gruppe::KartoflerRoerOgHavebrug,

        "Udyrkede arealer, vildtagre o.l."
        | "Arealer med tilsagn under miljøordningerne"
        | "Særlige afgrødekoder i forbindelse med tilsagn eller miljøtiltag" => {
            Gruppe::NaturOgMiljoetilsagn
        }

        "Trækulturer" | "Energiafgrøder og anden særlig produktion" | "Øvrige afgrøder" => {
            Gruppe::SkovEnergiOgOevrige
        }

        _ => return None,
    };
    Some(gruppe)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Afsnittene i oversigten for 2026, som de står i PDF'en.
    const AFSNIT_2026: [&str; 24] = [
        "Vårsæd til modenhed",
        "Vintersæd til modenhed",
        "Oliefrø og Bælgsæd",
        "Hør og Hamp",
        "Frøgræs",
        "Kartofler",
        "Rodfrugter til fabrik",
        "Helsæd, vår",
        "Helsæd, vinter",
        "Græs, permanent",
        "Græsmarksplanter, omdrift",
        "Kløver og lucerne i renbestand",
        "Andre foderafgrøder",
        "Grøntsager, friland",
        "Udyrkede arealer, vildtagre o.l.",
        "Arealer med tilsagn under miljøordningerne",
        "Særlige afgrødekoder i forbindelse med tilsagn eller miljøtiltag",
        "Medicinplanter",
        "Havefrø",
        "Småplanteproduktion og planteskoleplanter",
        "Frugt og bær",
        "Trækulturer",
        "Energiafgrøder og anden særlig produktion",
        "Øvrige afgrøder",
    ];

    #[test]
    fn alle_afsnit_i_2026_har_en_gruppe() {
        for afsnit in AFSNIT_2026 {
            assert!(gruppe_for_afsnit(afsnit).is_some(), "{afsnit}");
        }
    }

    #[test]
    fn et_ukendt_afsnit_giver_ingen_gruppe() {
        assert_eq!(gruppe_for_afsnit("Et nyt afsnit"), None);
    }

    #[test]
    fn ingen_afsnit_lander_i_ukendt_kode() {
        for afsnit in AFSNIT_2026 {
            assert_ne!(gruppe_for_afsnit(afsnit), Some(Gruppe::UkendtKode));
        }
    }

    #[test]
    fn noegler_er_unikke_og_kan_slaas_op() {
        for gruppe in Gruppe::ALLE {
            assert_eq!(Gruppe::fra_noegle(gruppe.noegle()), Some(gruppe));
        }
        let mut noegler: Vec<_> = Gruppe::ALLE.iter().map(|g| g.noegle()).collect();
        noegler.sort_unstable();
        noegler.dedup();
        assert_eq!(noegler.len(), Gruppe::ALLE.len());
    }

    #[test]
    fn numre_er_unikke_og_har_plads_i_en_pixel() {
        let mut numre: Vec<_> = Gruppe::ALLE.iter().map(|g| g.nr()).collect();
        assert!(numre.iter().all(|nr| (1..=15).contains(nr)));
        numre.sort_unstable();
        numre.dedup();
        assert_eq!(numre.len(), Gruppe::ALLE.len());
    }
}
