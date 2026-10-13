//! Werkzeug „Wandöffnung“ und „Türöffnung“ in der App (Paket O3): Knopf,
//! Maus, Esc. Die Rechnung (Fangen, Öffnung, Vorschau) steht in
//! [`werkzeug`], das Werkzeug selbst im
//! [`WallTool`](crate::wall_tool::WallTool), der auch die Vorschau zeichnet.
//! Im Grundriss setzt „Tür“ über einer Innenwand eine Innentür
//! ([`innentuer`]); über der Außenwand zieht man dort Fenster und Außentür
//! mit zwei Klicks auf (Jörn 12.10. 09:26).

#[path = "innentuer.rs"]
pub(crate) mod innentuer;
#[path = "oeffnung_werkzeug.rs"]
pub(crate) mod werkzeug;

use crate::meldung::Meldung;
use crate::ui::ViewKind;
use crate::App;
use sk_math::vec3;
use sk_model::oeffnung::OpeningKind;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use werkzeug::{fangen, pfad, plan_fang, Bild, Fang, Klick, Werkzeug};

/// Fangabstand an Höhenlinien und Wandenden in Bildschirmpixeln (96 dpi).
const FANG_PX: f64 = 10.0;

/// Ansichten, in denen das Werkzeug arbeitet: Außenwandflächen in 3D und
/// den Ansichten, Außen- und Innenwände im Grundriss (dort bleibt der
/// Grundriss stehen, Jörn 12.10. 09:26), nicht im Schnitt.
fn ansicht_ok(v: ViewKind) -> bool {
    v != ViewKind::Section
}

impl App {
    /// Knopf „Fenster“ (Wandöffnung) bzw. „Tür“ (Türöffnung): schaltet
    /// das Werkzeug der Art ein, derselbe Knopf aus. Im Schnitt geht es nach
    /// 3D, der Grundriss bleibt.
    pub(crate) fn oeffnung_klick(&mut self, kind: OpeningKind) {
        let an = self.tool.oeffnung.as_ref().is_none_or(|w| w.kind != kind);
        if an && self.ui.foundation_active {
            return;
        }
        self.tool.set_enabled(false);
        self.tool.oeffnung = an.then(|| Werkzeug::new(kind));
        if an {
            self.nord.set_aktiv(false);
            if !ansicht_ok(self.ui.view) {
                self.set_view(ViewKind::Persp);
            }
        }
        self.ext_sync();
        self.refresh_cursor();
        self.redraw = true;
    }

    /// Ecke unter der Maus (Ansichtspixel `x`, `y`): Außenwandfläche
    /// getroffen und gefangen.
    fn oeffnung_fang(&self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) -> Option<Fang> {
        let (o, d) = self.cam.ray(x, y, vw, vh);
        let (face, p) = self.scene.outer_face_at(o, d)?;
        let m = self.scene.model();
        let elevation = m.storey(face.storey)?.elevation;
        // Fangabstand in mm: 100 mm senkrecht an dieser Stelle in Pixeln
        let px = |q| self.cam.project(q, vw, vh);
        let tol = match (px(p), px(p + vec3(0.0, 0.0, 100.0))) {
            (Some(a), Some(b)) => {
                let n = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                FANG_PX * sc * 100.0 / n.max(1e-6)
            }
            _ => 0.0,
        };
        Some(fangen(&face, &m.face_lines(face.storey), elevation, p, tol))
    }

    /// Kante unter der Maus im Grundriss: auf der Außenwand des aktiven
    /// Geschosses, in Schnitthöhe. Mit gesetzter erster Kante folgt die
    /// Kante sonst der Flucht der ersten (die Maus darf in den Raum gehen).
    fn oeffnung_plan_fang(&self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) -> Option<Fang> {
        let z = self.scene.plan_cut();
        let at = |dx: f64| self.cam.plane_point(x + dx, y, vw, vh, z);
        let (p, q) = (at(0.0)?, at(FANG_PX * sc)?);
        let tol = (q - p).length();
        let st = self.scene.active_storey();
        let scene = &self.scene;
        let hit = innentuer::treffer_aussen(scene.model(), &|r| scene.koerper(r), st, p, tol)
            .map(|(face, _, _, dicke)| plan_fang(&face, p, tol, z, dicke));
        let erste = self.tool.oeffnung.as_ref()?.erste.as_ref();
        match (hit, erste) {
            (Some(h), Some(a)) if pfad(a, &h).is_none() => {
                Some(plan_fang(&a.face, p, tol, z, a.dicke))
            }
            (None, Some(a)) => Some(plan_fang(&a.face, p, tol, z, a.dicke)),
            (hit, _) => hit,
        }
    }

