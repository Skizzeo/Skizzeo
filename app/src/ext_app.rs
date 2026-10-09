//! Werkzeug „Erweiterungen“ in der App (E6): Knopf und Liste im Paneel
//! „Werkzeuge“, Typ und Felder, Vorschau beim Setzen und das Einsetzen ins
//! Modell als ein Rückgängig-Schritt je Bauteil. Die Eingabe selbst läuft
//! im Wandwerkzeug ([`crate::wall_tool`]), was ein Bauteil daraus wird, in
//! [`crate::ext_werkzeug`].

use crate::auswahl::{Auswahl, Hit, Zeile};
use crate::ext_werkzeug::ExtModus;
use crate::scene::Scene;
use crate::ui::{self, ExtPanel, Field, FieldRow, Id, Panel};
use crate::{meldung, App, ViewKind, NOTICE_TIME};
use sk_model::erweiterung::{anzeige, ExtDef, Geschoss};
use sk_platform::{Event, Key, MouseButton};

/// Ordner der Erweiterungen neben den Einstellungen:
/// `<Einstellungen>/Erweiterungen` ([`crate::ext_ablage`]).
pub const ORDNER: &str = "Erweiterungen";

/// Längster Text aus einer .szb im Werkzeug.
const MAX: usize = 60;

impl App {
    /// Knopf „Erweiterungen“ bzw. Typ-Chip des Werkzeugs.
    pub(crate) fn ext_klick(&mut self, id: Id) {
        if id == Id::Ext && self.tool.ext.is_some() {
            // Der Knopf des Bauteils beendet das Werkzeug
            self.tool.set_enabled(false);
            self.refresh_cursor();
            self.ext_sync();
            return;
        }
        let again = self.ext_liste.as_ref().is_some_and(|l| l.knopf == id);
        self.ext_liste_zu();
        if !again {
            self.ext_liste_auf(id);
        }
    }

    fn ext_liste_auf(&mut self, knopf: Id) {
        self.close_type_menu(false);
        let top = self.top();
        let Some(anchor) = self.ui.button_rect(knopf, self.w, top) else {
            return;
        };
        let (titel, zeilen, aktuell, fuss) = match knopf {
            // Paneel „Eigenschaften“ (E7)
            Id::PropsType | Id::ExtListe(_) => {
                let Some((t, z, cur)) = self.ext_prop_liste(knopf) else {
                    return;
                };
                (t, z, cur, String::new())
            }
            Id::ExtTyp => {
                let Some(m) = self.tool.ext.as_ref() else {
                    return;
                };
                let zeilen: Vec<Zeile> = m
                    .def
                    .def
                    .typ
                    .iter()
                    .map(|t| Zeile::Eintrag {
                        name: anzeige(t.get("name").unwrap_or(t.key()), MAX),
                        detail: anzeige(&t.get("werte").unwrap_or("").replace(';', " ·"), MAX),
                    })
                    .collect();
                let cur = m
                    .vorlage
                    .typ
                    .as_deref()
                    .and_then(|k| m.def.def.typ.iter().position(|t| t.key() == k));
                ("Typ".to_string(), zeilen, cur, String::new())
            }
            _ => {
                let mut zeilen = Vec::new();
                for (g, ix) in self.ext_bib.gruppen() {
                    zeilen.push(Zeile::Gruppe(g.to_string()));
                    for i in ix {
                        let d = &self.ext_bib.defs[i];
                        zeilen.push(Zeile::Eintrag {
                            name: anzeige(d.name(), MAX),
                            detail: format!("{} · Version {}", d.key, d.version),
                        });
                    }
                }
                let fuss = if zeilen.is_empty() {
                    "Noch keine Erweiterung eingelesen.".to_string()
                } else {
                    "Klick wählt das Bauteil zum Setzen.".to_string()
                };
                ("Erweiterungen".to_string(), zeilen, None, fuss)
            }
        };
        let panel = match knopf {
            Id::PropsType | Id::ExtListe(_) => Panel::Props,
            _ => Panel::Tools,
        };
        let panel = self.ui.rect(panel, self.w, top);
        self.ext_liste = Some(Auswahl::new(
            knopf,
            titel,
            zeilen,
            aktuell,
            fuss,
            anchor,
            panel,
            self.ui.scale,
            (self.w as f32, self.h as f32),
        ));
        if matches!(knopf, Id::ExtTyp | Id::PropsType) && self.ui.set_chip_open(knopf, true) {
            self.dirty_buttons.push(knopf);
        }
        self.type_menu_dirty = true;
        self.tip = None;
    }

    pub(crate) fn ext_liste_zu(&mut self) {
        if let Some(l) = self.ext_liste.take() {
            if self.ui.set_chip_open(l.knopf, false) {
                self.dirty_buttons.push(l.knopf);
            }
            self.type_menu_dirty = true;
        }
    }

