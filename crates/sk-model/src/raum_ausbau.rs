//! Abweichungen des Raumbuchs je Raum (RB6, raumbuch/plan.md §4.4): Ein Raum
//! übernimmt Boden, Estrich, Wandbekleidung, Wandfliesen, Decke und
//! Sockelleiste aus seiner Raumvorlage; je Fläche kann er einen anderen
//! Ausbautyp, keinen oder eine andere Fliesenhöhe tragen. Eine Wandseite
//! heißt nach der Guid ihres Wandsegments und der Seite zur Zeichenrichtung,
//! damit sie Teilen und Laden übersteht; ohne Wandseite gilt die Abweichung
//! für alle Wandseiten des Raums, eine Wandseite geht vor. Gespeichert am
//! Raum ([`Space::ausbau`]), so nimmt Rückgängig sie mit dem Raum zurück;
//! in der Datei als `[spacefinish]` (`szo/ausbau.rs`). Methoden von
//! `Model`, aus `model.rs` ausgelagert.

use super::*;
use std::collections::{HashMap, HashSet};

/// Seite eines Wandsegments: links bzw. rechts der Zeichenrichtung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WandSeite {
    pub wand: Guid,
    pub links: bool,
}

/// Fläche eines Raums, an der eine Abweichung gilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AusbauStelle {
    Boden,
    Estrich,
    Decke,
    /// Wandbekleidung (Putz, Tapete, Anstrich); ohne Seite alle Wandseiten.
    Wand(Option<WandSeite>),
    /// Wandfliesen mit ihrer Höhe; ohne Seite alle Wandseiten.
    Fliesen(Option<WandSeite>),
    /// Sockelleiste; ohne Seite alle Wandseiten.
    Sockel(Option<WandSeite>),
}

impl AusbauStelle {
    /// Typart, die an der Stelle stehen darf.
    pub fn art(self) -> TypeCategory {
        match self {
            AusbauStelle::Boden => TypeCategory::FloorFinish,
            AusbauStelle::Estrich => TypeCategory::Screed,
            AusbauStelle::Decke => TypeCategory::CeilingFinish,
            AusbauStelle::Wand(_) => TypeCategory::WallFinish,
            AusbauStelle::Fliesen(_) => TypeCategory::WallTiles,
            AusbauStelle::Sockel(_) => TypeCategory::Skirting,
        }
    }

    /// Wandseite der Stelle, `None` bei Boden, Estrich, Decke und für alle
    /// Wandseiten.
    pub fn seite(self) -> Option<WandSeite> {
        match self {
            AusbauStelle::Wand(s) | AusbauStelle::Fliesen(s) | AusbauStelle::Sockel(s) => s,
            _ => None,
        }
    }

    /// Dieselbe Stelle an der Wandseite `s` (Wandteilung).
    fn an(self, s: WandSeite) -> AusbauStelle {
        match self {
            AusbauStelle::Wand(_) => AusbauStelle::Wand(Some(s)),
            AusbauStelle::Fliesen(_) => AusbauStelle::Fliesen(Some(s)),
            AusbauStelle::Sockel(_) => AusbauStelle::Sockel(Some(s)),
            x => x,
        }
    }
}

/// Was an der Stelle statt der Vorlage gilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AusbauWahl {
    /// Kein Ausbau (kein Belag, kein Putz, keine Fliesen, keine Sockelleiste).
    Keiner,
    /// Ausbautyp nach Guid.
    Typ(Guid),
}

/// Eine Abweichung von der Raumvorlage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AusbauAbweichung {
    pub stelle: AusbauStelle,
    /// `None`: Typ wie die Vorlage (bei Fliesen nur eine andere Höhe).
    pub wahl: Option<AusbauWahl>,
    /// Nur Wandfliesen: Höhe ab OK Fertigfußboden (mm), `None` wie die
    /// Vorlage.
    pub hoehe: Option<f64>,
}

impl AusbauAbweichung {
    /// Weicht sie überhaupt ab?
    pub fn leer(&self) -> bool {
        self.wahl.is_none() && self.hoehe.is_none()
    }
}

