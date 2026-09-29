//! Hvor data kommer fra, og hvad vi skylder udgiverne.
//!
//! Siden `/kilder` og krediteringen nederst på kortet læser begge herfra,
//! så de ikke kan komme til at sige noget forskelligt. Pipelinen gemmer
//! hvornår en kilde er hentet under kildens `id`.

pub struct Kilde {
    pub id: &'static str,
    pub titel: &'static str,
    pub udgiver: &'static str,
    pub url: &'static str,
    pub licens: &'static str,
    pub licens_url: Option<&'static str>,
    /// Den korte tekst på kortet.
    pub kreditering: &'static str,
    /// Hvordan vi har bearbejdet data. CC BY 4.0 kræver at det står.
    pub bearbejdning: Option<&'static str>,
    pub note: Option<&'static str>,
}

pub const MARKER: Kilde = Kilde {
    id: "marker",
    titel: "Markkort med afgrødekode, areal og CVR",
    udgiver: "Landbrugsstyrelsen",
    url: "https://landbrugsgeodata.fvm.dk/",
    licens: "Ingen licens angivet",
    licens_url: None,
    kreditering: "Marker: Landbrugsstyrelsen",
    bearbejdning: Some(
        "Hver mark har fået den landsdel den ligger i og en afgrødegruppe ud fra sin \
         afgrødekode. Geometrien er forenklet i vektortiles'ene. Zoomet ud vises \
         markerne som et billede, hvor hvert punkt har den gruppe der fylder mest.",
    ),
    note: Some(
        "LandbrugsGIS oplyser hverken licens eller krav til kildeangivelse. Vi angiver \
         Landbrugsstyrelsen som udgiver.",
    ),
};

pub const AFGROEDEKODER: Kilde = Kilde {
    id: "afgroedekoder",
    titel: "Oversigt over afgrødekoder til Fællesskemaet",
    udgiver: "Landbrugsstyrelsen",
    url: "https://lbst.dk/tilskud/tast-selv/afgroedekoder",
    licens: "Ingen licens angivet",
    licens_url: None,
    kreditering: "Afgrødekoder: Landbrugsstyrelsen",
    bearbejdning: Some(
        "Afgrødekode, afgrøde og afsnit er trukket ud af PDF'en. De 24 afsnit er lagt \
         sammen til seks afgrødegrupper.",
    ),
    note: Some("Oversigten udgives kun som PDF og oplyser ingen licens."),
};

pub const LANDSDELE: Kilde = Kilde {
    id: "landsdele",
    titel: "Landsdele (NUTS3) fra DAGI",
    udgiver: "Klimadatastyrelsen",
    url: "https://datafordeler.dk/vejledning/brugervilkaar/kds-geografiske-data/",
    licens: "CC BY 4.0",
    licens_url: Some("https://creativecommons.org/licenses/by/4.0/deed.da"),
    kreditering: "Landsdele: Klimadatastyrelsen (DAGI), CC BY 4.0, bearbejdet",
    bearbejdning: Some("Grænserne er forenklet og har kun navn og NUTS3-kode med."),
    note: None,
};

pub const OPENSTREETMAP: Kilde = Kilde {
    id: "openstreetmap",
    titel: "Baggrundskort",
    udgiver: "OpenStreetMap-bidragydere",
    url: "https://www.openstreetmap.org/copyright",
    licens: "ODbL",
    licens_url: Some("https://opendatacommons.org/licenses/odbl/"),
    kreditering: "© OpenStreetMap-bidragydere",
    bearbejdning: None,
    note: None,
};

pub const ESRI_WORLD_IMAGERY: Kilde = Kilde {
    id: "esri-world-imagery",
    titel: "Satellitbilleder",
    udgiver: "Esri",
    url: "https://goto.arcgisonline.com/maps/World_Imagery",
    licens: "Esris brugsvilkår",
    licens_url: Some("https://goto.arcgisonline.com/maps/World_Imagery"),
    kreditering: "Source: Esri, Vantor, Earthstar Geographics, and the GIS User Community",
    bearbejdning: None,
    note: Some("Krediteringen er tjenestens egen copyrightText, gengivet ordret."),
};

/// Rækkefølgen på `/kilder`.
pub const ALLE: [&Kilde; 5] = [
    &MARKER,
    &AFGROEDEKODER,
    &LANDSDELE,
    &OPENSTREETMAP,
    &ESRI_WORLD_IMAGERY,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_er_unikke() {
        let mut id: Vec<_> = ALLE.iter().map(|k| k.id).collect();
        id.sort_unstable();
        id.dedup();
        assert_eq!(id.len(), ALLE.len());
    }

    #[test]
    fn manglende_licens_er_forklaret() {
        for kilde in ALLE {
            if kilde.licens_url.is_none() {
                assert!(kilde.note.is_some(), "{} mangler en note", kilde.id);
            }
        }
    }
}
