//! Werkzeug „Erweiterungen“ (Schrittplan E6, Vertrag §14): setzt ein
//! Erweiterungsbauteil als Punkt, Linie oder Rechteck im aktiven Geschoss.
//! Die Eingabe (Fangen, Gummiband, Zahl + Enter, R) läuft im
//! [`WallTool`](crate::wall_tool::WallTool); hier steht, was ein Bauteil
//! daraus macht, und die Bibliothek der eingelesenen Definitionen.
//!
//! - punkt: Klick setzt das Bauteil. Zahl + Enter setzt es genau im
//!   Abstand zum zuletzt gesetzten. Tab dreht um 90° (`drehen=ja`).
//! - linie: Klick setzt Anfang und Ende; Zahl + Enter die Länge, R den
//!   90°-Sprung. Die Länge geht in den Parameter `[bedienung] laenge`.
//! - rechteck: Klick setzt zwei Ecken; Zahl + Enter die Breite, Tab die
//!   Tiefe (`[bedienung] breite`, `tiefe`).

use sk_math::Vec3;
use sk_model::erweiterung::{Ergebnis, ExtDef, ExtFeld, ExtPart, Geschoss};

/// Wie ein Bauteil gesetzt wird (`[bedienung] einfuegen`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Punkt,
    Linie,
    Rechteck,
}

impl Art {
    pub fn von(def: &ExtDef) -> Art {
        match def.einfuegen() {
            "linie" => Art::Linie,
            "rechteck" => Art::Rechteck,
            _ => Art::Punkt,
        }
    }

    /// Klicks bis zum fertigen Bauteil.
    pub fn klicks(self) -> usize {
        match self {
            Art::Punkt => 1,
            _ => 2,
        }
    }

    /// Felder der Pille bei Zahl + Enter.
    pub fn labels(self) -> [&'static str; 2] {
        match self {
            Art::Punkt => ["Abstand", "Winkel"],
            Art::Linie => ["Länge", "Winkel"],
            Art::Rechteck => ["Breite", "Tiefe"],
        }
    }

    /// Die drei festen Bedienhinweise (Vertrag §14).
    pub fn hinweise(self) -> [&'static str; 3] {
        match self {
            Art::Punkt => [
                "Klick setzt das Bauteil.",
                "Zahl + Enter setzt genau.",
                "Tab: drehen, Esc: zu",
            ],
            Art::Linie => [
                "Klick setzt Anfang und Ende.",
                "Zahl + Enter setzt die Länge.",
                "R: 90°-Sprung, Esc: zu",
            ],
            Art::Rechteck => [
                "Klick setzt zwei Ecken.",
                "Zahl + Enter: Breite, Tab: Tiefe",
                "Esc: zu",
            ],
        }
    }
}

/// Ein laufendes Werkzeug „Erweiterungen“.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtModus {
    pub def: ExtDef,
    /// Typ und eigene Werte der nächsten Exemplare (Paneel).
    pub vorlage: ExtPart,
    pub art: Art,
    /// Drehung beim Setzen eines Punkts (Tab), Grad gegen den Uhrzeigersinn.
    pub rot: f64,
    /// Zuletzt gesetzter Punkt (Bezug für Zahl + Enter bei `punkt`).
    pub zuletzt: Option<Vec3>,
    /// Letzte Rechnung der Vorschau.
    vorschau: Vorschau,
}

/// Rechnung der Vorschau nach Typ, Werten und Geschoss (Review 3ci
/// Hinweis a): Lage und Drehung gehen nicht in die Rechnung ein, beim
/// Kameradrehen und beim Ziehen eines Punkts bleibt sie gleich. Zählt beim
/// Vergleich der Werkzeuge nicht mit, eine Kopie beginnt leer.
#[derive(Debug, Default)]
struct Vorschau(std::cell::RefCell<Option<(ExtPart, Geschoss, Ergebnis)>>);

impl Clone for Vorschau {
    fn clone(&self) -> Vorschau {
        Vorschau::default()
    }
}

