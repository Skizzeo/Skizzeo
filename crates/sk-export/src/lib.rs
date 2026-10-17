//! 3D-Ausgabe für Druck und Visualisierung: STL für den 3D-Druck, OBJ mit
//! MTL und GLB (glTF 2.0 binär) für Twinmotion, Lumion und andere
//! Visualisierungsprogramme (Export-Thread, Jörn 17.10.).
//!
//! Die Crate kennt weder Modell noch App. Die App gibt eine [`Szene`]: die
//! Gliederung des Baums als [`Knoten`], an jedem Knoten Netze aus
//! Dreiecken (mm, z nach oben, Projektkoordinaten zu ±0,00) mit einem
//! [`Material`]. Die Schreiber machen daraus Dateien; sie rechnen nichts am
//! Modell und ändern keine Mengen.

#![forbid(unsafe_code)]

pub mod glb;
mod json;
pub mod obj;
pub mod stl;

/// Ein Punkt oder eine Richtung (mm bzw. Einheitsvektor).
pub type P3 = [f64; 3];

/// Ein Material: Name und Farbe der Oberfläche (sRGB).
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub farbe: [u8; 3],
}

/// Ein Dreieck gegen den Uhrzeigersinn um seine Normale (Außenseite).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dreieck {
    pub p: [P3; 3],
    pub n: P3,
}

impl Dreieck {
    /// Dreieck mit der Normale aus seinem Umlauf; `None` ohne Fläche.
    pub fn aus_umlauf(p: [P3; 3]) -> Option<Dreieck> {
        let n = kreuz(sub(p[1], p[0]), sub(p[2], p[0]));
        let l = laenge(n);
        (l > 1e-9).then(|| Dreieck {
            p,
            n: [n[0] / l, n[1] / l, n[2] / l],
        })
    }
}

/// Dreiecke eines Materials.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Netz {
    /// Platz in [`Szene::materialien`].
    pub material: usize,
    pub dreiecke: Vec<Dreieck>,
}

/// Ein Knoten der Gliederung (Projekt, Gebäude, Geschoss, Bauteilart,
/// Bauteil, Gelände) mit eigenen Netzen und Unterknoten.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Knoten {
    pub name: String,
    pub netze: Vec<Netz>,
    pub kinder: Vec<Knoten>,
}

impl Knoten {
    pub fn neu(name: impl Into<String>) -> Knoten {
        Knoten {
            name: name.into(),
            ..Knoten::default()
        }
    }

    /// Dreiecke in diesem Knoten und darunter.
    pub fn dreiecke(&self) -> usize {
        self.netze.iter().map(|n| n.dreiecke.len()).sum::<usize>()
            + self.kinder.iter().map(Knoten::dreiecke).sum::<usize>()
    }

    /// Nimmt Netze ohne Dreieck und Knoten ohne Inhalt heraus; `false`, wenn
    /// der Knoten selbst danach leer ist.
    pub fn ausduennen(&mut self) -> bool {
        self.netze.retain(|n| !n.dreiecke.is_empty());
        self.kinder.retain_mut(Knoten::ausduennen);
        !self.netze.is_empty() || !self.kinder.is_empty()
    }

    /// Jeder Knoten mit Netzen und seinem Pfad ab der Wurzel, in
    /// Vorordnung.
    fn besuchen<'a>(&'a self, pfad: &mut Vec<&'a str>, f: &mut impl FnMut(&[&str], &'a Knoten)) {
        pfad.push(&self.name);
        if !self.netze.is_empty() {
            f(pfad, self);
        }
        for k in &self.kinder {
            k.besuchen(pfad, f);
        }
        pfad.pop();
    }
}

/// Was exportiert wird: Gliederung und Materialien.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Szene {
    /// Name der Wurzel (Projekt).
    pub name: String,
    pub materialien: Vec<Material>,
    pub knoten: Vec<Knoten>,
}

