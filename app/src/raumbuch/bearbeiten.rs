//! Abweichungen des Raumbuchs im Paneel „Eigenschaften“ eines Raums (RB6,
//! `raumbuch/plan.md` §4.4): „Nutzung …“ wählt die Raumvorlage aus Projekt
//! und Werk (Bedienbarkeit 40.5), je Fläche ein Knopf öffnet die Liste:
//! „wie Raumvorlage“, „ohne …“ und die Ausbautypen der Art aus Projekt und
//! Werk. Wand, Wandfliesen und Sockelleiste gelten für alle Wandseiten oder
//! für die Wandseite, die der Knopf darüber wählt; die Fliesenhöhe ist ein
//! Zahlenfeld. Jede Änderung ist ein Rückgängig-Schritt.

use crate::auswahl::Zeile;
use crate::scene::Scene;
use crate::ui::{Field, FieldRow, Id, Section};
use crate::App;
use sk_model::model::{
    AusbauAbweichung, AusbauStelle, AusbauWahl, WandSeite, FINISH_TYPES, TILES_DEFAULT,
    TILES_HEIGHT,
};
use sk_model::{ElementId, Guid, Model, SpaceType};
use sk_raum::ausbau::{self, RaumAusbau};
use sk_raum::oeffnungen::Rohbau;
use sk_raum::{Kante, RaumForm};
use std::sync::Mutex;

/// Knöpfe im Paneel, Wert in [`Id::Raumbuch`].
pub const BODEN: u8 = 0;
pub const ESTRICH: u8 = 1;
pub const DECKE: u8 = 2;
pub const SEITE: u8 = 3;
pub const WAND: u8 = 4;
pub const FLIESEN: u8 = 5;
pub const SOCKEL: u8 = 6;
pub const NUTZUNG: u8 = 7;

/// Je Knopf: Beschriftung wie die Vorlage und abweichend, Name der Fläche
/// und was „leer“ heißt.
const KNOEPFE: [(&str, &str, &str, &str); 8] = [
    ("Boden …", "Boden: abweichend …", "Boden", "ohne Belag"),
    (
        "Estrich …",
        "Estrich: abweichend …",
        "Estrich",
        "ohne Estrich",
    ),
    ("Decke …", "Decke: abweichend …", "Decke", "ohne Bekleidung"),
    ("", "", "Wandseite", ""),
    ("Wand …", "Wand: abweichend …", "Wand", "ohne Bekleidung"),
    (
        "Wandfliesen …",
        "Wandfliesen: abweichend …",
        "Wandfliesen",
        "keine Fliesen",
    ),
    (
        "Sockelleiste …",
        "Sockelleiste: abweichend …",
        "Sockelleiste",
        "keine Sockelleiste",
    ),
    ("Nutzung …", "Nutzung …", "Nutzung", ""),
];

/// Was ein Eintrag der Liste tut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tun {
    /// Wandseite zum Bearbeiten wählen (`None`: alle).
    Seite(Option<WandSeite>),
    /// Wahl an der Stelle setzen (`None`: wie Raumvorlage bzw. wie alle
    /// Wandseiten).
    Wahl(Option<AusbauWahl>),
    /// Raumvorlage wählen.
    Nutzung(Guid),
}

/// Stelle des Knopfs `k` an der Wandseite `seite` (`None`: alle).
fn stelle(k: u8, seite: Option<WandSeite>) -> Option<AusbauStelle> {
    Some(match k {
        BODEN => AusbauStelle::Boden,
        ESTRICH => AusbauStelle::Estrich,
        DECKE => AusbauStelle::Decke,
        WAND => AusbauStelle::Wand(seite),
        FLIESEN => AusbauStelle::Fliesen(seite),
        SOCKEL => AusbauStelle::Sockel(seite),
        _ => return None,
    })
}

/// Text eines Knopfs mit Wandseite. Knöpfe tragen `&'static str`; es gibt
/// nur so viele Texte, wie Wandseiten Namen haben.
fn fest(t: String) -> &'static str {
    static TEXTE: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut v = TEXTE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = v.iter().find(|s| **s == t) {
        return s;
    }
    let s: &'static str = Box::leak(t.into_boxed_str());
    v.push(s);
    s
}