impl PartialEq for Vorschau {
    fn eq(&self, _: &Vorschau) -> bool {
        true
    }
}

/// Höchstens so viele Felder im Werkzeug-Paneel (Vertrag §14).
pub const MAX_FELDER: usize = 4;

impl ExtModus {
    pub fn new(def: ExtDef) -> ExtModus {
        let vorlage = ExtPart::new(&def, [0.0, 0.0]);
        ExtModus {
            art: Art::von(&def),
            vorlage,
            def,
            rot: 0.0,
            zuletzt: None,
            vorschau: Vorschau::default(),
        }
    }

    /// Körper und Mengen von `t` im Geschoss `g`, für die Vorschau gemerkt.
    pub fn rechnen(&self, t: &ExtPart, g: &Geschoss) -> Ergebnis {
        let mut schluessel = t.clone();
        schluessel.at = [0.0, 0.0];
        schluessel.rot = 0.0;
        let mut c = self.vorschau.0.borrow_mut();
        if let Some((k, kg, e)) = c.as_ref() {
            if *k == schluessel && kg == g {
                return e.clone();
            }
        }
        let e = sk_model::erweiterung::rechnen(&self.def, t, g);
        *c = Some((schluessel, *g, e.clone()));
        e
    }

    /// Der Parameter, den die Eingabe setzt (`laenge`, `breite`, `tiefe`).
    fn eingabe(&self, k: &str) -> Option<&str> {
        self.def.bedienung(k)
    }

    /// Exemplar aus den gesetzten Punkten (Arbeitsebene): Punkt ein Punkt,
    /// Linie und Rechteck zwei. `None`, wenn es so kein Bauteil gibt
    /// (Linie unter 1 mm, Rechteck ohne Fläche).
    pub fn teil(&self, p: &[Vec3]) -> Option<ExtPart> {
        let mut t = self.vorlage.clone();
        match (self.art, p) {
            (Art::Punkt, [a, ..]) => {
                t.at = [a.x, a.y];
                t.rot = self.rot;
            }
            (Art::Linie, [a, b, ..]) => {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l = dx.hypot(dy);
                if l < 1.0 {
                    return None;
                }
                t.at = [a.x, a.y];
                t.rot = dy.atan2(dx).to_degrees();
                if let Some(k) = self.eingabe("laenge") {
                    t.set(k, l.round());
                }
            }
            (Art::Rechteck, [a, b, ..]) => {
                let (bx, ty) = ((b.x - a.x).abs(), (b.y - a.y).abs());
                if bx < 1.0 || ty < 1.0 {
                    return None;
                }
                t.at = [a.x.min(b.x), a.y.min(b.y)];
                t.rot = 0.0;
                if let Some(k) = self.eingabe("breite") {
                    t.set(k, bx.round());
                }
                if let Some(k) = self.eingabe("tiefe") {
                    t.set(k, ty.round());
                }
            }
            _ => return None,
        }
        Some(t)
    }

    /// Felder des Werkzeug-Paneels: `zeichnen=ja` und sichtbar, höchstens
    /// [`MAX_FELDER`].
    pub fn felder(&self, g: &Geschoss) -> Vec<ExtFeld> {
        self.def
            .felder(&self.vorlage, g)
            .into_iter()
            .filter(|f| f.zeichnen && f.sichtbar)
            .take(MAX_FELDER)
            .collect()
    }

    /// Welche Maße aus der Eingabe kommen, z. B. „Länge aus der Eingabe“.
    pub fn aus_eingabe(&self, g: &Geschoss) -> Option<String> {
        let felder = self.def.felder(&self.vorlage, g);
        let name = |k: &str| {
            let key = self.eingabe(k)?;
            felder.iter().find(|f| f.key == key).map(|f| f.name.clone())
        };
        let namen: Vec<String> = match self.art {
            Art::Punkt => Vec::new(),
            Art::Linie => name("laenge").into_iter().collect(),
            Art::Rechteck => [name("breite"), name("tiefe")]
                .into_iter()
                .flatten()
                .collect(),
        };
        (!namen.is_empty()).then(|| format!("{} aus der Eingabe", namen.join(" und ")))
    }