/// Befund einer Abweichung vor dem Setzen, `None` wenn sie passt.
pub fn ausbau_befund(m: &Model, a: &AusbauAbweichung) -> Option<String> {
    if let Some(AusbauWahl::Typ(g)) = a.wahl {
        let Some(t) = m.type_by_guid(g).and_then(|id| m.layer_set(id)) else {
            return Some("Ausbautyp fehlt im Projekt".into());
        };
        if t.category != a.stelle.art() {
            return Some(kein(&t.name, a.stelle));
        }
    }
    hoehe_befund(a)
}

/// „Gipsputz 1,5 ist kein Bodenbelag“.
fn kein(name: &str, s: AusbauStelle) -> String {
    format!("{name} ist kein {}", s.art().name())
}

/// Befund der Fliesenhöhe.
fn hoehe_befund(a: &AusbauAbweichung) -> Option<String> {
    match (a.stelle, a.hoehe) {
        (AusbauStelle::Fliesen(_), Some(h)) => {
            let (lo, hi) = TILES_HEIGHT;
            (!(lo..=hi).contains(&h))
                .then(|| format!("Fliesenhöhe {} bis {} m", m_de(lo), m_de(hi)))
        }
        (_, Some(_)) => Some("Höhe nur bei Wandfliesen".into()),
        _ => None,
    }
}

/// „1,20“ (m) aus mm.
fn m_de(mm: f64) -> String {
    format!("{:.2}", mm / 1000.0).replace('.', ",")
}

/// Abweichungen in fester Reihenfolge, je Stelle eine.
fn ordnen(v: &mut Vec<AusbauAbweichung>) {
    v.sort_by_key(|a| a.stelle);
    v.dedup_by_key(|a| a.stelle);
}

impl Model {
    /// Setzt eine Abweichung am Raum `raum` (im offenen Schritt); eine leere
    /// nimmt die Abweichung der Stelle zurück. Einen Werks-Ausbautyp legt
    /// sie im Projekt an. `Err` mit dem Befund, wenn `raum` kein Raum ist,
    /// der Typ fehlt, nicht zur Stelle passt oder die Höhe nicht stimmt.
    pub fn set_space_finish(&mut self, raum: ElementId, a: AusbauAbweichung) -> Result<(), String> {
        if self.space(raum).is_none() {
            return Err("Kein Raum".into());
        }
        // Einen Werks-Ausbautyp erst prüfen, dann anlegen: kein Typ im
        // Projekt aus einer Abweichung, die nicht gilt
        if let Some(AusbauWahl::Typ(g)) = a.wahl {
            if self.type_by_guid(g).is_none() {
                let fehlt = || "Ausbautyp fehlt im Projekt".to_string();
                let t = FINISH_TYPES
                    .iter()
                    .find(|t| t.guid == g)
                    .ok_or_else(fehlt)?;
                if t.category != a.stelle.art() {
                    return Err(kein(t.name, a.stelle));
                }
                if let Some(b) = hoehe_befund(&a) {
                    return Err(b);
                }
                self.ensure_finish_type(g).ok_or_else(fehlt)?;
            }
        }
        if let Some(b) = ausbau_befund(self, &a) {
            return Err(b);
        }
        self.set_space(raum, |s| {
            s.ausbau.retain(|x| x.stelle != a.stelle);
            if !a.leer() {
                s.ausbau.push(a);
            }
            ordnen(&mut s.ausbau);
        });
        Ok(())
    }

    /// Abweichung des Raums an genau dieser Stelle.
    pub fn space_finish(&self, raum: ElementId, stelle: AusbauStelle) -> Option<&AusbauAbweichung> {
        self.space(raum)?.ausbau.iter().find(|a| a.stelle == stelle)
    }