    /// Eintrag `i` der offenen Liste: Bauteil zum Setzen bzw. Typ.
    fn ext_waehlen(&mut self, i: usize) {
        let Some(knopf) = self.ext_liste.as_ref().map(|l| l.knopf) else {
            return;
        };
        self.ext_liste_zu();
        match knopf {
            Id::PropsType => self.ext_prop_typ(i),
            Id::ExtListe(j) => self.ext_prop_wert(j, Some(i as u8)),
            Id::ExtTyp => {
                if let Some(m) = self.tool.ext.as_mut() {
                    if let Some(k) = m.def.def.typ.get(i).map(|t| t.key().to_string()) {
                        m.set_typ(&k);
                    }
                }
            }
            _ => {
                let order: Vec<usize> = self
                    .ext_bib
                    .gruppen()
                    .into_iter()
                    .flat_map(|g| g.1)
                    .collect();
                let Some(d) = order.get(i).and_then(|&i| self.ext_bib.defs.get(i)) else {
                    return;
                };
                self.ext_start(d.clone());
            }
        }
        self.ext_sync();
        self.redraw = true;
    }

    /// Beginnt das Setzen von `d` (Grundriss oder 3D).
    pub(crate) fn ext_start(&mut self, d: ExtDef) {
        if !self.tool_allowed() {
            self.set_view(ViewKind::Persp);
        }
        self.nord.set_aktiv(false);
        self.tool.start_ext(ExtModus::new(d));
        self.refresh_cursor();
        self.ext_sync();
    }

    /// „Einfügen“ im Bauteilkatalog: das Bauteil aus dem Ordner, sonst
    /// das, welches das Projekt mitbringt.
    pub(crate) fn ext_start_key(&mut self, key: &str) {
        let d = self.ext_bib.defs.iter().find(|d| d.key == key).cloned();
        let d = d.or_else(|| self.scene.model().ext_def(key).cloned());
        if let Some(d) = d {
            self.ext_start(d);
            self.redraw = true;
        }
    }