    /// Wählt den Typ `key`; eigene Werte, die der Typ setzt, gehen weg.
    pub fn set_typ(&mut self, key: &str) {
        let Some(t) = self.def.typ(key) else {
            return;
        };
        let keys: Vec<String> = t
            .get("werte")
            .unwrap_or("")
            .split(';')
            .filter_map(|x| x.split_once('=').map(|(k, _)| k.trim().to_string()))
            .collect();
        self.vorlage.werte.retain(|(k, _)| !keys.contains(k));
        self.vorlage.typ = Some(key.to_string());
    }
}

/// Eingelesene Definitionen, nach Gruppe und Name geordnet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bibliothek {
    pub defs: Vec<ExtDef>,
}

impl Bibliothek {
    /// Nimmt `d` auf; eine Definition mit gleichem `key` weicht der höheren
    /// Version.
    pub fn dazu(&mut self, d: ExtDef) {
        if let Some(i) = self.defs.iter().position(|x| x.key == d.key) {
            if self.defs[i].version >= d.version {
                return;
            }
            self.defs.remove(i);
        }
        self.defs.push(d);
        self.defs
            .sort_by(|a, b| (a.gruppe_rang(), a.name()).cmp(&(b.gruppe_rang(), b.name())));
    }

    /// Gruppen in Reihenfolge mit den Indizes ihrer Definitionen.
    pub fn gruppen(&self) -> Vec<(&'static str, Vec<usize>)> {
        let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (i, d) in self.defs.iter().enumerate() {
            match out.iter_mut().find(|g| g.0 == d.gruppe()) {
                Some(g) => g.1.push(i),
                None => out.push((d.gruppe(), vec![i])),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;

    const BEISPIELE: [&str; 5] = [
        include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.streifenfundament.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb"),
    ];

    fn modus(i: usize) -> ExtModus {
        ExtModus::new(ExtDef::einlesen(BEISPIELE[i]).unwrap())
    }

    /// Die Vorschau rechnet nur neu, wenn Typ, Werte oder Geschoss sich
    /// ändern; Lage und Drehung nicht (Review 3ci Hinweis a).
    #[test]
    fn vorschau_gemerkt() {
        let g = Geschoss::PROBE;
        let m = modus(3);
        let mut t = m.vorlage.clone();
        let e = m.rechnen(&t, &g);
        assert_eq!(e, sk_model::erweiterung::rechnen(&m.def, &t, &g));
        t.at = [500.0, 700.0];
        t.rot = 90.0;
        assert_eq!(m.rechnen(&t, &g), e);
        assert!(m.vorschau.0.borrow().as_ref().unwrap().0.at == [0.0, 0.0]);
        t.set("b", 300.0);
        let breit = m.rechnen(&t, &g);
        assert_ne!(breit, e);
        assert_eq!(breit, sk_model::erweiterung::rechnen(&m.def, &t, &g));
        let og = Geschoss { gh: 3000.0, ..g };
        assert_eq!(
            m.rechnen(&t, &og),
            sk_model::erweiterung::rechnen(&m.def, &t, &og)
        );
        // Kopie und Vergleich ohne die Rechnung
        assert_eq!(m.clone(), m);
        assert!(m.clone().vorschau.0.borrow().is_none());
    }

    #[test]
    fn punkt_linie_rechteck() {
        let g = Geschoss::PROBE;
        // Stütze: Punkt mit Drehung
        let mut m = modus(3);
        assert_eq!(m.art, Art::Punkt);
        m.rot = 90.0;
        let t = m.teil(&[vec3(100.0, 200.0, 0.0)]).unwrap();
        assert_eq!((t.at, t.rot), ([100.0, 200.0], 90.0));
        assert_eq!(m.aus_eingabe(&g), None);
        let f: Vec<String> = m.felder(&g).iter().map(|f| f.name.clone()).collect();
        assert_eq!(f, ["Breite", "Tiefe"]);
        // Geländer: Linie, Länge in l, Richtung als Drehung
        let m = modus(1);
        assert_eq!(m.art, Art::Linie);
        let t = m
            .teil(&[vec3(0.0, 0.0, 0.0), vec3(0.0, 2500.4, 0.0)])
            .unwrap();
        assert_eq!(t.at, [0.0, 0.0]);
        assert!((t.rot - 90.0).abs() < 1e-9);
        assert_eq!(t.werte, [("l".to_string(), 2500.0)]);
        assert_eq!(m.aus_eingabe(&g).as_deref(), Some("Länge aus der Eingabe"));
        assert_eq!(m.teil(&[vec3(0.0, 0.0, 0.0), vec3(0.5, 0.0, 0.0)]), None);
        // Bodenplatte: Rechteck aus zwei Ecken, gleich in welcher Richtung
        let m = modus(0);
        assert_eq!(m.art, Art::Rechteck);
        let t = m
            .teil(&[vec3(5000.0, 3000.0, 0.0), vec3(1000.0, 0.0, 0.0)])
            .unwrap();
        assert_eq!(t.at, [1000.0, 0.0]);
        assert_eq!(
            t.werte,
            [("b".to_string(), 4000.0), ("t".to_string(), 3000.0)]
        );
        assert_eq!(
            m.aus_eingabe(&g).as_deref(),
            Some("Breite und Tiefe aus der Eingabe")
        );
        // Treppe: Lauflänge aus der Eingabe, Felder wie in der Werkbank
        let m = modus(4);
        let f: Vec<String> = m.felder(&g).iter().map(|f| f.name.clone()).collect();
        assert_eq!(f, ["Lauflänge", "Laufbreite"]);
        assert_eq!(
            m.aus_eingabe(&g).as_deref(),
            Some("Lauflänge aus der Eingabe")
        );
    }

    #[test]
    fn typ_wechsel_nimmt_eigene_werte_des_typs_weg() {
        let g = Geschoss::PROBE;
        let mut m = modus(3);
        let typen: Vec<String> = m.def.def.typ.iter().map(|t| t.key().to_string()).collect();
        assert!(typen.len() >= 2, "{typen:?}");
        m.vorlage.set("b", 555.0);
        m.set_typ(&typen[1]);
        assert_eq!(m.vorlage.typ.as_deref(), Some(typen[1].as_str()));
        let b = m.felder(&g).into_iter().find(|f| f.key == "b").unwrap();
        assert_ne!(b.wert, 555.0);
    }

    #[test]
    fn bibliothek_aus_einem_ordner() {
        let dir = std::env::temp_dir().join(format!("skizzeo-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, t) in BEISPIELE.iter().enumerate() {
            std::fs::write(dir.join(format!("b{i}.szb")), t).unwrap();
        }
        std::fs::write(dir.join("kaputt.szb"), "SZB 0\n[bauteil] key=x\n").unwrap();
        std::fs::write(dir.join("notiz.txt"), "kein Bauteil").unwrap();
        let a = crate::ext_ablage::Ablage::lesen(&dir);
        let (b, h) = (a.bibliothek(), a.hinweise);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(b.defs.len(), 5);
        assert_eq!(h.len(), 1);
        assert!(h[0].starts_with("kaputt.szb: "), "{h:?}");
        let g: Vec<(&str, Vec<&str>)> = b
            .gruppen()
            .into_iter()
            .map(|(n, ix)| (n, ix.iter().map(|&i| b.defs[i].name()).collect()))
            .collect();
        assert_eq!(
            g,
            [
                (
                    "Tragwerk",
                    vec!["Bodenplatte", "Stahlbetonstütze", "Streifenfundament"]
                ),
                (
                    "Treppen und Geländer",
                    vec!["Gerade Treppe", "Stabgeländer"]
                ),
            ]
        );
        assert_eq!(
            crate::ext_ablage::Ablage::lesen(std::path::Path::new("/gibt/es/nicht")).bibliothek(),
            Bibliothek::default()
        );
    }
}
