//! Zuordnung Schicht → Bauleistung (Regel 81) und Artikel der Schicht
//! (Regel 82). Rein: Eingang sind Bauteilart, Schicht und Baustoff, Ausgang
//! die Bauleistung mit Grund; nichts wird geschrieben.

use crate::geld::Dez;
use crate::katalog::{Artikel, Einheit, Katalog, Leistung};
use sk_model::element::Category;
use sk_model::library::{MatCategory, Material};
use sk_model::{Guid, MaterialLayer};

/// Warum eine Schicht ihre Bauleistung hat (Regel 81, Stufen 1–5).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Grund {
    /// Stufe 1: `svc=` an der Schicht des Typs.
    Gewaehlt,
    /// Stufe 2: genau eine Regel passt.
    Regel,
    /// Stufe 2: mehrere passen, die spezifischste gilt.
    Mehrdeutig { auch: Vec<Guid> },
    /// Stufe 3: geschätzt nach der nächsten Dicke.
    Geschaetzt { nach: Guid },
    /// Stufe 4: Richtpreis des Baustoffs, nur Material.
    Richtpreis,
    /// Stufe 5 (und Luft): ohne Bauleistung.
    Ohne,
}

/// Ergebnis der Zuordnung einer Schicht.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Zuordnung {
    /// Bauleistung (auch bei `Geschaetzt`); `None` bei Richtpreis und ohne.
    pub leistung: Option<Guid>,
    pub grund: Grund,
}

impl Zuordnung {
    fn ohne() -> Zuordnung {
        Zuordnung {
            leistung: None,
            grund: Grund::Ohne,
        }
    }

    /// Zählt nur als Schätzung (Stufen 3 und 4): Warnung, nie ins LV.
    pub fn geschaetzt(&self) -> bool {
        matches!(self.grund, Grund::Geschaetzt { .. } | Grund::Richtpreis)
    }
}

/// Dicke in mm als Festkomma (Dateizahlen sind Festkomma, Mengen `f64`).
pub fn dicke(mm: f64) -> Dez {
    Dez((mm * Dez::SKALA as f64).round() as i64)
}

/// Wort der Bauteilart in `cats=` (`kinds.rs`).
pub fn kategorie_wort(c: Category) -> &'static str {
    sk_model::kinds::spec(c).szo
}

/// Richtpreis des Baustoffs mit Preiseinheit (Paket 5, Regel 78).
pub fn richtpreis(m: &Material) -> Option<(Dez, Einheit)> {
    use sk_model::element::PropValue;
    use sk_model::matprop::{price_unit, PRICE, PRICE_UNIT};
    let PropValue::Number(p) = m.props.get(PRICE)? else {
        return None;
    };
    let unit = match m.props.get(PRICE_UNIT) {
        Some(PropValue::Text(u)) => u.as_str(),
        _ => price_unit(m.category)?,
    };
    Some((
        Dez((p * Dez::SKALA as f64).round() as i64),
        Einheit::aus(unit)?,
    ))
}

/// Passt die Regel der Bauleistung auf die Schicht (Regel 81 Stufe 2)?
fn passt(l: &Leistung, kat: &str, mat: Guid, t: Dez, fun: &str) -> bool {
    !l.retired
        && l.kategorien.iter().any(|c| c == kat)
        && l.mat.is_none_or(|m| m == mat)
        && l.tmin.is_none_or(|v| t >= v)
        && l.tmax.is_none_or(|v| t <= v)
        && l.funktion.as_deref().is_none_or(|f| f == fun)
}

/// Rangfolge der Stufe 2: mehr Regelfelder, engere Dickenspanne, kleinere
/// Guid.
fn rang(a: &Leistung, b: &Leistung) -> std::cmp::Ordering {
    let spanne = |l: &Leistung| match (l.tmin, l.tmax) {
        (Some(x), Some(y)) => y.0 - x.0,
        _ => i64::MAX,
    };
    b.regelfelder()
        .cmp(&a.regelfelder())
        .then(spanne(a).cmp(&spanne(b)))
        .then(a.guid.cmp(&b.guid))
}