    /// Mausbewegung (Ansichtskoordinaten); `MouseLeave` über Paneelen und
    /// Bändern nimmt die Ecke weg.
    pub(crate) fn oeffnung_move(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) {
        let plan = self.ui.view == ViewKind::Plan;
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return;
        };
        w.ansicht(plan);
        // Über einer Innenwand gilt die Innentür; die Außenwand nur, wenn
        // keine Innenwand trifft oder die erste Kante schon steht
        if self.innentuer_an() {
            match *ev {
                Event::MouseMove { x, y, .. } => self.tuer_suchen(x, y, vw, vh, sc),
                Event::MouseLeave => self.tuer_bild(|t, _, _, _| t.weg()),
                _ => return,
            }
            if self.tuer_hover() {
                if let Some(w) = self.tool.oeffnung.as_mut() {
                    w.hover = None;
                }
                return;
            }
        }
        let hover = match *ev {
            Event::MouseMove { x, y, .. } if plan => self.oeffnung_plan_fang(x, y, vw, vh, sc),
            Event::MouseMove { x, y, .. } if ansicht_ok(self.ui.view) => {
                self.oeffnung_fang(x, y, vw, vh, sc)
            }
            Event::MouseMove { .. } | Event::MouseLeave => None,
            _ => return,
        };
        self.oeffnung_vorschau(hover);
    }

    /// Ecke unter der Maus setzen und die Vorschau neu rechnen.
    fn oeffnung_vorschau(&mut self, hover: Option<Fang>) {
        let m = self.scene.model();
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return;
        };
        let alt = w.vorschau.clone();
        w.hover = hover;
        w.vorschau = w.bild(|f| m.face_lines(f.storey));
        self.redraw |= w.vorschau != alt;
    }

    /// Linksklick über einer Außenwand: erste Ecke, dann die Öffnung als
    /// ein Schritt. `true`, wenn das Werkzeug den Klick nahm.
    pub(crate) fn oeffnung_down(&mut self, ev: &Event) -> bool {
        let Event::MouseDown {
            button: MouseButton::Left,
            ..
        } = *ev
        else {
            return false;
        };
        if self.innentuer_an() && self.tuer_hover() {
            return self.tuer_setzen();
        }
        let m = self.scene.model();
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return false;
        };
        if w.hover.is_none() {
            return false;
        }
        // Lehnt das Modell ab, bleibt die erste Ecke und die Statuszeile
        // sagt warum (Review #86, H4)
        match w.setzen(m) {
            Klick::Oeffnung(o) => {
                if self.scene.add_opening(o).is_some() {
                    self.upload_model();
                }
            }
            Klick::Abgelehnt(t) => self.status(Meldung::mit("{}.", &[&t]), crate::NOTICE_TIME),
            Klick::Nichts | Klick::Erste => {}
        }
        let hover = self.tool.oeffnung.as_ref().and_then(|w| w.hover.clone());
        self.oeffnung_vorschau(hover);
        self.redraw = true;
        true
    }

    /// Nach Rückgängig oder Wiederherstellen (Review #86, H5): Die erste Ecke
    /// und die Vorschau zeigten auf Flächen, die es so nicht mehr gibt.
    pub(crate) fn oeffnung_neu(&mut self) {
        if let Some(w) = self.tool.oeffnung.as_mut() {
            w.neu();
            self.redraw = true;
        }
    }

    /// Fokusverlust: wie nach einem Schritt, die Maus ist fort.
    pub(crate) fn oeffnung_fokus(&mut self, aktiv: bool) {
        if !aktiv {
            self.oeffnung_neu();
        }
    }

    /// Taste gedrückt: Ziffern, **Enter** und **Esc** zuerst an die
    /// Eingabe des Abstands der Innentür, sonst **Esc** wie
    /// [`App::oeffnung_esc`], **Tab** schaltet den Bezugspunkt der
    /// Innentür. `true`, wenn das Werkzeug sie nahm.
    pub(crate) fn oeffnung_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        if self.innentuer_an() {
            let mut taste = innentuer::Taste::Frei;
            self.tuer_bild(|t, _, _, _| taste = t.taste(key, mods));
            match taste {
                innentuer::Taste::Frei => {}
                innentuer::Taste::Geaendert => {
                    self.redraw = true;
                    return true;
                }
                innentuer::Taste::Setzen => {
                    self.tuer_setzen();
                    return true;
                }
                innentuer::Taste::PasstNicht => {
                    let t = innentuer::PASST_NICHT;
                    self.status(Meldung::mit("{}.", &[t]), crate::NOTICE_TIME);
                    self.redraw = true;
                    return true;
                }
            }
        }
        match key {
            Key::Escape => self.oeffnung_esc(),
            Key::Tab if self.innentuer_an() => {
                self.tuer_bild(|t, m, k, st| {
                    t.tab();
                    t.nochmal(m, k, st);
                });
                self.tuer_anzeigen();
                true
            }
            _ => false,
        }
    }

    /// Mausrad über einer Innenwand mit der Innentür am Cursor: blättert die
    /// Breiten statt zu zoomen, mit Strg zoomt es weiter. `true`, wenn das
    /// Werkzeug es nahm.
    pub(crate) fn tuer_rad(&mut self, ev: &Event) -> bool {
        let Event::Wheel { delta, mods, .. } = *ev else {
            return false;
        };
        if mods.ctrl {
            return false;
        }
        if !self.innentuer_an() || !self.tuer_hover() {
            return false;
        }
        self.tuer_bild(|t, m, k, st| {
            t.rad(delta);
            t.nochmal(m, k, st);
        });
        self.tuer_anzeigen();
        true
    }

    /// Statuszeile: Breite und Bezugspunkt der Innentür am Cursor.
    fn tuer_anzeigen(&mut self) {
        let t = self.tool.oeffnung.as_ref().map(|w| w.innen.anzeige());
        if let Some(t) = t {
            self.status(Meldung::mit("{}", &[&t]), crate::NOTICE_TIME);
        }
    }

    /// Innentür im Grundriss: Werkzeug „Tür“ in der Grundrissansicht,
    /// solange keine Außentür halb aufgezogen ist.
    fn innentuer_an(&self) -> bool {
        self.ui.view == ViewKind::Plan
            && self
                .tool
                .oeffnung
                .as_ref()
                .is_some_and(|w| w.kind == OpeningKind::Door && w.erste.is_none())
    }

    /// Ändert die Innentür mit `f` und zeichnet ihre Vorschau neu.
    fn tuer_bild(
        &mut self,
        f: impl FnOnce(
            &mut innentuer::Innentuer,
            &sk_model::Model,
            innentuer::Koerper,
            sk_model::StoreyId,
        ),
    ) {
        let st = self.scene.active_storey();
        let scene = &self.scene;
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return;
        };
        f(&mut w.innen, scene.model(), &|r| scene.koerper(r), st);
        // Maßkette im Stil und in der Schrift der Außenmaßketten (G7)
        let stil = scene.model().massstil();
        let ui = self.ui.fonts.regular.as_ref();
        let schrift = w
            .innen
            .hover
            .as_ref()
            .and_then(|_| self.massketten.schrift(&stil.schrift, ui));
        let bild = w.innen.hover.as_ref().map_or_else(Bild::default, |v| {
            let masse = schrift
                .as_ref()
                .map(|f| innentuer::kette(v, st, stil, f))
                .unwrap_or_default();
            let kette = masse.striche.iter().map(|s| {
                let rund = s.art == sk_plan::bild::Strichart::Symbol;
                (s.a, s.b, crate::bemassung::staerke(stil, s.art), rund)
            });
            Bild {
                linien: Vec::new(),
                umriss: innentuer::umriss(v),
                kette: kette.collect(),
                tinte: masse.tinte,
                pille: w.innen.label(stil.rundung),
                punkt: Some((v.punkt, false)),
            }
        });
        if w.vorschau.tinte != bild.tinte {
            w.tinte_nr = crate::scene::neuer_stand();
        }
        self.redraw |= w.vorschau != bild;
        w.vorschau = bild;
    }

    /// Mausbewegung im Grundriss (Ansichtspixel): Punkt in Schnitthöhe,
    /// Fangabstand in mm aus [`FANG_PX`].
    fn tuer_suchen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) {
        let z = self.scene.plan_cut();
        let at = |dx: f64| self.cam.plane_point(x + dx, y, vw, vh, z);
        let hit = match (at(0.0), at(FANG_PX * sc)) {
            (Some(p), Some(q)) => Some((p, (q - p).length())),
            _ => None,
        };
        let vorher = self.tuer_hover();
        self.tuer_bild(|t, m, k, st| match hit {
            Some((p, tol)) => t.suchen(m, k, st, p, tol, z),
            None => t.weg(),
        });
        // Kommt die Tür an eine Innenwand, sagt die Statuszeile Breite und
        // Bezugspunkt (Einstellungen #99, B1)
        if !vorher && self.tuer_hover() {
            self.tuer_anzeigen();
        }
    }

    /// Hängt die Innentür als Vorschau an einer Innenwand?
    fn tuer_hover(&self) -> bool {
        self.tool
            .oeffnung
            .as_ref()
            .is_some_and(|w| w.innen.hover.is_some())
    }

    /// Klick mit der Innentür am Cursor bzw. **Enter** nach dem Abstand:
    /// setzt sie als ein Schritt; lehnt das Modell ab, sagt die Statuszeile
    /// warum. Eine getippte Zahl gilt danach nicht mehr.
    fn tuer_setzen(&mut self) -> bool {
        let o = self.tool.oeffnung.as_ref().and_then(|w| w.innen.oeffnung());
        let Some(o) = o else {
            return false;
        };
        let m = self.scene.model();
        match m.opening_fault(&o, None).map(|f| f.text(m)) {
            Some(t) => self.status(Meldung::mit("{}.", &[&t]), crate::NOTICE_TIME),
            None => {
                if self.scene.add_opening(o).is_some() {
                    self.upload_model();
                }
            }
        }
        self.tuer_bild(|t, m, k, st| {
            t.eingabe = None;
            t.nochmal(m, k, st);
        });
        self.redraw = true;
        true
    }

    /// Esc: nimmt die erste Ecke zurück, ohne sie schließt das Werkzeug.
    /// `true`, wenn es lief.
    pub(crate) fn oeffnung_esc(&mut self) -> bool {
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return false;
        };
        if w.esc() {
            let hover = w.hover.clone();
            self.oeffnung_vorschau(hover);
        } else {
            self.tool.oeffnung = None;
            self.ext_sync();
            self.refresh_cursor();
        }
        self.redraw = true;
        true
    }
}
