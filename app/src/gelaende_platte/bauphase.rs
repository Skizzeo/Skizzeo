//! Bauphasen in 3D und Schnitt (Jörn 18.10., „zu 3: a“, Vorschlag
//! `gelaende/bauphasen-vorschlag.md`): dasselbe Modell vom Urgelände bis zum
//! fertigen Grundstück in acht Bildern. Die Phase ist nur Ansicht: Sie
//! steht nicht in der Datei, ändert keine Menge und gilt für 3D und Schnitt
//! zugleich; Grundriss und Ansichten zeigen immer den fertigen Stand.
//! Gewählt wird sie an der Phasenleiste ([`crate::gelaende_platte::phasenleiste`]).
//!
//! Hier, was eine Phase zeigt: welche Bauteile des Gebäudes
//! ([`Bauphase::verborgen`]), welche Schichten des Bodenaufbaus
//! ([`super::bodenaufbau`]) und welche Menge aus dem LV dazu gehört
//! ([`Bauphase::menge`]); dazu das Zuschneiden des Geländes um die Häuser,
//! wo in 3D die Baugrube an seiner Stelle steht ([`ausserhalb`]).

use crate::selection::de;
use sk_math::{vec3, Vec3};
use sk_model::qto_earth::{EXCAVATION, GRAVEL, SAND, TOPSOIL, TRENCH, TRENCH_SAND};
use sk_model::Category;
use std::collections::BTreeSet;

/// Eine der acht Phasen; ohne Wahl der fertige Stand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Bauphase {
    Bestand,
    Mutterboden,
    Ausgekoffert,
    Fuellsand,
    Graeben,
    Gruendung,
    Rohbau,
    #[default]
    Fertig,
}

/// Die Bauteile der Gründung (Phase 6).
const GRUENDUNG: [Category; 3] = [
    Category::GroundSlab,
    Category::StripFooting,
    Category::PerimeterInsulation,
];

impl Bauphase {
    pub(crate) const ALLE: [Bauphase; 8] = [
        Bauphase::Bestand,
        Bauphase::Mutterboden,
        Bauphase::Ausgekoffert,
        Bauphase::Fuellsand,
        Bauphase::Graeben,
        Bauphase::Gruendung,
        Bauphase::Rohbau,
        Bauphase::Fertig,
    ];

    /// Nummer 1 bis 8.
    pub(crate) fn nr(self) -> u8 {
        self as u8 + 1
    }