/// Ein Raum mit Fläche: Rohbau, Ausbau und seine Wandseiten mit Namen
/// („Wand AW-002 Nord“) in der Folge der Kanten.
struct Raum {
    id: ElementId,
    roh: Rohbau,
    aus: RaumAusbau,
    seiten: Vec<(WandSeite, String)>,
}

impl Raum {
    /// Mit den Wandzügen der Szene (Review #112 B1).
    fn neu(scene: &Scene, id: ElementId, form: RaumForm) -> Option<Raum> {
        let m = scene.model();
        let roh = Rohbau::mit(m, &scene.ketten(), form);
        let aus = RaumAusbau::fuer(m, &roh)?;
        let kanten: Vec<&Kante> = roh.kanten().collect();
        let mut seiten: Vec<(WandSeite, String)> = Vec::new();
        for (k, kante) in kanten.iter().enumerate() {
            let Some(w) = ausbau::wand_seite(m, kante) else {
                continue;
            };
            if !seiten.iter().any(|s| s.0 == w) {
                let name = sk_raum::mengen::seite(m, &kanten, k);
                seiten.push((w, format!("Wand {name}")));
            }
        }
        Some(Raum {
            id,
            roh,
            aus,
            seiten,
        })
    }

    /// Aus dem Speicher der Räume wie das Paneel (Review #87), nicht aus
    /// einer neuen Erkennung des Geschosses.
    fn finden(scene: &Scene, id: ElementId) -> Option<Raum> {
        let g = scene.model().element(id)?.storey;
        let form = scene
            .raumgruppen(g)
            .flaechen()
            .map(|f| &f.form)
            .find(|f| f.raum == Some(id))?
            .clone();
        Raum::neu(scene, id, form)
    }

    /// Die Wandseite, die das Paneel bearbeitet, solange es sie gibt.
    fn seite(&self, scene: &Scene) -> Option<WandSeite> {
        scene
            .raum_seite()
            .filter(|s| s.0 == self.id && self.seiten.iter().any(|x| x.0 == s.1))
            .map(|s| s.1)
    }

    fn name(&self, w: WandSeite) -> Option<&str> {
        self.seiten.iter().find(|s| s.0 == w).map(|s| s.1.as_str())
    }

    /// Wirksame Fliesenhöhe (mm) für `seite`, wenn dort Fliesen liegen; für
    /// alle Wandseiten die des Raums, wenn irgendwo Fliesen liegen.
    fn fliesenhoehe(&self, m: &Model, seite: Option<WandSeite>) -> Option<f64> {
        let kanten = self.roh.kanten();
        let mut je_seite = self
            .aus
            .seiten
            .iter()
            .zip(kanten)
            .filter_map(|(s, k)| Some((ausbau::wand_seite(m, k)?, s.fliesen?.1)));
        match seite {
            Some(w) => je_seite.find(|x| x.0 == w).map(|x| x.1),
            None => {
                je_seite.next()?;
                let abw = m.space_finish(self.id, AusbauStelle::Fliesen(None));
                let vorlage = self.vorlage(m).and_then(|s| s.tiles).map(|t| t.height);
                Some(
                    abw.and_then(|a| a.hoehe)
                        .or(vorlage)
                        .unwrap_or(TILES_DEFAULT),
                )
            }
        }
    }

    fn vorlage<'a>(&self, m: &'a Model) -> Option<std::borrow::Cow<'a, SpaceType>> {
        ausbau::vorlage(m, ausbau::nutzung(m, &self.roh.form))
    }
}