    /// Vereinte Wände (Verlängern in einer Flucht, G5c): Die Abweichungen
    /// der Wandseiten von `alt` gelten nun an `neu`, mit getauschter Seite,
    /// wenn `alt` gegen `neu` lief (`spiegeln`). Hat ein Raum an der Stelle
    /// von `neu` schon eine, bleibt diese, die von `alt` fällt weg. Eine
    /// Abweichung, die nur für die Wandseite von `alt` galt, gilt danach
    /// für die ganze Seite der vereinten Wand (Review H-210k). `alt == neu`
    /// mit `spiegeln` tauscht nur die Seiten (Review #210 B4).
    pub(super) fn ausbau_wand_vereint(&mut self, alt: Guid, neu: Guid, spiegeln: bool) {
        if alt == neu && !spiegeln {
            return;
        }
        let raeume: Vec<ElementId> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Space(s)
                    if s.ausbau
                        .iter()
                        .any(|a| a.stelle.seite().is_some_and(|w| w.wand == alt)) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .collect();
        for id in raeume {
            self.set_space(id, |s| {
                let (weg, mut bleibt): (Vec<AusbauAbweichung>, Vec<AusbauAbweichung>) = s
                    .ausbau
                    .iter()
                    .partition(|a| a.stelle.seite().is_some_and(|w| w.wand == alt));
                for a in weg {
                    let Some(w) = a.stelle.seite() else {
                        continue;
                    };
                    let seite = WandSeite {
                        wand: neu,
                        links: w.links != spiegeln,
                    };
                    let stelle = a.stelle.an(seite);
                    if !bleibt.iter().any(|b| b.stelle == stelle) {
                        bleibt.push(AusbauAbweichung { stelle, ..a });
                    }
                }
                ordnen(&mut bleibt);
                s.ausbau = bleibt;
            });
        }
    }

    /// Geteilte Wand (`set_points`): Die Abweichungen der Wandseiten von
    /// `alt` gelten auch für das neue Stück `neu` (schnittstelle-raum.md
    /// §4, Grundriss 17:29).
    pub(super) fn ausbau_wand_geteilt(&mut self, alt: Guid, neu: Guid) {
        let raeume: Vec<ElementId> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Space(s)
                    if s.ausbau
                        .iter()
                        .any(|a| a.stelle.seite().is_some_and(|w| w.wand == alt)) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .collect();
        for id in raeume {
            self.set_space(id, |s| {
                let dazu: Vec<AusbauAbweichung> = s
                    .ausbau
                    .iter()
                    .filter_map(|a| {
                        let w = a.stelle.seite().filter(|w| w.wand == alt)?;
                        Some(AusbauAbweichung {
                            stelle: a.stelle.an(WandSeite { wand: neu, ..w }),
                            ..*a
                        })
                    })
                    .collect();
                s.ausbau.extend(dazu);
                ordnen(&mut s.ausbau);
            });
        }
    }

    /// Weggefallene Wandsegmente (Löschen, Zugpunkte, Wandzug): Die
    /// Abweichungen ihrer Wandseiten gehen im selben Schritt aus den Räumen,
    /// kein toter Verweis (BIM Regel 24); Rückgängig bringt beides zurück.
    pub(super) fn ausbau_wand_weg(&mut self, weg: &[Guid]) {
        if weg.is_empty() {
            return;
        }
        let weg: HashSet<Guid> = weg.iter().copied().collect();
        let trifft = |a: &AusbauAbweichung| a.stelle.seite().is_some_and(|w| weg.contains(&w.wand));
        let raeume: Vec<ElementId> = self
            .elements
            .iter()
            .filter_map(|(id, e)| match &e.kind {
                ElementKind::Space(s) if s.ausbau.iter().any(trifft) => Some(id),
                _ => None,
            })
            .collect();
        for id in raeume {
            self.set_space(id, |s| s.ausbau.retain(|a| !trifft(a)));
        }
    }

    /// Zeilen `[spacefinish]`, die nicht zählen.
    pub(crate) fn space_finish_raw(&self) -> &[RohZeile] {
        &self.space_finish_raw
    }

    /// Verweist eine Abweichung eines Raums auf den Typ? Dann lässt er
    /// sich weder löschen noch in eine andere Art umstellen (Review #190
    /// B2), wie bei einer Raumvorlage.
    pub fn type_in_space_finish(&self, id: LayerSetId) -> bool {
        let Some(g) = self.layer_set(id).map(|t| t.guid) else {
            return false;
        };
        self.elements.iter().any(|(_, e)| match &e.kind {
            ElementKind::Space(s) => s.ausbau.iter().any(|a| a.wahl == Some(AusbauWahl::Typ(g))),
            _ => false,
        })
    }