    /// Offene Liste: nimmt Maus und Tasten; ein Klick daneben schließt sie.
    pub(crate) fn handle_ext_liste(&mut self, e: Event) -> bool {
        let Some(l) = self.ext_liste.as_mut() else {
            return false;
        };
        match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let h = match l.hit(x, y) {
                    Hit::Eintrag(i) => Some(i),
                    _ => None,
                };
                if h != l.hover {
                    l.hover = h;
                    self.type_menu_dirty = true;
                }
                true
            }
            Event::MouseDown { x, y, .. } => {
                if l.hit(x, y) == Hit::Outside {
                    self.ext_liste_zu();
                }
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Hit::Eintrag(i) = l.hit(x, y) {
                    self.ext_waehlen(i);
                }
                true
            }
            Event::Key {
                key, down: true, ..
            } => {
                match key {
                    Key::Escape => self.ext_liste_zu(),
                    // Pfeiltasten ab und auf (wie die Typ-Liste)
                    Key::Other(0x26 | 0x28) => {
                        l.step(key == Key::Other(0x28));
                        self.type_menu_dirty = true;
                    }
                    Key::Enter => match l.hover.or(l.aktuell) {
                        Some(i) => self.ext_waehlen(i),
                        None => self.ext_liste_zu(),
                    },
                    _ => {}
                }
                true
            }
            Event::Key { .. } | Event::MouseUp { .. } | Event::Wheel { .. } => true,
            _ => false,
        }
    }

    /// Bezugsgeschoss des aktiven Geschosses für Felder und Vorschau.
    fn ext_geschoss(&self) -> Geschoss {
        self.scene.model().ext_geschoss(self.scene.active_storey())
    }

    /// Paneel „Werkzeuge“ an das laufende Werkzeug angleichen.
    pub(crate) fn ext_sync(&mut self) {
        let g = self.ext_geschoss();
        let offen = self
            .ext_liste
            .as_ref()
            .is_some_and(|l| l.knopf == Id::ExtTyp);
        let scene = &mut self.scene;
        let panel = self.tool.ext.as_ref().map(|m| panel(m, &g, offen, scene));
        let knopf = !self.ext_bib.defs.is_empty();
        if panel != self.ui.ext_tool || knopf != self.ui.ext_knopf {
            self.ui.ext_tool = panel;
            self.ui.ext_knopf = knopf;
            self.overlay_dirty = true;
        }
    }

    /// Feld `i` des Werkzeugs: eigener Wert der nächsten Bauteile.
    pub(crate) fn ext_feld(&mut self, i: u8, v: f64) {
        let g = self.ext_geschoss();
        if let Some(m) = self.tool.ext.as_mut() {
            if let Some(k) = m.felder(&g).get(i as usize).map(|f| f.key.clone()) {
                m.vorlage.set(&k, v);
            }
        }
        self.ext_sync();
        self.redraw = true;
    }

    /// Gesetzte Bauteile ins Modell, je eins ein Rückgängig-Schritt. Die
    /// Definition kommt mit dem ersten Exemplar ins Projekt.
    pub(crate) fn ext_einsetzen(&mut self) {
        let teile = std::mem::take(&mut self.tool.ext_fertig);
        let Some(m) = self.tool.ext.as_ref() else {
            return;
        };
        if teile.is_empty() {
            return;
        }
        let def = m.def.clone();
        let storey = self.scene.active_storey();
        let label = self
            .scene
            .bezeichnung(format!("{} setzen", anzeige(def.name(), MAX)));
        for t in teile {
            let mut fehler = None;
            self.scene.edit_model(label, |md| {
                if md.ext_def(&def.key).is_none() {
                    if let Err(e) = md.put_ext_def(def.clone()) {
                        fehler = Some(e.to_string());
                        return false;
                    }
                }
                match md.add_ext(storey, t) {
                    Ok(_) => true,
                    Err(e) => {
                        fehler = Some(e.to_string());
                        false
                    }
                }
            });
            if let Some(f) = fehler {
                self.scene.undo();
                self.status(meldung::Meldung::mit("{}", &[&f]), NOTICE_TIME);
                break;
            }
        }
        self.sect.ensure(&self.scene);
        self.upload_model();
        self.refresh_cursor();
    }

    /// Netz des Bauteils, wie es gerade am Cursor entsteht.
    pub(crate) fn ext_vorschau(&self) -> Option<sk_render::MeshData> {
        let m = self.tool.ext.as_ref()?;
        let t = self.tool.ext_vorschau()?;
        let g = self.ext_geschoss();
        let (e, _) = sk_model::erweiterung_koerper::begrenzt(m.rechnen(&t, &g));
        let st = self.scene.active_storey();
        let z = self.scene.model().storey(st).map_or(0.0, |s| s.elevation) + e.z0;
        let lage = sk_model::erweiterung_koerper::Lage {
            at: t.at,
            rot: t.rot,
            z,
        };
        let s = sk_model::erweiterung_koerper::solid(&e, &lage, &|_| 0);
        let s = if self.ui.view == ViewKind::Plan {
            let cut = self.scene.plan_cut();
            sk_model::erweiterung_koerper::geschnitten(
                &s,
                &e,
                &lage,
                &|_| 0,
                sk_math::vec3(0.0, 0.0, cut),
                sk_math::vec3(0.0, 0.0, 1.0),
            )
        } else {
            s
        };
        Some(crate::scene::mesh_of(&s))
    }

    /// Liste zeichnen (im Platz der Typ-Liste, die dann zu ist).
    pub(crate) fn ext_liste_bild(&self) -> Option<(sk_paint::Canvas, i32, i32)> {
        let l = self.ext_liste.as_ref()?;
        Some(l.paint(&self.theme, &self.ui.fonts))
    }
}