impl Szene {
    /// Alle Dreiecke.
    pub fn dreiecke(&self) -> usize {
        self.knoten.iter().map(Knoten::dreiecke).sum()
    }

    /// Nimmt leere Netze und Knoten heraus.
    pub fn ausduennen(&mut self) {
        self.knoten.retain_mut(Knoten::ausduennen);
    }

    /// Jeder Knoten mit Netzen und seinem Pfad unter der Wurzel, in
    /// Vorordnung.
    pub fn je_knoten<'a>(&'a self, mut f: impl FnMut(&[&str], &'a Knoten)) {
        let mut pfad = Vec::new();
        for k in &self.knoten {
            k.besuchen(&mut pfad, &mut f);
        }
    }

    /// Umschließender Quader aller Punkte (mm); `None` ohne Dreieck.
    pub fn rahmen(&self) -> Option<(P3, P3)> {
        let mut r: Option<(P3, P3)> = None;
        self.je_knoten(|_, k| {
            for p in k.netze.iter().flat_map(|n| &n.dreiecke).flat_map(|d| d.p) {
                let (lo, hi) = r.get_or_insert((p, p));
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        });
        r
    }
}

/// Welche Achse in der Datei nach oben zeigt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Oben {
    /// y nach oben (glTF, die meisten Visualisierungsprogramme).
    #[default]
    Y,
    /// z nach oben wie im Modell.
    Z,
}

/// Lage des Projektnullpunkts im amtlichen Lagesystem (ETRS89/UTM) wie
/// die Georeferenz im IFC (`IfcMapConversion`): Ost- und Nordwert und
/// Höhe von ±0,00 in m, Winkel der Modell-x-Achse im Gitter (rad, gegen
/// den Uhrzeigersinn von Ost).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lagesystem {
    pub ost: f64,
    pub nord: f64,
    pub hoehe: f64,
    pub drehung: f64,
}

/// Wie die Szene in einer Datei für die Visualisierung liegt: Einheit
/// Meter, Ursprung am Projektnullpunkt oder im Lagesystem.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lage {
    pub oben: Oben,
    pub lagesystem: Option<Lagesystem>,
}

impl Lage {
    /// Punkt im Modell (mm) als Punkt in der Datei (m).
    pub fn punkt(&self, p: P3) -> P3 {
        let m = [p[0] / 1000.0, p[1] / 1000.0, p[2] / 1000.0];
        let m = match self.lagesystem {
            Some(l) => {
                let (s, c) = l.drehung.sin_cos();
                [
                    l.ost + m[0] * c - m[1] * s,
                    l.nord + m[0] * s + m[1] * c,
                    l.hoehe + m[2],
                ]
            }
            None => m,
        };
        self.achse(m)
    }

    /// Richtung im Modell als Richtung in der Datei.
    pub fn richtung(&self, n: P3) -> P3 {
        let n = match self.lagesystem {
            Some(l) => {
                let (s, c) = l.drehung.sin_cos();
                [n[0] * c - n[1] * s, n[0] * s + n[1] * c, n[2]]
            }
            None => n,
        };
        self.achse(n)
    }

    /// z oben → Achse der Datei (rechtshändig: y oben heißt Nord nach −z).
    pub fn achse(&self, p: P3) -> P3 {
        match self.oben {
            Oben::Z => p,
            Oben::Y => [p[0], p[2], -p[1]],
        }
    }
}

/// Name für Formate, die Leerzeichen trennen (OBJ, MTL): Leerraum wird
/// zu `_`, leer zu `-`.
pub fn ohne_leerraum(s: &str) -> String {
    let t: String = s
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect();
    if t.is_empty() {
        "-".into()
    } else {
        t
    }
}

