//! Fælles viden for pipeline og server: navnene på datafilerne,
//! afgrødegrupperne og datakilderne med deres vilkår.

pub mod gruppe;
pub mod kilder;

/// GeoPackage med marker, landsdele og opslagstabeller.
pub const DATABASE_FIL: &str = "markkort.gpkg";

/// Vektortiles med markernes geometri, gruppe og landsdel.
pub const TILES_FIL: &str = "marker.mbtiles";

/// Lagnavnet inde i vektortiles'ene.
pub const TILES_LAG: &str = "marker";