    /// Name an der Leiste, z. B. „Füllsand“.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Bauphase::Bestand => "Bestand",
            Bauphase::Mutterboden => "Mutterboden ab",
            Bauphase::Ausgekoffert => "Ausgekoffert",
            Bauphase::Fuellsand => "Füllsand",
            Bauphase::Graeben => "Gräben",
            Bauphase::Gruendung => "Gründung",
            Bauphase::Rohbau => "Rohbau",
            Bauphase::Fertig => "Fertig",
        }
    }

    /// Tooltip am Feld der Phase: was zu sehen ist.
    pub(crate) fn tip(self) -> &'static str {
        match self {
            Bauphase::Bestand => "1 Bestand: Urgelände mit Mutterboden, noch kein Haus",
            Bauphase::Mutterboden => "2 Mutterboden ab: im Baufeld abgeschoben",
            Bauphase::Ausgekoffert => {
                "3 Ausgekoffert: 0,80 m unter OK Gelände, Rand in der Böschung"
            }
            Bauphase::Fuellsand => {
                "4 Füllsand: Sandplatte bis UK Dämmung, 0,50 m über die Schürze hinaus"
            }
            Bauphase::Graeben => "5 Gräben: Fundamentgräben im Füllsand, innen Kies",
            Bauphase::Gruendung => "6 Gründung: Frostschürze, Dämmung und Sohlplatte",
            Bauphase::Rohbau => "7 Rohbau: das Haus, Arbeitsraum noch offen",
            Bauphase::Fertig => "8 Fertig: verfüllt, Mutterboden angedeckt (Esc)",
        }
    }

    /// Die Phase `d` Schritte weiter (negativ: zurück), höchstens bis zur
    /// ersten bzw. letzten.
    pub(crate) fn schritt(self, d: i32) -> Bauphase {
        let i = (self as i32 + d).clamp(0, 7) as usize;
        Bauphase::ALLE[i]
    }

    /// Steht der Bodenaufbau so da wie gebaut, ohne Verfüllung und ohne
    /// angedeckten Oberboden? Bis zum Rohbau.
    pub(crate) fn offen(self) -> bool {
        self != Bauphase::Fertig
    }

    /// Arten des Gebäudes, die die Phase ausblendet: bis zu den Gräben
    /// alle, in der Gründung alle bis auf Platte, Schürze und Dämmung,
    /// danach keine.
    pub(crate) fn verborgen(self) -> BTreeSet<Category> {
        let alle = Category::ALL.into_iter().chain([Category::Extension]);
        match self {
            p if p < Bauphase::Gruendung => alle.collect(),
            Bauphase::Gruendung => alle.filter(|c| !GRUENDUNG.contains(c)).collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Zeile aus dem LV unter der Leiste, z. B. „Füllsand einbauen
    /// 64,1 m³“, aus den Automatikmengen (Schlüssel, mm³); `None` in den
    /// Phasen ohne Erdarbeit oder ohne Menge.
    pub(crate) fn menge(self, auto: &[(&str, f64)]) -> Option<String> {
        let teile: &[(&str, &[&str])] = match self {
            Bauphase::Mutterboden => &[("Oberboden abtragen", &[TOPSOIL])],
            Bauphase::Ausgekoffert => &[("Auskoffern", &[EXCAVATION])],
            Bauphase::Fuellsand => &[("Füllsand einbauen", &[SAND])],
            Bauphase::Graeben => &[
                ("Gräben ausheben", &[TRENCH_SAND, TRENCH]),
                ("Kies", &[GRAVEL]),
            ],
            _ => &[],
        };
        let summe = |keys: &[&str]| -> f64 {
            auto.iter()
                .filter(|a| keys.contains(&a.0))
                .map(|a| a.1)
                .sum()
        };
        let text: Vec<String> = teile
            .iter()
            .map(|(name, keys)| (name, summe(keys) / 1e9))
            .filter(|(_, v)| *v >= 0.05)
            .map(|(name, v)| format!("{name} {} m³", de(v, 1)))
            .collect();
        (!text.is_empty()).then(|| text.join(" · "))
    }
}

/// Liegt `q` im Feld `r` (x0, y0, x1, y1)?
pub(crate) fn im_feld(r: &[f64; 4], q: [f64; 2]) -> bool {
    q[0] > r[0] && q[0] < r[2] && q[1] > r[1] && q[1] < r[3]
}

/// Die Felder, die sich überlappen, zu einem (die Baugrube rechnet je Feld
/// ein Raster, zwei überlappende Felder stünden doppelt da).
pub(crate) fn vereint(mut felder: Vec<[f64; 4]>) -> Vec<[f64; 4]> {
    loop {
        let mut neu = false;
        let mut out: Vec<[f64; 4]> = Vec::new();
        for f in felder {
            match out
                .iter_mut()
                .find(|o| f[0] < o[2] && o[0] < f[2] && f[1] < o[3] && o[1] < f[3])
            {
                Some(o) => {
                    *o = [
                        o[0].min(f[0]),
                        o[1].min(f[1]),
                        o[2].max(f[2]),
                        o[3].max(f[3]),
                    ];
                    neu = true;
                }
                None => out.push(f),
            }
        }
        felder = out;
        if !neu {
            return felder;
        }
    }
}

/// Die Hälfte des konvexen Vielecks `p` mit Koordinate `achse` (0: x, 1: y)
/// unter `c` (`unter`) bzw. darüber; die Höhe folgt der Kante.
fn halb(p: &[Vec3], achse: usize, c: f64, unter: bool) -> Vec<Vec3> {
    let w = |v: &Vec3| if achse == 0 { v.x } else { v.y };
    let drin = |v: &Vec3| (w(v) <= c) == unter;
    let mut out = Vec::new();
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        if drin(&a) {
            out.push(a);
        }
        if drin(&a) != drin(&b) {
            let t = (c - w(&a)) / (w(&b) - w(&a));
            out.push(a + (b - a) * t);
        }
    }
    out
}