/// Abschnitte des Raumbuchs im Paneel des Raums mit der Form `form`; leer
/// für einen ruhenden Raum.
pub fn abschnitte(scene: &Scene, form: &RaumForm) -> Vec<Section> {
    let m = scene.model();
    let Some(r) = form.raum.and_then(|id| Raum::neu(scene, id, form.clone())) else {
        return Vec::new();
    };
    let seite = r.seite(scene);
    let knopf = |k: u8| {
        let t = KNOEPFE[k as usize];
        let st = stelle(k, seite);
        let abw = st.is_some_and(|st| m.space_finish(r.id, st).is_some());
        (Id::Raumbuch(k), if abw { t.1 } else { t.0 })
    };
    let mut v = vec![Section {
        title: "Raumbuch",
        fields: Vec::new(),
        hint: "Abweichungen von der Raumvorlage gelten nur in diesem Raum.",
        buttons: vec![knopf(NUTZUNG), knopf(BODEN), knopf(ESTRICH), knopf(DECKE)],
    }];
    if r.seiten.is_empty() {
        return v;
    }
    let welche = seite.and_then(|w| r.name(w)).unwrap_or("Alle Wandseiten");
    v.push(Section {
        title: "Wandseiten",
        fields: Vec::new(),
        hint: "Eine einzelne Wandseite geht vor allen Wandseiten.",
        buttons: vec![
            (Id::Raumbuch(SEITE), fest(format!("{welche} …"))),
            knopf(WAND),
            knopf(FLIESEN),
            knopf(SOCKEL),
        ],
    });
    if let Some(h) = r.fliesenhoehe(m, seite) {
        let (min, max) = TILES_HEIGHT;
        v.push(Section {
            title: "Wandfliesen",
            fields: vec![FieldRow {
                field: Field::RaumFliesen,
                label: "Höhe",
                value: h,
                min,
                max,
                zero: false,
                einheit: None,
            }],
            hint: "Ab OK Fertigfußboden, an der gewählten Wandseite.",
            buttons: Vec::new(),
        });
    }
    v
}

/// Name eines Ausbautyps aus Projekt oder Werk.
fn typ_name(m: &Model, g: Guid) -> Option<String> {
    match m.type_by_guid(g).and_then(|id| m.layer_set(id)) {
        Some(t) => Some(t.name.clone()),
        None => FINISH_TYPES
            .iter()
            .find(|t| t.guid == g)
            .map(|t| t.name.to_string()),
    }
}

/// Was die Vorlage an der Stelle des Knopfs `k` vorsieht.
fn aus_vorlage(m: &Model, r: &Raum, k: u8) -> Option<Guid> {
    let s = r.vorlage(m)?;
    match k {
        BODEN => s.floor,
        ESTRICH => s
            .screed
            .or(Some(ausbau::estrich_vorgabe(m, r.roh.form.geschoss))),
        DECKE => s.ceiling,
        WAND => s.wall,
        FLIESEN => s.tiles.map(|t| t.set),
        SOCKEL => s.skirting,
        _ => None,
    }
}

/// Liste an einem Knopf: Titel, Zeilen, was jeder Eintrag tut, und der
/// gewählte Eintrag.
pub type Liste = (String, Vec<Zeile>, Vec<Tun>, Option<usize>);