pub(crate) fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn kreuz(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn punkt(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn laenge(a: P3) -> f64 {
    punkt(a, a).sqrt()
}

#[cfg(test)]
pub(crate) mod probe {
    use super::*;

    /// Würfel mit Kantenlänge `a` (mm) ab `o`, nach außen gerichtet.
    pub fn wuerfel(o: P3, a: f64, material: usize) -> Netz {
        let p = |x: f64, y: f64, z: f64| [o[0] + x * a, o[1] + y * a, o[2] + z * a];
        let q = [
            [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)],
            [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)],
            [p(1., 1., 0.), p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.)],
            [p(0., 1., 0.), p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.)],
            [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)],
        ];
        let mut dreiecke = Vec::new();
        for [a, b, c, d] in q {
            dreiecke.extend(Dreieck::aus_umlauf([a, b, c]));
            dreiecke.extend(Dreieck::aus_umlauf([a, c, d]));
        }
        Netz { material, dreiecke }
    }

    /// Projekt → Gebäude 1 → EG → zwei Würfel (Wand, Decke), dazu das
    /// Gelände als flache Platte.
    pub fn szene() -> Szene {
        let mut eg = Knoten::neu("Erdgeschoss");
        let mut w = Knoten::neu("AW-001");
        w.netze.push(wuerfel([0.0, 0.0, 0.0], 1000.0, 0));
        let mut d = Knoten::neu("DE-001");
        d.netze.push(wuerfel([2000.0, 0.0, 0.0], 500.0, 1));
        eg.kinder = vec![w, d];
        let mut g = Knoten::neu("Gebäude 1");
        g.kinder.push(eg);
        let mut gel = Knoten::neu("Gelände");
        gel.netze
            .push(wuerfel([-5000.0, -5000.0, -2000.0], 1000.0, 2));
        Szene {
            name: "Haus".into(),
            materialien: vec![
                Material {
                    name: "Porenbeton".into(),
                    farbe: [230, 230, 220],
                },
                Material {
                    name: "Stahlbeton".into(),
                    farbe: [160, 160, 160],
                },
                Material {
                    name: "Rasen".into(),
                    farbe: [90, 140, 60],
                },
            ],
            knoten: vec![gel, g],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausduennen_nimmt_leere_aeste() {
        let mut s = probe::szene();
        s.knoten.push(Knoten::neu("leer"));
        s.knoten[1].kinder[0].kinder.push(Knoten::neu("Raum"));
        s.ausduennen();
        assert_eq!(s.knoten.len(), 2);
        assert_eq!(s.knoten[1].kinder[0].kinder.len(), 2);
        assert_eq!(s.dreiecke(), 36);
    }

    #[test]
    fn pfade_in_vorordnung() {
        let s = probe::szene();
        let mut v = Vec::new();
        s.je_knoten(|p, _| v.push(p.join("/")));
        assert_eq!(
            v,
            [
                "Gelände",
                "Gebäude 1/Erdgeschoss/AW-001",
                "Gebäude 1/Erdgeschoss/DE-001"
            ]
        );
    }

    #[test]
    fn lagesystem_wie_ifc_mapconversion() {
        // x-Achse 90° gedreht: Modell-x zeigt nach Nord
        let l = Lage {
            oben: Oben::Z,
            lagesystem: Some(Lagesystem {
                ost: 600_000.0,
                nord: 5_500_000.0,
                hoehe: 300.0,
                drehung: std::f64::consts::FRAC_PI_2,
            }),
        };
        let p = l.punkt([1000.0, 0.0, 2500.0]);
        assert!((p[0] - 600_000.0).abs() < 1e-9);
        assert!((p[1] - 5_500_001.0).abs() < 1e-9);
        assert!((p[2] - 302.5).abs() < 1e-9);
    }

    #[test]
    fn y_oben_rechtshaendig() {
        let l = Lage::default();
        assert_eq!(l.punkt([1000.0, 2000.0, 3000.0]), [1.0, 3.0, -2.0]);
        // Normale bleibt Einheitsvektor, Umlauf bleibt außen
        let a = l.achse([1.0, 0.0, 0.0]);
        let b = l.achse([0.0, 1.0, 0.0]);
        assert_eq!(kreuz(a, b), l.achse([0.0, 0.0, 1.0]));
    }
}