/// Die Teile des konvexen, ebenen Vielecks `p` außerhalb aller `felder`,
/// jedes konvex; dort steht in den offenen Phasen die Baugrube.
pub(crate) fn ausserhalb(p: &[Vec3], felder: &[[f64; 4]]) -> Vec<Vec<Vec3>> {
    let mut teile = vec![p.to_vec()];
    for r in felder {
        let mut neu = Vec::new();
        for t in teile {
            let (lo, hi) = t.iter().fold(
                (vec3(f64::MAX, f64::MAX, 0.0), vec3(f64::MIN, f64::MIN, 0.0)),
                |(lo, hi), v| {
                    (
                        vec3(lo.x.min(v.x), lo.y.min(v.y), 0.0),
                        vec3(hi.x.max(v.x), hi.y.max(v.y), 0.0),
                    )
                },
            );
            if hi.x <= r[0] || lo.x >= r[2] || hi.y <= r[1] || lo.y >= r[3] {
                neu.push(t);
                continue;
            }
            // je Seite des Felds das Stück davor ab, der Rest liegt im Feld
            let mut rest = t;
            for (achse, c, unter) in [
                (0, r[0], true),
                (0, r[2], false),
                (1, r[1], true),
                (1, r[3], false),
            ] {
                let aussen = halb(&rest, achse, c, unter);
                if aussen.len() >= 3 {
                    neu.push(aussen);
                }
                rest = halb(&rest, achse, c, !unter);
                if rest.len() < 3 {
                    break;
                }
            }
        }
        teile = neu;
    }
    teile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flaeche(p: &[Vec3]) -> f64 {
        sk_math::polygon::signed_area(p)
    }

    /// Acht Phasen in der Reihenfolge, Schritte bleiben in 1 bis 8, ohne
    /// Wahl der fertige Stand; bis zu den Gräben ist kein Bauteil zu sehen,
    /// in der Gründung nur Platte, Schürze und Dämmung.
    #[test]
    fn phasen() {
        assert_eq!(Bauphase::default(), Bauphase::Fertig);
        let nr: Vec<u8> = Bauphase::ALLE.iter().map(|p| p.nr()).collect();
        assert_eq!(nr, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Bauphase::Fertig.schritt(1), Bauphase::Fertig);
        assert_eq!(Bauphase::Bestand.schritt(-1), Bauphase::Bestand);
        assert_eq!(Bauphase::Fuellsand.schritt(-2), Bauphase::Mutterboden);
        for p in Bauphase::ALLE {
            assert!(p.tip().starts_with(&format!("{} {}", p.nr(), p.name())));
        }
        let alle = Category::ALL.len() + 1;
        assert_eq!(Bauphase::Graeben.verborgen().len(), alle);
        let g = Bauphase::Gruendung.verborgen();
        assert_eq!(g.len(), alle - 3);
        assert!(!g.contains(&Category::StripFooting) && g.contains(&Category::ExteriorWall));
        assert!(Bauphase::Rohbau.verborgen().is_empty() && Bauphase::Fertig.verborgen().is_empty());
    }

    /// Die Zeile aus dem LV summiert die Schlüssel der Phase über alle
    /// Häuser; Phasen ohne Erdarbeit und Mengen unter 0,05 m³ zeigen keine.
    #[test]
    fn mengenzeile() {
        let a = |key: &'static str, m3: f64| (key, m3 * 1e9);
        let auto = [
            a(SAND, 40.0),
            a(SAND, 24.14),
            a(TRENCH_SAND, 9.4),
            a(TRENCH, 2.79),
            a(GRAVEL, 0.01),
        ];
        assert_eq!(
            Bauphase::Fuellsand.menge(&auto).as_deref(),
            Some("Füllsand einbauen 64,1 m³")
        );
        assert_eq!(
            Bauphase::Graeben.menge(&auto).as_deref(),
            Some("Gräben ausheben 12,2 m³")
        );
        assert_eq!(Bauphase::Mutterboden.menge(&auto), None);
        for p in [
            Bauphase::Bestand,
            Bauphase::Gruendung,
            Bauphase::Rohbau,
            Bauphase::Fertig,
        ] {
            assert_eq!(p.menge(&auto), None);
        }
    }

    /// Zugeschnitten bleibt außerhalb der Felder genau die Fläche, die nicht
    /// in ihnen liegt; überlappende Felder werden eins.
    #[test]
    fn zuschnitt() {
        let q = [
            vec3(0.0, 0.0, 5.0),
            vec3(10.0, 0.0, 5.0),
            vec3(10.0, 10.0, 5.0),
            vec3(0.0, 10.0, 5.0),
        ];
        let felder = vereint(vec![
            [2.0, 2.0, 5.0, 5.0],
            [4.0, 4.0, 6.0, 6.0],
            [20.0, 0.0, 30.0, 1.0],
        ]);
        assert_eq!(felder, [[2.0, 2.0, 6.0, 6.0], [20.0, 0.0, 30.0, 1.0]]);
        let teile = ausserhalb(&q, &felder);
        let summe: f64 = teile.iter().map(|t| flaeche(t)).sum();
        assert!((summe - 84.0).abs() < 1e-9, "{summe}");
        for t in &teile {
            assert!(t.iter().all(|v| v.z == 5.0));
            let m = t.iter().fold(vec3(0.0, 0.0, 0.0), |s, v| s + *v) * (1.0 / t.len() as f64);
            assert!(!im_feld(&felder[0], [m.x, m.y]));
        }
        // ganz drin: nichts, ganz daneben: unverändert
        assert!(ausserhalb(&q, &[[-1.0, -1.0, 11.0, 11.0]]).is_empty());
        assert_eq!(ausserhalb(&q, &[[20.0, 0.0, 30.0, 1.0]]), vec![q.to_vec()]);
    }
}
