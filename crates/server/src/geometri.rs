//! Om et punkt ligger inde i en mark fra en GeoPackage.
//!
//! Serveren har ingen geometrifunktioner og skal kun kunne én ting: afgøre
//! hvilke marker et klik på kortet ligger i. R-træet finder de marker hvis
//! udstrækning rammer punktet, og her afgøres det, om punktet ligger inde i
//! selve fladen.
//!
//! En GeoPackage gemmer geometrien som et kort hoved efterfulgt af WKB.
//! Kun flader i to dimensioner (Polygon og MultiPolygon) kendes; det er hvad
//! pipelinen skriver.

/// Om punktet (`x`, `y`) ligger inde i fladen i `gpkg`, i fladens egne
/// koordinater. `None` hvis geometrien ikke er en flade denne kode kender.
///
/// Punktet ligger inde, hvis en stråle fra det krydser fladens kanter et
/// ulige antal gange. Det gælder også for huller og for flere flader i én
/// geometri, så alle ringe tælles med under ét.
pub fn indeholder(gpkg: &[u8], x: f64, y: f64) -> Option<bool> {
    let mut wkb = Wkb::new(wkb_i(gpkg)?);
    let mut krydsninger = 0;
    wkb.flade(&mut |ring| krydsninger += krydser(ring, x, y))?;
    Some(krydsninger % 2 == 1)
}

/// WKB'en efter GeoPackage-hovedet: "GP", version, flag, SRS-id (4 byte) og
/// en udstrækning hvis størrelse flaget angiver.
fn wkb_i(gpkg: &[u8]) -> Option<&[u8]> {
    if gpkg.get(..2)? != b"GP" {
        return None;
    }
    let flag = *gpkg.get(3)?;
    let udstraekning = match (flag >> 1) & 0b111 {
        0 => 0,
        1 => 32,
        2 | 3 => 48,
        4 => 64,
        _ => return None,
    };
    gpkg.get(8 + udstraekning..)
}

/// Hvor mange gange en vandret stråle mod højre fra (`x`, `y`) krydser
/// ringens kanter. En kant tæller, når den ene ende ligger over punktet og
/// den anden ikke gør, så et hjørne lige i højde med punktet kun tælles én
/// gang.
fn krydser(ring: &[[f64; 2]], x: f64, y: f64) -> u32 {
    let mut antal = 0;
    for kant in ring.windows(2) {
        let [[x1, y1], [x2, y2]] = [kant[0], kant[1]];
        if (y1 > y) != (y2 > y) && x < x1 + (y - y1) * (x2 - x1) / (y2 - y1) {
            antal += 1;
        }
    }
    antal
}

struct Wkb<'a> {
    data: &'a [u8],
    pos: usize,
    lille_endian: bool,
}

impl<'a> Wkb<'a> {
    fn new(data: &'a [u8]) -> Self {
        Wkb {
            data,
            pos: 0,
            lille_endian: true,
        }
    }

    /// Læser én geometri og giver hver af dens ringe til `ring`.
    fn flade(&mut self, ring: &mut impl FnMut(&[[f64; 2]])) -> Option<()> {
        self.lille_endian = match self.byte()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        match self.u32()? {
            3 => self.polygon(ring),
            6 => {
                for _ in 0..self.u32()? {
                    self.flade(ring)?;
                }
                Some(())
            }
            _ => None,
        }
    }