/// Liste am Knopf `k` des Raums `id`.
pub fn liste(scene: &Scene, id: ElementId, k: u8) -> Option<Liste> {
    let m = scene.model();
    let r = Raum::finden(scene, id)?;
    let seite = r.seite(scene);
    let raum = m.space(id)?;
    let nummer = &m.element(id)?.number;
    // Zeilen mit dem, was sie tun (Überschriften nichts)
    let mut v: Vec<(Zeile, Option<Tun>)> = Vec::new();
    let eintrag = |v: &mut Vec<_>, name: String, detail: String, t: Tun| {
        v.push((Zeile::Eintrag { name, detail }, Some(t)));
    };
    let fertig = |v: Vec<(Zeile, Option<Tun>)>| -> (Vec<Zeile>, Vec<Tun>) {
        let tun = v.iter().filter_map(|x| x.1).collect();
        (v.into_iter().map(|x| x.0).collect(), tun)
    };
    if k == NUTZUNG {
        // Raumvorlagen des Projekts, dann die des Werks, die es noch nicht
        // gibt; mit der Wahl kommen sie und ihre Ausbautypen ins Projekt
        for (g, name) in m.space_type_choices() {
            let detail = if m.space_type(g).is_some() {
                String::new()
            } else {
                "kommt mit der Wahl ins Projekt".to_string()
            };
            eintrag(&mut v, name, detail, Tun::Nutzung(g));
        }
        let (zeilen, tun) = fertig(v);
        let cur = raum
            .nutzung
            .and_then(|n| tun.iter().position(|t| *t == Tun::Nutzung(n)));
        return Some((
            format!("Nutzung von {nummer} {}", raum.name),
            zeilen,
            tun,
            cur,
        ));
    }
    if k == SEITE {
        eintrag(
            &mut v,
            "Alle Wandseiten".into(),
            format!("{} Wandseiten im Raum", r.seiten.len()),
            Tun::Seite(None),
        );
        for (w, name) in &r.seiten {
            let eigen: Vec<&str> = [WAND, FLIESEN, SOCKEL]
                .into_iter()
                .filter(|&k| stelle(k, Some(*w)).is_some_and(|st| m.space_finish(id, st).is_some()))
                .map(|k| KNOEPFE[k as usize].2)
                .collect();
            let detail = if eigen.is_empty() {
                "wie alle Wandseiten".to_string()
            } else {
                format!("abweichend: {}", eigen.join(", "))
            };
            eintrag(&mut v, name.clone(), detail, Tun::Seite(Some(*w)));
        }
        let cur = seite.map_or(Some(0), |w| {
            r.seiten.iter().position(|s| s.0 == w).map(|i| i + 1)
        });
        let titel = format!("Wandseite in {nummer} {}", raum.name);
        let (zeilen, tun) = fertig(v);
        return Some((titel, zeilen, tun, cur));
    }
    let st = stelle(k, seite)?;
    let (_, _, flaeche, leer) = KNOEPFE[k as usize];
    let leer_text = |g: Option<Guid>| g.and_then(|g| typ_name(m, g)).unwrap_or(leer.into());
    // Erster Eintrag: ohne eigene Wahl, mit dem, was dann gilt
    let vorlage = aus_vorlage(m, &r, k);
    let vorlage_name = r.vorlage(m).map_or(String::new(), |s| s.name.clone());
    match st.seite() {
        Some(_) => {
            let alle = stelle(k, None).and_then(|x| m.space_finish(id, x)?.wahl);
            let g = match alle {
                Some(AusbauWahl::Keiner) => None,
                Some(AusbauWahl::Typ(g)) => Some(g),
                None => vorlage,
            };
            eintrag(
                &mut v,
                "wie alle Wandseiten".into(),
                leer_text(g),
                Tun::Wahl(None),
            );
        }
        None => eintrag(
            &mut v,
            "wie Raumvorlage".into(),
            format!("{} · {vorlage_name}", leer_text(vorlage)),
            Tun::Wahl(None),
        ),
    }
    eintrag(
        &mut v,
        leer.into(),
        String::new(),
        Tun::Wahl(Some(AusbauWahl::Keiner)),
    );
    // Ausbautypen der Art: erst die des Projekts, dann die des Werks
    let art = st.art();
    let mut eigene: Vec<(String, String, Guid)> = m
        .layer_sets()
        .iter()
        .filter(|(_, t)| t.category == art)
        .map(|(_, t)| (t.name.clone(), t.code.clone(), t.guid))
        .collect();
    eigene.sort();
    let werk: Vec<(String, Guid)> = FINISH_TYPES
        .iter()
        .filter(|t| t.category == art && m.type_by_guid(t.guid).is_none())
        .map(|t| (t.name.to_string(), t.guid))
        .collect();
    if !eigene.is_empty() {
        v.push((Zeile::Gruppe("Im Projekt".into()), None));
    }
    for (name, code, g) in eigene {
        eintrag(&mut v, name, code, Tun::Wahl(Some(AusbauWahl::Typ(g))));
    }
    if !werk.is_empty() {
        v.push((Zeile::Gruppe("Werk".into()), None));
    }
    for (name, g) in werk {
        let detail = "kommt mit der Wahl ins Projekt".to_string();
        eintrag(&mut v, name, detail, Tun::Wahl(Some(AusbauWahl::Typ(g))));
    }
    let jetzt = Tun::Wahl(m.space_finish(id, st).and_then(|a| a.wahl));
    let (zeilen, tun) = fertig(v);
    let cur = tun.iter().position(|t| *t == jetzt);
    let wo = seite.and_then(|w| r.name(w)).map_or_else(
        || format!("{nummer} {}", raum.name),
        |w| format!("{w}, {nummer}"),
    );
    Some((format!("{flaeche} in {wo}"), zeilen, tun, cur))
}