/// Abstand der Dicke `t` zum Band 0,75 × `tmin` … 1,25 × `tmax` der Regel
/// (Stufe 3); `None` außerhalb.
fn abstand(l: &Leistung, t: Dez) -> Option<i64> {
    if l.tmin.is_none() && l.tmax.is_none() {
        return None;
    }
    let lo = l.tmin.map_or(0, |v| v.0 * 3 / 4);
    let hi = l.tmax.map_or(i64::MAX, |v| v.0 + v.0 / 4);
    if t.0 < lo || t.0 > hi {
        return None;
    }
    Some(match (l.tmin, l.tmax) {
        (Some(v), _) if t < v => v.0 - t.0,
        (_, Some(v)) if t > v => t.0 - v.0,
        _ => 0,
    })
}

/// Bauleistung einer Schicht nach Regel 81. `material` ist der Baustoff der
/// Schicht (für Guid, Art und Richtpreis).
pub fn zuordnen(
    k: &Katalog,
    kat: Category,
    schicht: &MaterialLayer,
    material: Option<&Material>,
) -> Zuordnung {
    let Some(mat) = material else {
        return Zuordnung::ohne();
    };
    // Luft nie (K4)
    if mat.category == MatCategory::Air {
        return Zuordnung::ohne();
    }
    // Stufe 1: gewählt und gültig
    if let Some(g) = schicht.svc {
        if k.leistung(g).is_some_and(|l| !l.retired) {
            return Zuordnung {
                leistung: Some(g),
                grund: Grund::Gewaehlt,
            };
        }
    }
    let kw = kategorie_wort(kat);
    let t = dicke(schicht.thickness);
    let fun = sk_model::szo::layer_function(schicht.function);
    // Stufe 2: Regel, die spezifischste gewinnt
    let mut treffer: Vec<&Leistung> = k
        .leistungen
        .iter()
        .filter(|l| passt(l, kw, mat.guid, t, fun))
        .collect();
    if !treffer.is_empty() {
        treffer.sort_by(|a, b| rang(a, b));
        let g = treffer[0].guid;
        let grund = if treffer.len() == 1 {
            Grund::Regel
        } else {
            Grund::Mehrdeutig {
                auch: treffer[1..].iter().map(|l| l.guid).collect(),
            }
        };
        return Zuordnung {
            leistung: Some(g),
            grund,
        };
    }
    // Stufe 3: gleiche Bauteilart, gleicher Baustoff, gleiche Funktion;
    // Dicke im Band, kleinster Abstand, dann Rangfolge
    let mut nah: Vec<(i64, &Leistung)> = k
        .leistungen
        .iter()
        .filter(|l| {
            !l.retired
                && l.kategorien.iter().any(|c| c == kw)
                && l.mat == Some(mat.guid)
                && l.funktion.as_deref().is_none_or(|f| f == fun)
        })
        .filter_map(|l| abstand(l, t).map(|d| (d, l)))
        .collect();
    if !nah.is_empty() {
        nah.sort_by(|a, b| a.0.cmp(&b.0).then(rang(a.1, b.1)));
        return Zuordnung {
            leistung: Some(nah[0].1.guid),
            grund: Grund::Geschaetzt {
                nach: nah[0].1.guid,
            },
        };
    }
    // Stufe 4: Richtpreis, nur Material
    if richtpreis(mat).is_some() {
        return Zuordnung {
            leistung: None,
            grund: Grund::Richtpreis,
        };
    }
    Zuordnung::ohne()
}