    fn polygon(&mut self, ring: &mut impl FnMut(&[[f64; 2]])) -> Option<()> {
        for _ in 0..self.u32()? {
            let antal = self.u32()? as usize;
            // Et antal punkter der ikke er plads til, er en ødelagt
            // geometri, ikke en grund til at reservere hukommelse.
            if antal > (self.data.len() - self.pos) / 16 {
                return None;
            }
            let mut punkter = Vec::with_capacity(antal);
            for _ in 0..antal {
                punkter.push([self.f64()?, self.f64()?]);
            }
            ring(&punkter);
        }
        Some(())
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let bytes = self.data.get(self.pos..self.pos + N)?.try_into().ok()?;
        self.pos += N;
        Some(bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.bytes()?;
        Some(if self.lille_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn f64(&mut self) -> Option<f64> {
        let bytes = self.bytes()?;
        Some(if self.lille_endian {
            f64::from_le_bytes(bytes)
        } else {
            f64::from_be_bytes(bytes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// En ring i WKB, i den angivne byterækkefølge.
    fn ring(punkter: &[[f64; 2]], lille: bool) -> Vec<u8> {
        let mut wkb = tal(punkter.len() as u32, lille);
        for [x, y] in punkter {
            for v in [x, y] {
                wkb.extend(if lille {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                });
            }
        }
        wkb
    }

    fn tal(v: u32, lille: bool) -> Vec<u8> {
        if lille {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
        .to_vec()
    }

    fn polygon(ringe: &[&[[f64; 2]]], lille: bool) -> Vec<u8> {
        let mut wkb = vec![u8::from(lille)];
        wkb.extend(tal(3, lille));
        wkb.extend(tal(ringe.len() as u32, lille));
        for r in ringe {
            wkb.extend(ring(r, lille));
        }
        wkb
    }

    fn multipolygon(flader: &[Vec<u8>]) -> Vec<u8> {
        let mut wkb = vec![1];
        wkb.extend(tal(6, true));
        wkb.extend(tal(flader.len() as u32, true));
        for flade in flader {
            wkb.extend(flade);
        }
        wkb
    }

    /// GeoPackage-hoved med en udstrækning på 32 byte, som GDAL skriver.
    fn gpkg(wkb: &[u8]) -> Vec<u8> {
        let mut blob = vec![b'G', b'P', 0, 0b0000_0011];
        blob.extend(4326u32.to_le_bytes());
        blob.extend([0; 32]);
        blob.extend(wkb);
        blob
    }

    const YDRE: [[f64; 2]; 5] = [
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [0.0, 0.0],
    ];
    const HUL: [[f64; 2]; 5] = [[4.0, 4.0], [6.0, 4.0], [6.0, 6.0], [4.0, 6.0], [4.0, 4.0]];

    #[test]
    fn punkt_i_flade_men_ikke_i_hullet() {
        let blob = gpkg(&polygon(&[&YDRE, &HUL], true));
        assert_eq!(indeholder(&blob, 2.0, 2.0), Some(true));
        assert_eq!(indeholder(&blob, 5.0, 5.0), Some(false));
        assert_eq!(indeholder(&blob, 11.0, 5.0), Some(false));
        assert_eq!(indeholder(&blob, -1.0, 5.0), Some(false));
    }

    #[test]
    fn hjoerne_i_hoejde_med_punktet_taeller_en_gang() {
        // En trekant med spidsen lige ud for punktet.
        let trekant = [[0.0, 0.0], [10.0, 5.0], [0.0, 10.0], [0.0, 0.0]];
        let blob = gpkg(&polygon(&[&trekant], true));
        assert_eq!(indeholder(&blob, 2.0, 5.0), Some(true));
        assert_eq!(indeholder(&blob, 12.0, 5.0), Some(false));
    }

    #[test]
    fn multipolygon_rammer_hver_af_fladerne() {
        let anden = [[20.0, 0.0], [30.0, 0.0], [30.0, 10.0], [20.0, 0.0]];
        let blob = gpkg(&multipolygon(&[
            polygon(&[&YDRE], true),
            polygon(&[&anden], false),
        ]));
        assert_eq!(indeholder(&blob, 5.0, 5.0), Some(true));
        assert_eq!(indeholder(&blob, 28.0, 2.0), Some(true));
        assert_eq!(indeholder(&blob, 15.0, 5.0), Some(false));
    }

    #[test]
    fn hovedet_kan_vaere_uden_udstraekning() {
        let mut blob = vec![b'G', b'P', 0, 0b0000_0001];
        blob.extend(4326u32.to_le_bytes());
        blob.extend(polygon(&[&YDRE], true));
        assert_eq!(indeholder(&blob, 5.0, 5.0), Some(true));
    }

    #[test]
    fn andet_end_flader_kendes_ikke() {
        let mut punkt = vec![1];
        punkt.extend(tal(1, true));
        punkt.extend(1.0f64.to_le_bytes());
        punkt.extend(1.0f64.to_le_bytes());
        assert_eq!(indeholder(&gpkg(&punkt), 1.0, 1.0), None);
        assert_eq!(indeholder(b"ikke en geometri", 1.0, 1.0), None);
        // En ring der lover flere punkter end der er plads til.
        let mut kort = polygon(&[&YDRE], true);
        kort.truncate(kort.len() - 8);
        assert_eq!(indeholder(&gpkg(&kort), 5.0, 5.0), None);
    }
}