impl Scene {
    /// Ändert die Abweichung an der Stelle des Knopfs `k` im Raum `id`
    /// (Wandseite nach der Wahl im Paneel) als einen Rückgängig-Schritt;
    /// `true`, wenn sich etwas geändert hat.
    pub fn raum_abweichung(
        &mut self,
        id: ElementId,
        k: u8,
        f: impl FnOnce(&mut AusbauAbweichung),
    ) -> bool {
        let m = self.model();
        let seite = Raum::finden(self, id).and_then(|r| r.seite(self));
        let Some(st) = stelle(k, seite) else {
            return false;
        };
        let vorher = m.space_finish(id, st).copied().unwrap_or(AusbauAbweichung {
            stelle: st,
            wahl: None,
            hoehe: None,
        });
        let mut a = vorher;
        f(&mut a);
        if a == vorher {
            return false;
        }
        self.raeume_aendern("Raumbuch ändern", |m| m.set_space_finish(id, a).is_ok())
    }

    /// Nutzung des Raums `id`: die Raumvorlage `g`, mit ihren Ausbautypen
    /// ins Projekt (wie ein Name, der sie nennt), als ein Rückgängig-Schritt;
    /// `true`, wenn sich etwas geändert hat.
    pub fn raum_nutzung(&mut self, id: ElementId, g: Guid) -> bool {
        if self.model().space(id).is_none_or(|s| s.nutzung == Some(g)) {
            return false;
        }
        self.raeume_aendern("Nutzung gewählt", |m| {
            let Some(n) = m.ensure_space_type(g) else {
                return false;
            };
            m.set_space(id, |s| s.nutzung = Some(n)) && ausbau::typen_anlegen(m, id)
        })
    }

    /// Feld „Höhe“ der Wandfliesen im Paneel des Raums.
    pub(crate) fn raum_fliesenhoehe(&mut self, id: ElementId, mm: f64) -> bool {
        self.raum_abweichung(id, FLIESEN, |a| a.hoehe = Some(mm))
    }
}

impl App {
    /// Liste am Knopf `k` des gewählten Raums.
    pub(crate) fn raum_liste(&self, k: u8) -> Option<(String, Vec<Zeile>, Option<usize>)> {
        let (t, z, _, cur) = liste(&self.scene, self.sel.id?, k)?;
        Some((t, z, cur))
    }

