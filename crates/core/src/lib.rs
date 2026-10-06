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

/// Skrives af pipelinen, når de tre filer ovenfor alle er på plads, og
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