/// Paneel „Werkzeuge“ des Werkzeugs `m` (Vertrag §14); `offen`: die
/// Typ-Liste ist offen.
pub(crate) fn panel(m: &ExtModus, g: &Geschoss, offen: bool, scene: &mut Scene) -> ExtPanel {
    let felder = m
        .felder(g)
        .iter()
        .enumerate()
        .map(|(i, f)| FieldRow {
            field: Field::ExtTool(i as u8),
            label: scene.bezeichnung(anzeige(&f.name, MAX)),
            value: f.wert,
            min: f.min.unwrap_or(f64::NEG_INFINITY),
            max: f.max.unwrap_or(f64::INFINITY),
            zero: false,
            einheit: Some(scene.bezeichnung(anzeige(&f.einheit, MAX))),
        })
        .collect();
    let chip = (!m.def.def.typ.is_empty()).then(|| {
        let t = m.vorlage.typ.as_deref().and_then(|k| m.def.typ(k));
        ui::Chip {
            name: t.map_or("ohne Typ".to_string(), |t| {
                anzeige(t.get("name").unwrap_or(t.key()), MAX)
            }),
            detail: "Typ".to_string(),
            look: None,
            open: offen,
            marked: m.def.ueberschreibt(&m.vorlage),
        }
    });
    ExtPanel {
        name: scene.bezeichnung(anzeige(m.def.name(), MAX)),
        chip,
        felder,
        aus_eingabe: m.aus_eingabe(g).map_or(Vec::new(), |t| {
            // „Breite und Tiefe aus der Eingabe“ passt nicht in eine Zeile
            match t.strip_suffix(" aus der Eingabe") {
                Some(n) if t.chars().count() > 26 => {
                    vec![scene.bezeichnung(n.into()), "aus der Eingabe"]
                }
                _ => vec![scene.bezeichnung(t)],
            }
        }),
        hinweise: m.art.hinweise(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auswahl::Zeile;
    use crate::catalog_view::{Catalog, Tab};
    use crate::ext_werkzeug::Bibliothek;
    use crate::prefs::Win;
    use sk_paint::Canvas;
    use sk_ui::theme::Theme;
    use sk_ui::widgets::{Fonts, Rect};

    const BEISPIELE: [&str; 5] = [
        include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.treppe.szb"),
        include_str!("../../crates/sk-szb/beispiele/werk.streifenfundament.szb"),
    ];

    fn schriften() -> Fonts {
        let f = Fonts::system();
        if f.regular.is_some() {
            return f;
        }
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        let lade = |n: &str| {
            std::fs::read(lib.join(n))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        Fonts {
            regular: lade("LiberationSans-Regular.ttf"),
            bold: lade("LiberationSans-Bold.ttf"),
            italic: lade("LiberationSans-Italic.ttf"),
        }
    }

    /// Ist-Bilder E6 neben der Werkbank (Reiter „Einfügen“): Paneel je
    /// Beispiel, Werkzeuge mit Knopf, Liste und Bauteilkatalog. Nur auf
    /// Wunsch: `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo
    /// istbilder_e6 -- --ignored`
    #[test]
    #[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
    fn istbilder_e6() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let fonts = schriften();
        if fonts.regular.is_none() {
            return;
        }
        std::fs::create_dir_all(&ziel).unwrap();
        let t = Theme::dark();
        let defs: Vec<ExtDef> = BEISPIELE
            .iter()
            .map(|x| ExtDef::einlesen(x).unwrap())
            .collect();
        let mut scene = Scene::new();
        let mut ui = ui::Ui::new(1.0, &t);
        ui.fit(1.0, 1440, 900);
        ui.fonts = schriften();
        // Vier Paneele nebeneinander wie in der Werkbank, dazu das ohne
        // Werkzeug mit dem Knopf „Erweiterungen“
        let mut bilder = Vec::new();
        ui.ext_knopf = true;
        let (c, ..) = ui.paint(&t, Panel::Tools, 1440, 32);
        bilder.push(c.clone());
        for d in &defs[..4] {
            let m = ExtModus::new(d.clone());
            ui.ext_tool = Some(panel(&m, &Geschoss::PROBE, false, &mut scene));
            let (c, ..) = ui.paint(&t, Panel::Tools, 1440, 32);
            bilder.push(c.clone());
        }
        let breite: usize = bilder.iter().map(|c| c.width + 20).sum::<usize>() + 20;
        let hoehe = bilder.iter().map(|c| c.height).max().unwrap() + 40;
        let mut m = Canvas::new(breite, hoehe);
        m.fill_rect(0.0, 0.0, breite as f32, hoehe as f32, t.ui.bg);
        let mut x = 20;
        for c in &bilder {
            m.blit(c, x as i32, 20);
            x += c.width + 20;
        }
        std::fs::write(ziel.join("ist-e6-werkzeug.png"), m.to_png()).unwrap();
        // Liste am Knopf „Erweiterungen“
        let mut bib = Bibliothek::default();
        for d in &defs {
            bib.dazu(d.clone());
        }
        let mut zeilen = Vec::new();
        for (g, ix) in bib.gruppen() {
            zeilen.push(Zeile::Gruppe(g.to_string()));
            for i in ix {
                let d = &bib.defs[i];
                zeilen.push(Zeile::Eintrag {
                    name: d.name().to_string(),
                    detail: format!("{} · Version {}", d.key, d.version),
                });
            }
        }
        let a = Auswahl::new(
            Id::Ext,
            "Erweiterungen".into(),
            zeilen,
            None,
            String::new(),
            Rect::new(46.0, 236.0, 178.0, 40.0),
            Rect::new(32.0, 84.0, 206.0, 300.0),
            1.0,
            (1440.0, 900.0),
        );
        let (c, ..) = a.paint(&t, &fonts);
        std::fs::write(ziel.join("ist-e6-liste.png"), c.to_png()).unwrap();
        // Bauteilkatalog, Reiter „Erweiterungen“
        let w = Win {
            w: 1280,
            h: 800,
            top: 32,
            scale: 1.0,
        };
        let mut k = Catalog::open(&scene, None);
        k.set_ext(&bib.defs);
        k.tab = Tab::Ext;
        let (c, ..) = k.paint(&t, &fonts, &w);
        std::fs::write(ziel.join("ist-e6-katalog.png"), c.to_png()).unwrap();
    }
}