    /// Eintrag `i` der Liste am Knopf `k`: Wandseite wählen oder die
    /// Abweichung setzen.
    pub(crate) fn raum_waehlen(&mut self, k: u8, i: usize) {
        let Some(id) = self.sel.id else {
            return;
        };
        let Some(t) = liste(&self.scene, id, k).and_then(|l| l.2.get(i).copied()) else {
            return;
        };
        match t {
            Tun::Seite(s) => {
                self.scene.raum_seite_setzen(s.map(|s| (id, s)));
                self.props_key = None;
            }
            Tun::Wahl(_) | Tun::Nutzung(_) => {
                if let Some(b) = sk_model::edit_blocked(self.scene.model(), &[id]) {
                    self.show_locked(b, Some((id, crate::delete::Act::Field)));
                    return;
                }
                let neu = match t {
                    Tun::Nutzung(g) => self.scene.raum_nutzung(id, g),
                    Tun::Wahl(w) => self.scene.raum_abweichung(id, k, |a| a.wahl = w),
                    Tun::Seite(_) => false,
                };
                if neu {
                    self.upload_model();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::model::{LIME_WALL_TYPE_GUID, WALL_TILES_TYPE_GUID};

    /// Haus 10 × 8 m mit zwei Geschossen; der Raum im EG als Wohnzimmer
    /// mit seinen Ausbautypen.
    fn haus() -> (Scene, ElementId) {
        let mut s = Scene::with_model(Model::with_seed(85));
        assert!(s.edit_model("Haus", |m| {
            let b = m.add_building(2);
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.build_from_polygon(b, &pts).is_some()
        }));
        let m = s.model();
        let eg = m.levels_in(m.buildings().iter().next().map(|b| b.0))[1];
        let r = m.spaces(eg)[0];
        assert!(s.edit_model("Ausbau", |m| ausbau::typen_anlegen(m, r)));
        (s, r)
    }

    fn knoepfe(s: &Scene, r: ElementId) -> Vec<(String, Vec<&'static str>)> {
        let p = crate::selection::props(s, r).unwrap();
        p.sections
            .iter()
            .map(|x| (x.title.to_string(), x.buttons.iter().map(|b| b.1).collect()))
            .collect()
    }

    fn namen(z: &[Zeile]) -> Vec<String> {
        z.iter()
            .map(|z| match z {
                Zeile::Gruppe(g) => format!("[{g}]"),
                Zeile::Eintrag { name, .. } => name.clone(),
            })
            .collect()
    }

    /// Ohne Abweichung: Knöpfe je Fläche, Liste mit Vorlage, „ohne“ und
    /// den Typen; „ohne Belag“ macht den Boden abweichend, Rückgängig nimmt
    /// es zurück.
    #[test]
    fn flaeche_im_raum_waehlen() {
        let (mut s, r) = haus();
        let k = knoepfe(&s, r);
        assert_eq!(
            k[0],
            (
                "Raumbuch".into(),
                vec!["Nutzung …", "Boden …", "Estrich …", "Decke …"]
            )
        );
        assert_eq!(
            k[1],
            (
                "Wandseiten".into(),
                vec![
                    "Alle Wandseiten …",
                    "Wand …",
                    "Wandfliesen …",
                    "Sockelleiste …"
                ]
            )
        );
        assert_eq!(k.len(), 2, "keine Fliesen im Wohnzimmer");
        let (titel, z, tun, cur) = liste(&s, r, BODEN).unwrap();
        assert_eq!(titel, "Boden in R-001 Raum 1");
        let n = namen(&z);
        assert_eq!(&n[..3], ["wie Raumvorlage", "ohne Belag", "[Im Projekt]"]);
        assert!(n.contains(&"Parkett Eiche 20".to_string()), "{n:?}");
        assert!(n.contains(&"[Werk]".to_string()), "{n:?}");
        assert_eq!(cur, Some(0));
        assert_eq!(
            tun.len(),
            z.iter()
                .filter(|z| matches!(z, Zeile::Eintrag { .. }))
                .count()
        );
        let Zeile::Eintrag { detail, .. } = &z[0] else {
            panic!()
        };
        assert_eq!(detail, "Parkett Eiche 20 · Wohnzimmer");

        let vorher = crate::selection::props(&s, r).unwrap().values;
        assert!(s.raum_abweichung(r, BODEN, |a| a.wahl = Some(AusbauWahl::Keiner)));
        assert!(!s.raum_abweichung(r, BODEN, |a| a.wahl = Some(AusbauWahl::Keiner)));
        assert_eq!(knoepfe(&s, r)[0].1[1], "Boden: abweichend …");
        assert_eq!(liste(&s, r, BODEN).unwrap().3, Some(1));
        let werte = crate::selection::props(&s, r).unwrap().values;
        assert!(werte.iter().all(|v| v.0 != "Boden"), "{werte:?}");
        assert!(s.undo());
        assert_eq!(crate::selection::props(&s, r).unwrap().values, vorher);
        assert_eq!(knoepfe(&s, r)[0].1[1], "Boden …");
    }

    /// Kalkputz an allen Wandseiten, an einer Wandseite Fliesen 2,00 m: die
    /// Wandseite wird über ihren Knopf gewählt, die Höhe über das Feld.
    #[test]
    fn wandseite_waehlen() {
        let (mut s, r) = haus();
        let kalk = liste(&s, r, WAND)
            .unwrap()
            .2
            .into_iter()
            .position(|t| t == Tun::Wahl(Some(AusbauWahl::Typ(LIME_WALL_TYPE_GUID))))
            .unwrap();
        assert!(kalk > 2);
        assert!(s.raum_abweichung(r, WAND, |a| a.wahl =
            Some(AusbauWahl::Typ(LIME_WALL_TYPE_GUID))));
        let (titel, z, tun, cur) = liste(&s, r, SEITE).unwrap();
        assert_eq!(titel, "Wandseite in R-001 Raum 1");
        assert_eq!(z.len(), 5, "{:?}", namen(&z));
        assert_eq!(cur, Some(0));
        let Tun::Seite(Some(w)) = tun[2] else {
            panic!("{tun:?}")
        };
        let name = namen(&z)[2].clone();
        assert!(name.starts_with("Wand AW-"), "{name}");
        s.raum_seite_setzen(Some((r, w)));
        let k = knoepfe(&s, r);
        assert_eq!(k[1].1[0], format!("{name} …"));
        assert_eq!(
            k[1].1[1], "Wand …",
            "an der Wandseite selbst keine Abweichung"
        );
        let (titel, z, _, cur) = liste(&s, r, WAND).unwrap();
        assert_eq!(titel, format!("Wand in {name}, R-001"));
        assert_eq!(namen(&z)[0], "wie alle Wandseiten");
        assert_eq!(cur, Some(0));
        let Zeile::Eintrag { detail, .. } = &z[0] else {
            panic!()
        };
        assert_eq!(detail, "Kalkzementputz mit Anstrich");

        let fliesen = Some(AusbauWahl::Typ(WALL_TILES_TYPE_GUID));
        assert!(s.raum_abweichung(r, FLIESEN, |a| a.wahl = fliesen));
        let p = crate::selection::props(&s, r).unwrap();
        let feld = &p.sections[2].fields[0];
        assert_eq!(
            (feld.field, feld.value),
            (Field::RaumFliesen, TILES_DEFAULT)
        );
        assert!(s.set_field(r, Field::RaumFliesen, 2000.0));
        let a = s
            .model()
            .space_finish(r, AusbauStelle::Fliesen(Some(w)))
            .unwrap();
        assert_eq!((a.wahl, a.hoehe), (fliesen, Some(2000.0)));
        assert!(s
            .model()
            .space_finish(r, AusbauStelle::Fliesen(None))
            .is_none());
        assert_eq!(knoepfe(&s, r)[1].1[3], "Sockelleiste …", "keine eigene");
        let d = &liste(&s, r, SEITE).unwrap().1[2];
        assert!(matches!(d, Zeile::Eintrag { detail, .. } if detail == "abweichend: Wandfliesen"));

        // Alle Wandseiten: das Feld nennt die Höhe des Raums
        s.raum_seite_setzen(None);
        let p = crate::selection::props(&s, r).unwrap();
        assert_eq!(p.sections[2].fields[0].value, TILES_DEFAULT);
        assert!(s.undo() && s.undo());
        assert!(s.model().space(r).unwrap().ausbau.len() == 1);
    }

    /// „Nutzung …“ listet die Raumvorlagen; Bad gewählt bringt die Vorlage
    /// und ihre Ausbautypen ins Projekt, Rückgängig nimmt alles zurück.
    #[test]
    fn nutzung_im_raum_waehlen() {
        use sk_model::model::BATH_SPACE_GUID;
        let (mut s, r) = haus();
        let vorher = crate::selection::props(&s, r).unwrap().values;
        let typen = s.model().space_types().len();
        let (titel, z, tun, cur) = liste(&s, r, NUTZUNG).unwrap();
        assert_eq!(titel, "Nutzung von R-001 Raum 1");
        assert_eq!(z.len(), tun.len());
        let bad = tun
            .iter()
            .position(|t| *t == Tun::Nutzung(BATH_SPACE_GUID))
            .unwrap();
        assert_eq!(namen(&z)[bad], "Bad");
        assert!(s.model().space_type(BATH_SPACE_GUID).is_none());
        let Zeile::Eintrag { detail, .. } = &z[bad] else {
            panic!()
        };
        assert_eq!(detail, "kommt mit der Wahl ins Projekt");
        assert_ne!(cur, Some(bad));

        assert!(s.raum_nutzung(r, BATH_SPACE_GUID));
        assert!(!s.raum_nutzung(r, BATH_SPACE_GUID), "schon Bad");
        assert_eq!(s.model().space(r).unwrap().nutzung, Some(BATH_SPACE_GUID));
        assert!(s.model().space_type(BATH_SPACE_GUID).is_some());
        assert_eq!(liste(&s, r, NUTZUNG).unwrap().3, Some(bad));
        assert_eq!(knoepfe(&s, r).len(), 3, "Fliesen im Bad");
        let werte = crate::selection::props(&s, r).unwrap().values;
        assert!(werte.iter().any(|v| v.1.contains("Bad")), "{werte:?}");

        assert!(s.undo());
        assert_eq!(crate::selection::props(&s, r).unwrap().values, vorher);
        assert!(s.model().space_type(BATH_SPACE_GUID).is_none());
        assert_eq!(s.model().space_types().len(), typen);
    }
}