/// Artikel der Schicht (Regel 82): Standardartikel mit der Dicke ± 1 mm,
/// sonst irgendein Artikel mit passender Dicke (kleinste Guid), sonst der
/// Standardartikel ohne Dicke. Ausgemustertes nur, wenn nichts anderes
/// passt. `None`: Richtpreis oder „Preis fehlt“ entscheidet der Aufrufer.
pub fn artikel_der_schicht(k: &Katalog, mat: Guid, t: Dez) -> Option<&Artikel> {
    let mm = Dez::SKALA;
    let dick = |a: &Artikel| a.t.is_some_and(|d| (d.0 - t.0).abs() <= mm);
    for ausgemustert in [false, true] {
        let alle = || {
            k.artikel
                .iter()
                .filter(move |a| a.mat == Some(mat) && a.retired == ausgemustert)
        };
        if let Some(a) = alle().filter(|a| a.std && dick(a)).min_by_key(|a| a.guid) {
            return Some(a);
        }
        if let Some(a) = alle().filter(|a| dick(a)).min_by_key(|a| a.guid) {
            return Some(a);
        }
        if let Some(a) = alle()
            .filter(|a| a.std && a.t.is_none())
            .min_by_key(|a| a.guid)
        {
            return Some(a);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{LayerFunction, Model};

    fn werk(m: &Model) -> Katalog {
        crate::lesen::werk(m)
    }

    fn schicht(m: &Model, name: &str, t: f64, f: LayerFunction) -> (MaterialLayer, Material) {
        let (id, mat) = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        (MaterialLayer::new(id, t, f), mat.clone())
    }

    fn kurz(k: &Katalog, z: &Zuordnung) -> String {
        z.leistung
            .and_then(|g| k.leistung(g))
            .map_or(String::new(), |l| l.kurz.clone())
    }

    /// Fall 2 und 3a (Abnahme 18, 26): genaueste Regel, nächste Dicke.
    #[test]
    fn stufen_der_zuordnung() {
        let m = Model::new();
        let k = werk(&m);
        let pb = |t| schicht(&m, "Porenbeton", t, LayerFunction::Structure);
        let (l, x) = pb(175.0);
        let z = zuordnen(&k, Category::ExteriorWall, &l, Some(&x));
        assert_eq!(z.grund, Grund::Regel);
        assert!(kurz(&k, &z).starts_with("AW Porenbeton-Planstein PP2-0,35 d=17,5cm"));
        // 20 cm: M10 (Abstand 20) vor M20 (Abstand 30)
        let (l, x) = pb(200.0);
        let z = zuordnen(&k, Category::ExteriorWall, &l, Some(&x));
        assert!(matches!(z.grund, Grund::Geschaetzt { .. }), "{z:?}");
        assert!(kurz(&k, &z).contains("d=17,5cm"));
        // 45 cm nach M30, 50 cm außerhalb jedes Bands
        let (l, x) = pb(450.0);
        let z = zuordnen(&k, Category::ExteriorWall, &l, Some(&x));
        assert!(kurz(&k, &z).contains("d=36,5cm"), "{z:?}");
        let (l, x) = pb(500.0);
        assert_eq!(
            zuordnen(&k, Category::ExteriorWall, &l, Some(&x)).grund,
            Grund::Ohne
        );
        // Mit Richtpreis: nur Material
        let mut x2 = x.clone();
        x2.props.insert(
            sk_model::matprop::PRICE.into(),
            sk_model::element::PropValue::Number(400.0),
        );
        assert_eq!(
            zuordnen(&k, Category::ExteriorWall, &l, Some(&x2)).grund,
            Grund::Richtpreis
        );
        assert_eq!(richtpreis(&x2), Some((Dez::ganz(400), Einheit::M3)));
        // Luft nie
        let luft = m
            .materials()
            .iter()
            .find(|(_, x)| x.category == MatCategory::Air)
            .map(|(id, x)| {
                (
                    MaterialLayer::new(id, 40.0, LayerFunction::AirGap),
                    x.clone(),
                )
            });
        if let Some((l, x)) = luft {
            assert_eq!(
                zuordnen(&k, Category::ExteriorWall, &l, Some(&x)).grund,
                Grund::Ohne
            );
        }
    }
}
