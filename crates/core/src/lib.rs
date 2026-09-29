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

/// Lagnavnet inde i vektortiles'ene.
pub const TILES_LAG: &str = "marker";
