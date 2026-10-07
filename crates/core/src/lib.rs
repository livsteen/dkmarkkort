//! Fælles viden for pipeline og server: navnene på datafilerne,
//! afgrødegrupperne og datakilderne med deres vilkår.

pub mod gruppe;
pub mod kilder;

/// GeoPackage med marker, landsdele og opslagstabeller.
pub const DATABASE_FIL: &str = "markkort.gpkg";

/// Vektortiles med markernes geometri, gruppe og landsdel.
pub const TILES_FIL: &str = "marker.mbtiles";

/// Rastertiles med markerne som ét billede til kortet zoomet ud. Hver pixel
/// er en [`gruppe::Gruppe::nr`] gange 16 plus landsdelens nummer, eller 0
/// hvor der ingen mark er.
pub const OVERBLIK_FIL: &str = "overblik.mbtiles";

/// GeoPackage med de sprøjtede marker for hver planperiode, hvad der er
/// sprøjtet på dem, og midlerne.
pub const SPROEJTNING_DATABASE_FIL: &str = "sproejtning.gpkg";

/// Vektortiles med de sprøjtede marker i planperioden der begynder i `aar`,
/// med id og belastning pr. hektar. Én fil pr. planperiode, fordi markerne
/// er forskellige fra år til år og ikke kan dele tiles.
pub fn sproejtning_tiles_fil(aar: u16) -> String {
    format!("sproejtning-{aar}.mbtiles")
}

/// Skrives af pipelinen, når alle filerne ovenfor er på plads, og
/// indeholder tidspunktet. En server der holder øje med den, åbner aldrig en
/// blanding af gamle og nye filer.
pub const BYGGET_FIL: &str = "bygget";

/// Skrives af pipeline-containeren med tidspunktet, når pipelinen fejler, og
/// slettes igen, når den lykkes. Så kan serveren skelne en fejl fra en
/// pipeline, der stadig arbejder.
pub const FEJLET_FIL: &str = "fejlet";

/// CVR-nummeret som pipelinen giver de marker, der er indberettet uden et.
/// Det er ikke en rigtig bedrift, men gør markerne søgbare samlet ét sted.
/// Marknumrene kan gå igen under det, så en mark kendes altid på sit id.
pub const UDEN_CVR: &str = "00000000";

/// Lagnavnet inde i vektortiles'ene.
pub const TILES_LAG: &str = "marker";

/// Lagnavnet inde i sprøjtningens vektortiles.
pub const SPROEJTNING_LAG: &str = "sproejtning";

/// Sprøjtejournalernes koder for mængder i kg og i liter. Et middels
/// belastning er opgjort pr. kg eller liter, så kun mængder i de to enheder
/// kan regnes om til belastning.
pub const ENHED_KG: i64 = 2;
pub const ENHED_LITER: i64 = 4;

/// Planperioden der begynder 1. august i `aar` og slutter 31. juli året
/// efter, skrevet som 2024/25.
pub fn planperiode(aar: u16) -> String {
    format!("{aar}/{:02}", (aar + 1) % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planperioden_skrives_med_begge_aar() {
        assert_eq!(planperiode(2024), "2024/25");
        assert_eq!(planperiode(2009), "2009/10");
        assert_eq!(planperiode(2099), "2099/00");
    }

    #[test]
    fn hver_planperiode_har_sin_egen_tilesfil() {
        assert_eq!(sproejtning_tiles_fil(2024), "sproejtning-2024.mbtiles");
        assert_ne!(sproejtning_tiles_fil(2023), sproejtning_tiles_fil(2024));
    }
}