    /// Lädt die Abweichungen aus der Datei an ihre Räume (ohne Schritt),
    /// einmal über die Bauteile (Review #190 B3).
    pub(crate) fn load_space_finish(
        &mut self,
        v: Vec<(Guid, AusbauAbweichung)>,
        raw: Vec<RohZeile>,
    ) {
        self.space_finish_raw = raw;
        if v.is_empty() {
            return;
        }
        let mut je: HashMap<Guid, Vec<AusbauAbweichung>> = HashMap::new();
        for (raum, a) in v {
            je.entry(raum).or_default().push(a);
        }
        let ziele: Vec<(ElementId, Vec<AusbauAbweichung>)> = self
            .elements
            .iter()
            .filter(|(_, e)| matches!(e.kind, ElementKind::Space(_)))
            .filter_map(|(id, e)| Some((id, je.remove(&e.guid)?)))
            .collect();
        for (id, v) in ziele {
            if let Some(ElementKind::Space(s)) = self.elements.get_mut(id).map(|e| &mut e.kind) {
                s.ausbau.extend(v);
                ordnen(&mut s.ausbau);
            }
        }
    }
}

/// Eine Zeile `[spacefinish]`, die nicht zählt, und Raum und Stelle der
/// gültigen Zeile mit derselben Kennung, vor der sie in der Datei stand:
/// Dort schreibt sie das Programm wieder hin, so bleibt die Folge gleicher
/// Kennungen für fremde Schlüssel ([`crate::catalog::record_key`]).
pub(crate) type RohZeile = (String, Option<(Guid, AusbauStelle)>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::Direction;
    use crate::{szo, GuidGen};
    use AusbauStelle::*;
    use AusbauWahl::*;

    /// Haus 10 × 8 m, ein Raum im EG; dazu die Nordwand.
    fn haus() -> (Model, ElementId, ElementId) {
        let mut m = Model::with_seed(111);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        m.begin("Raum");
        let r = m.add_space(st, Space::neu(vec3(5000.0, 4000.0, 0.0), "Bad".into()));
        m.commit();
        let nord = m.wall_at(eg, 1).unwrap();
        (m, r, nord)
    }

    fn rechts(m: &Model, w: ElementId) -> WandSeite {
        WandSeite {
            wand: m.element(w).unwrap().guid,
            links: false,
        }
    }

    fn ab(stelle: AusbauStelle, wahl: Option<AusbauWahl>, hoehe: Option<f64>) -> AusbauAbweichung {
        AusbauAbweichung {
            stelle,
            wahl,
            hoehe,
        }
    }

    /// Setzen prüft Raum, Typ, Art und Höhe; ein Werks-Ausbautyp entsteht
    /// erst, wenn die Abweichung gilt. Je Stelle eine Abweichung, geordnet;
    /// eine leere nimmt sie zurück.
    #[test]
    fn setzen_und_pruefen() {
        let (mut m, r, nord) = haus();
        let w = rechts(&m, nord);
        let fehler = |m: &mut Model, raum, a| m.set_space_finish(raum, a).unwrap_err();
        assert_eq!(
            fehler(&mut m, nord, ab(Boden, Some(Keiner), None)),
            "Kein Raum"
        );
        let e = fehler(&mut m, r, ab(Boden, Some(Typ(GYPSUM_WALL_TYPE_GUID)), None));
        assert!(e.ends_with(" ist kein Bodenbelag"), "{e}");
        let fliesen = ab(
            Fliesen(Some(w)),
            Some(Typ(WALL_TILES_TYPE_GUID)),
            Some(5000.0),
        );
        assert_eq!(fehler(&mut m, r, fliesen), "Fliesenhöhe 0,10 bis 4,00 m");
        assert!(
            m.type_by_guid(GYPSUM_WALL_TYPE_GUID).is_none(),
            "nicht angelegt"
        );
        assert!(
            m.type_by_guid(WALL_TILES_TYPE_GUID).is_none(),
            "nicht angelegt"
        );
        let e = fehler(&mut m, r, ab(Wand(None), None, Some(1000.0)));
        assert_eq!(e, "Höhe nur bei Wandfliesen");
        let e = fehler(&mut m, r, ab(Boden, Some(Typ(Guid(0x5151))), None));
        assert_eq!(e, "Ausbautyp fehlt im Projekt");
        assert!(m.space(r).unwrap().ausbau.is_empty());

        let fliesen = ab(
            Fliesen(Some(w)),
            Some(Typ(WALL_TILES_TYPE_GUID)),
            Some(2000.0),
        );
        m.set_space_finish(r, fliesen).unwrap();
        m.set_space_finish(r, ab(Boden, Some(Keiner), None))
            .unwrap();
        m.set_space_finish(r, ab(Wand(None), Some(Typ(LIME_WALL_TYPE_GUID)), None))
            .unwrap();
        assert!(m.type_by_guid(WALL_TILES_TYPE_GUID).is_some(), "angelegt");
        let stellen: Vec<AusbauStelle> = m
            .space(r)
            .unwrap()
            .ausbau
            .iter()
            .map(|a| a.stelle)
            .collect();
        assert_eq!(stellen, vec![Boden, Wand(None), Fliesen(Some(w))]);
        // Ersetzen: nur eine andere Höhe, Typ wie die Vorlage
        m.set_space_finish(r, ab(Fliesen(Some(w)), None, Some(1500.0)))
            .unwrap();
        let f = m.space_finish(r, Fliesen(Some(w))).unwrap();
        assert_eq!((f.wahl, f.hoehe), (None, Some(1500.0)));
        // Leer nimmt zurück
        m.set_space_finish(r, ab(Boden, None, None)).unwrap();
        assert!(m.space_finish(r, Boden).is_none());
        assert_eq!(m.space(r).unwrap().ausbau.len(), 2);
        // Ein Typ des Projekts anderer Art
        let decke = ab(Decke, Some(Typ(LIME_WALL_TYPE_GUID)), None);
        assert!(
            ausbau_befund(&m, &decke).is_some_and(|b| b.ends_with(" ist kein Deckenbekleidung"))
        );
    }

    /// Rückgängig nimmt Abweichung und angelegten Typ zurück, Wiederholen
    /// bringt sie; die Datei liest sich bytegleich zurück, ohne Abweichung
    /// ohne Abschnitt.
    #[test]
    fn rueckgaengig_und_datei() {
        let (mut m, r, nord) = haus();
        let w = rechts(&m, nord);
        let vorher = szo::write(&m);
        assert!(!vorher.contains("[spacefinish]"));
        m.begin("Abweichung");
        m.set_space_finish(r, ab(Boden, Some(Keiner), None))
            .unwrap();
        let fliesen = ab(
            Fliesen(Some(w)),
            Some(Typ(WALL_TILES_TYPE_GUID)),
            Some(2000.0),
        );
        m.set_space_finish(r, fliesen).unwrap();
        let tx = m.commit().unwrap();
        let nachher = szo::write(&m);
        m.apply(&tx, Direction::Undo);
        assert_eq!(szo::write(&m), vorher);
        m.apply(&tx, Direction::Redo);
        assert_eq!(szo::write(&m), nachher);

        let zeilen: Vec<&str> = nachher
            .lines()
            .filter(|l| l.starts_with("[spacefinish]"))
            .collect();
        assert_eq!(zeilen.len(), 2, "{zeilen:?}");
        assert!(zeilen[0].ends_with(" at=floor type=none"), "{}", zeilen[0]);
        let wand = format!(" at=tiles wall={} side=r type=", w.wand.to_ifc());
        assert!(
            zeilen[1].contains(&wand) && zeilen[1].ends_with(" h=2000"),
            "{}",
            zeilen[1]
        );
        let l = szo::read(&nachher, GuidGen::with_seed(5)).unwrap();
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(szo::write(&l.model), nachher);
        let raum = l.model.spaces(m.element(r).unwrap().storey)[0];
        assert_eq!(
            l.model.space(raum).unwrap().ausbau,
            m.space(r).unwrap().ausbau
        );
    }

    /// Eine Zeile, die nicht zählt, gibt einen Hinweis und bleibt roh
    /// stehen; der Raum behält die übrigen.
    #[test]
    fn datei_mit_fehlern() {
        let (mut m, r, _) = haus();
        m.set_space_finish(r, ab(Wand(None), Some(Typ(LIME_WALL_TYPE_GUID)), None))
            .unwrap();
        let text = szo::write(&m);
        let zeile = text
            .lines()
            .find(|l| l.starts_with("[spacefinish]"))
            .unwrap();
        let raum = m.element(r).unwrap().guid.to_ifc();
        let schlecht = [
            zeile.replace(&raum, &Guid(0x77).to_ifc()),
            zeile.replace(" at=wall", " at=floor"),
            zeile.replace(" at=wall", " at=roof"),
            format!("{zeile} h=1200"),
            zeile.replace(&LIME_WALL_TYPE_GUID.to_ifc(), &Guid(0x78).to_ifc()),
            zeile.to_string(),
        ];
        let datei = format!("{text}{}\n", schlecht.join("\n"));
        let l = szo::read(&datei, GuidGen::with_seed(5)).unwrap();
        let gruende: Vec<&str> = l
            .hints
            .iter()
            .map(|h| {
                let h = h.split_once("[spacefinish] ").unwrap().1;
                h.split_once(';').unwrap().0
            })
            .collect();
        assert_eq!(
            gruende,
            [
                "Raum fehlt",
                "Kalkzementputz mit Anstrich ist kein Bodenbelag",
                "unlesbar",
                "Höhe nur bei Wandfliesen, 0,10 bis 4,00 m",
                "Ausbautyp fehlt",
                "doppelt"
            ],
            "{:?}",
            l.hints
        );
        let raum = l.model.spaces(m.element(r).unwrap().storey)[0];
        assert_eq!(
            l.model.space(raum).unwrap().ausbau,
            m.space(r).unwrap().ausbau
        );
        let wieder = szo::write(&l.model);
        for s in &schlecht {
            assert!(wieder.contains(&format!("{s}\n")), "{s}");
        }
        let l2 = szo::read(&wieder, GuidGen::with_seed(5)).unwrap();
        assert_eq!(szo::write(&l2.model), wieder, "stabil");
    }

    /// Fremde Schlüssel bleiben an ihrer Zeile, auch wenn eine rohe Zeile
    /// derselben Stelle davor steht und die Räume in anderer Reihenfolge
    /// stehen, als das Programm sie schreibt (Test an #190).
    #[test]
    fn fremder_schluessel_hinter_roher_zeile() {
        let (mut m, r, _) = haus();
        m.set_space_finish(r, ab(Boden, Some(Keiner), None))
            .unwrap();
        let text = szo::write(&m);
        let gueltig = text
            .lines()
            .find(|l| l.starts_with("[spacefinish]"))
            .unwrap()
            .to_string();
        let typ = |g: Guid| gueltig.replace("type=none", &format!("type={}", g.to_ifc()));
        // Wandputz als Boden zählt nicht und bleibt roh, vor der gültigen
        let roh = typ(LIME_WALL_TYPE_GUID);
        let fremd = format!("{gueltig} zukunft=1");
        let datei = text.replace(&gueltig, &format!("{roh}\n{fremd}"));
        let l = szo::read(&datei, GuidGen::with_seed(5)).unwrap();
        let wieder = szo::write(&l.model);
        let zeilen = |t: &str| -> Vec<String> {
            let mut v: Vec<String> = t
                .lines()
                .filter(|l| l.starts_with("[spacefinish]"))
                .map(str::to_string)
                .collect();
            v.sort();
            v
        };
        let mut soll = vec![roh.clone(), fremd.clone()];
        soll.sort();
        assert_eq!(zeilen(&wieder), soll);
        assert!(wieder.contains(&format!("{roh}\n{fremd}\n")), "Folge");
        let l2 = szo::read(&wieder, GuidGen::with_seed(5)).unwrap();
        assert_eq!(szo::write(&l2.model), wieder, "stabil");
        // Ein anderer Typ ändert den Eintrag: Er steht ohne den fremden
        // Schlüssel (F-17), die rohe Zeile bleibt davor
        let mut m2 = l2.model;
        let raum = m2.spaces(m.element(r).unwrap().storey)[0];
        m2.begin("Typ");
        m2.set_space_finish(raum, ab(Boden, Some(Typ(WET_FLOOR_TYPE_GUID)), None))
            .unwrap();
        m2.commit();
        let neu = szo::write(&m2);
        assert!(
            neu.contains(&format!("{roh}\n{}\n", typ(WET_FLOOR_TYPE_GUID))),
            "{:?}",
            zeilen(&neu)
        );
    }

    /// Ein Typ, den nur eine Abweichung nennt, lässt sich weder löschen
    /// noch in eine andere Art umstellen (Review #190 B2); ohne die
    /// Abweichung geht beides.
    #[test]
    fn typ_der_abweichung_bleibt() {
        let (mut m, r, _) = haus();
        m.begin("Abweichung");
        m.set_space_finish(r, ab(Boden, Some(Typ(WET_FLOOR_TYPE_GUID)), None))
            .unwrap();
        m.commit();
        let id = m.type_by_guid(WET_FLOOR_TYPE_GUID).unwrap();
        assert!(m.type_users(id).is_empty() && !m.type_in_space_type(id));
        assert!(m.type_in_space_finish(id));
        m.begin("Löschen");
        assert_eq!(m.remove_type(id), Err(0));
        let mut t = m.layer_set(id).unwrap().clone();
        t.category = TypeCategory::CeilingFinish;
        assert!(!m.set_layer_set(id, t.clone()));
        m.set_space_finish(r, ab(Boden, None, None)).unwrap();
        assert!(!m.type_in_space_finish(id));
        assert!(m.set_layer_set(id, t));
        assert_eq!(m.remove_type(id), Ok(()));
        assert!(m.commit().is_some());
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Geteilte Wand: Die Abweichungen ihrer Wandseiten gelten an beiden
    /// Stücken; Rückgängig nimmt das neue Stück samt Abweichung zurück.
    #[test]
    fn wand_teilen() {
        let (mut m, r, nord) = haus();
        let w = rechts(&m, nord);
        let fliesen = ab(
            Fliesen(Some(w)),
            Some(Typ(WALL_TILES_TYPE_GUID)),
            Some(2000.0),
        );
        m.set_space_finish(r, fliesen).unwrap();
        m.set_space_finish(r, ab(Sockel(Some(w)), Some(Keiner), None))
            .unwrap();
        m.set_space_finish(r, ab(Decke, Some(Keiner), None))
            .unwrap();
        let vorher = m.space(r).unwrap().ausbau.clone();
        m.begin("Teilen");
        m.split_wall(nord, &[4000.0]).unwrap();
        let tx = m.commit().unwrap();
        let a = &m.space(r).unwrap().ausbau;
        assert_eq!(a.len(), 5, "{a:?}");
        let neu: Vec<Guid> = a
            .iter()
            .filter_map(|x| x.stelle.seite())
            .map(|s| s.wand)
            .filter(|g| *g != w.wand)
            .collect();
        assert_eq!(neu.len(), 2);
        assert_eq!(neu[0], neu[1]);
        let wn = WandSeite { wand: neu[0], ..w };
        assert!(m
            .elements()
            .iter()
            .any(|(_, e)| e.guid == wn.wand && matches!(e.kind, ElementKind::Wall(_))));
        assert_eq!(
            m.space_finish(r, Fliesen(Some(wn))).unwrap().hoehe,
            Some(2000.0)
        );
        assert_eq!(
            m.space_finish(r, Sockel(Some(wn))).unwrap().wahl,
            Some(Keiner)
        );
        m.apply(&tx, Direction::Undo);
        assert_eq!(m.space(r).unwrap().ausbau, vorher);
    }

    /// Wandseiten des Raums, deren Wand es nicht gibt (BIM Regel 24).
    fn tote(m: &Model, r: ElementId) -> Vec<WandSeite> {
        m.space(r)
            .unwrap()
            .ausbau
            .iter()
            .filter_map(|a| a.stelle.seite())
            .filter(|w| {
                !m.elements()
                    .iter()
                    .any(|(_, e)| e.guid == w.wand && matches!(e.kind, ElementKind::Wall(_)))
            })
            .collect()
    }

    /// Freier Wandzug neben dem Haus mit drei Segmenten (Umrisswände
    /// lassen sich nicht löschen); die Abweichungen prüfen ihre Wand nicht
    /// gegen den Raum.
    fn frei(m: &mut Model, nord: ElementId) -> RunId {
        let set = m.element(nord).unwrap().layer_set.unwrap();
        let (seite, st) = {
            let r = m.run(m.segment_of(nord).unwrap().0).unwrap();
            (r.ref_side, r.storey)
        };
        let pts = [
            vec3(14000.0, 0.0, 0.0),
            vec3(14000.0, 6000.0, 0.0),
            vec3(18000.0, 6000.0, 0.0),
            vec3(18000.0, 0.0, 0.0),
        ];
        m.begin("Wand");
        let run = m
            .add_wall_run(&pts, false, seite, st, set, Category::ExteriorWall)
            .unwrap();
        m.commit();
        run
    }

    /// Gelöschte Wand: Ihre Wandseiten-Abweichungen gehen im selben Schritt
    /// aus dem Raum, die für alle Wandseiten bleibt; Rückgängig bringt Wand
    /// und Abweichungen zurück (BIM Regel 24).
    #[test]
    fn wand_loeschen() {
        let (mut m, r, nord) = haus();
        let run = frei(&mut m, nord);
        let wand = m.wall_at(run, 1).unwrap();
        let w = rechts(&m, wand);
        let fliesen = ab(
            Fliesen(Some(w)),
            Some(Typ(WALL_TILES_TYPE_GUID)),
            Some(2000.0),
        );
        m.set_space_finish(r, fliesen).unwrap();
        let links = WandSeite { links: true, ..w };
        m.set_space_finish(r, ab(Sockel(Some(links)), Some(Keiner), None))
            .unwrap();
        let alle = ab(Wand(None), Some(Keiner), None);
        m.set_space_finish(r, alle).unwrap();
        let nordseite = ab(Sockel(Some(rechts(&m, nord))), Some(Keiner), None);
        m.set_space_finish(r, nordseite).unwrap();
        let vorher = m.space(r).unwrap().ausbau.clone();
        m.begin("Löschen");
        let d = m.delete_elements(&[wand]);
        assert_eq!(d.removed, vec![wand], "{:?}", d.refused);
        let tx = m.commit().unwrap();
        let mut bleibt = vec![alle, nordseite];
        ordnen(&mut bleibt);
        assert_eq!(m.space(r).unwrap().ausbau, bleibt);
        m.apply(&tx, Direction::Undo);
        assert_eq!(m.space(r).unwrap().ausbau, vorher);
        assert!(tote(&m, r).is_empty());
        m.apply(&tx, Direction::Redo);
        assert_eq!(m.space(r).unwrap().ausbau, bleibt);
        assert!(m.check().is_empty(), "{:?}", m.check());
    }

    /// Zugpunkte mit weniger Segmenten: Was an einer weggefallenen Wand
    /// hing, geht mit ihr; keine Abweichung zeigt auf eine fehlende Wand,
    /// Rückgängig bringt alle zurück (BIM Regel 24).
    #[test]
    fn zug_mit_weniger_segmenten() {
        let (mut m, r, nord) = haus();
        let run = frei(&mut m, nord);
        let waende = m.run(run).unwrap().segments.clone();
        assert_eq!(waende.len(), 3);
        for &e in &waende {
            m.set_space_finish(r, ab(Sockel(Some(rechts(&m, e))), Some(Keiner), None))
                .unwrap();
        }
        let vorher = m.space(r).unwrap().ausbau.clone();
        m.begin("Zug");
        let pts = [
            vec3(14000.0, 0.0, 0.0),
            vec3(14000.0, 6000.0, 0.0),
            vec3(18000.0, 6000.0, 0.0),
        ];
        assert!(m.set_run_points(run, &pts).is_some());
        let tx = m.commit().unwrap();
        assert_eq!(m.run(run).unwrap().segments.len(), 2);
        assert!(waende.iter().any(|&e| m.element(e).is_none()));
        assert!(tote(&m, r).is_empty(), "{:?}", tote(&m, r));
        assert_eq!(m.space(r).unwrap().ausbau.len(), 2);
        m.apply(&tx, Direction::Undo);
        assert_eq!(m.space(r).unwrap().ausbau, vorher);
        assert!(tote(&m, r).is_empty());
    }
}
