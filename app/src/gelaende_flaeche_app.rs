//! Geländeflächen in der App (G3d, Bedienbarkeit 43.1): das Werkzeug
//! „Fläche zeichnen“ in der Leiste „Gelände“, die Auswahl einer Fläche mit
//! einem Klick im Grundriss und ihre Eigenschaften im Paneel (Höhe,
//! Böschung, befestigt, löschen; dazu Andecken und Faktor des Grundstücks).
//!
//! Das Werkzeug setzt Ecken mit Klicks im Grundriss (es schaltet dorthin); ein Klick auf die
//! erste Ecke, ein zweiter Klick auf die letzte oder Enter schließt die
//! Fläche, Rücktaste oder Strg+Z nimmt die letzte Ecke zurück, Esc verwirft. Die neue
//! Fläche liegt eben auf der Höhe des Geländes in ihrer Mitte, mit der
//! Böschung nach Vorgabe; Höhe und Böschung stellt man danach im Paneel.

use crate::camera::Camera;
use crate::ui::{Field, FieldRow, GelaendeFeld, GelaendeKnopf, Id, Props, Section, ViewKind};
use crate::App;
use sk_math::vec3;
use sk_model::{
    Model, PatchHeight, SiteSoil, TerrainPatch, PATCH_SLOPE, PATCH_SLOPE_RANGE, PATCH_XY_MAX,
};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Raster der Ecken (mm).
const STEP: f64 = 10.0;
/// So nah (Bildpunkte) an der ersten Ecke schließt ein Klick die Fläche.
const SCHLIESSEN_PX: f64 = 10.0;
/// Schritte im Rückgängig.
pub const LABEL_NEU: &str = "Geländefläche";
pub const LABEL_AENDERN: &str = "Geländefläche ändern";
pub const LABEL_LOESCHEN: &str = "Geländefläche löschen";
pub const LABEL_BODEN: &str = "Geländebau";
/// Was die Statuszeile sagt, wenn sich die gezeichnete Fläche nicht bauen
/// lässt (Kanten kreuzen sich, keine Fläche).
pub const KREUZT: &str = "Die Kanten der Fläche kreuzen sich; bitte anders schließen.";
/// Alle Ecken auf einer Linie (H-189d).
pub const OHNE_INHALT: &str = "Die Ecken liegen auf einer Linie; die Fläche hat keinen Inhalt.";
/// Ecken weiter als 100 km vom Ursprung (H-189d).
pub const ZU_WEIT: &str = "Die Fläche liegt zu weit außerhalb; bitte näher am Haus zeichnen.";
/// Hinweise im Paneel; befestigt mit dem Satz aus den Mengen
/// ([`sk_model::qto_earth::BEFESTIGT`], Bedienbarkeit 43.1).
const HINWEIS: &str = "Höhe über ±0,00. Böschung 0: senkrechter Rand.";
const HINWEIS_BEFESTIGT: &str = "Höhe über ±0,00. Böschung 0: senkrechter Rand. Aufbau der \
    befestigten Fläche wird noch nicht gerechnet, Auftrag bis OK Fläche.";
const HINWEIS_BODEN: &str = "Gilt für alle Geländeflächen. Eigenboden fest zu eingebaut unter 1: \
    Boden setzt sich beim Verdichten; ändert nur Eigenboden und Lieferung.";

/// Ergebnis eines Ereignisses.
#[derive(Default)]
pub struct Ausgang {
    pub redraw: bool,
    pub consumed: bool,
    /// Geschlossener Umriss (mm).
    pub commit: Option<Vec<[f64; 2]>>,
    /// Der Umriss lässt sich nicht bauen: Satz für die Statuszeile.
    pub fehler: Option<&'static str>,
}

/// Werkzeug „Fläche zeichnen“.
#[derive(Clone, Debug, Default)]
pub struct Werkzeug {
    pub aktiv: bool,
    /// Gesetzte Ecken (mm).
    ecken: Vec<[f64; 2]>,
    /// Bodenpunkt unter der Maus (Gummiband).
    maus: Option<[f64; 2]>,
}

/// Bodenpunkt unter dem Bildpunkt, auf [`STEP`] gerundet.
fn boden(cam: &Camera, x: f64, y: f64, w: f64, h: f64) -> Option<[f64; 2]> {
    let r = |v: f64| (v / STEP).round() * STEP + 0.0;
    cam.ground_point(x, y, w, h).map(|g| [r(g.x), r(g.y)])
}

impl Werkzeug {
    /// Ein- oder ausschalten; aus verwirft die gesetzten Ecken.
    pub fn set_aktiv(&mut self, on: bool) {
        self.aktiv = on;
        self.ecken.clear();
        self.maus = None;
    }

    pub fn ecken(&self) -> &[[f64; 2]] {
        &self.ecken
    }

    /// Ecken gesetzt: Rückgängig, Wiederherstellen und Entf warten.
    pub fn zeichnet(&self) -> bool {
        self.aktiv && !self.ecken.is_empty()
    }

    /// Esc: erst die gesetzten Ecken, dann das Werkzeug. `true`, wenn
    /// etwas geschlossen wurde.
    pub fn escape(&mut self) -> bool {
        if !self.ecken.is_empty() {
            self.ecken.clear();
            return true;
        }
        if self.aktiv {
            self.set_aktiv(false);
            return true;
        }
        false
    }

    /// Rücktaste und Strg+Z nehmen die letzte Ecke zurück, Enter schließt
    /// ab drei Ecken; Strg+Y, Strg+Umschalt+Z und Entf tun beim Zeichnen
    /// nichts, statt aufs Modell zu wirken (Review #189 B1). `true`, wenn
    /// verbraucht.
    pub fn key(&mut self, key: Key, mods: Modifiers, out: &mut Ausgang) -> bool {
        if !self.zeichnet() {
            return false;
        }
        match key {
            Key::Backspace => {}
            Key::Char('Z') if mods.ctrl && !mods.shift => {}
            Key::Enter => {
                self.schliessen(out);
                return true;
            }
            Key::Char('Y' | 'Z') if mods.ctrl => return true,
            Key::Delete => return true,
            _ => return false,
        }
        self.ecken.pop();
        out.redraw = true;
        true
    }

    /// Umriss fertig, wenn er drei Ecken hat und sich bauen lässt; sonst
    /// bleibt er stehen und `fehler` sagt warum.
    fn schliessen(&mut self, out: &mut Ausgang) {
        out.redraw = true;
        if self.ecken.len() < 3 {
            return;
        }
        let p = TerrainPatch::flat(self.ecken.clone(), 0.0);
        if !p.valid() {
            let weit = self.ecken.iter().flatten().any(|v| v.abs() > PATCH_XY_MAX);
            out.fehler = Some(if auf_einer_linie(&self.ecken) {
                OHNE_INHALT
            } else if weit {
                ZU_WEIT
            } else {
                KREUZT
            });
            return;
        }
        out.commit = Some(std::mem::take(&mut self.ecken));
        self.maus = None;
    }

    /// Ereignis der Ansicht (Koordinaten ohne Titelleiste).
    pub fn handle(&mut self, e: &Event, cam: &Camera, w: f64, h: f64, scale: f64) -> Ausgang {
        let mut out = Ausgang::default();
        if !self.aktiv {
            return out;
        }
        match *e {
            Event::MouseMove { x, y, .. } => {
                let p = boden(cam, x, y, w, h);
                if p != self.maus {
                    self.maus = p;
                    out.redraw = !self.ecken.is_empty();
                }
                out.consumed = true;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                out.consumed = true;
                let Some(p) = boden(cam, x, y, w, h) else {
                    return out;
                };
                // Klick auf die erste Ecke (am Bildschirm) oder noch einmal
                // auf die letzte schließt
                let am_anfang = self.ecken.first().is_some_and(|a| {
                    cam.project(vec3(a[0], a[1], 0.0), w, h)
                        .is_some_and(|q| (q.0 - x).hypot(q.1 - y) < SCHLIESSEN_PX * scale)
                });
                if self.ecken.len() >= 3 && (am_anfang || self.ecken.last() == Some(&p)) {
                    self.schliessen(&mut out);
                } else if self.ecken.last() != Some(&p) {
                    self.ecken.push(p);
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => out.consumed = true,
            Event::MouseLeave => {
                out.redraw = self.maus.take().is_some() && !self.ecken.is_empty();
            }
            _ => {}
        }
        out
    }

    /// Gesetzte Kanten, Gummiband zur Maus und zurück zur ersten Ecke, in
    /// Höhe `z` (mm).
    pub fn linien(&self, z: f64, scale: f32, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let mut pts: Vec<[f64; 2]> = self.ecken.clone();
        if let (Some(m), false) = (self.maus, self.ecken.is_empty()) {
            pts.push(m);
        }
        let mut out = Vec::new();
        let p = |q: [f64; 2]| [q[0] as f32, q[1] as f32, z as f32 + 1.0];
        let linie = |a, b, color, dash: f32| Helper {
            a: p(a),
            b: p(b),
            color,
            width: 2.0 * scale,
            dash: dash * scale,
            pattern: sk_render::SOLID,
            occlude: false,
            round: false,
        };
        for s in pts.windows(2) {
            out.push(linie(s[0], s[1], col.draw, 0.0));
        }
        if pts.len() >= 3 {
            out.push(linie(pts[pts.len() - 1], pts[0], col.guide, 6.0));
        }
        if let Some(&a) = self.ecken.first() {
            let mut m = linie(a, a, col.track, 0.0);
            m.width = 8.0 * scale;
            out.push(m);
        }
        out
    }
}

/// Liegt `p` in dem Polygon `ring`?
fn innen(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut drin = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                drin = !drin;
            }
        }
    }
    drin
}

/// Nummer der Fläche an `p` (mm); bei Überlappung die zuletzt gesetzte,
/// wie sie auch gilt.
pub fn flaeche_bei(m: &Model, p: [f64; 2]) -> Option<usize> {
    m.site_patches()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, f)| innen(&f.outline, p))
        .map(|(i, _)| i)
}

/// Liegen alle Ecken (mm) höchstens 1 mm neben der Geraden durch die erste
/// und die von ihr fernste Ecke?
fn auf_einer_linie(ring: &[[f64; 2]]) -> bool {
    let a = ring[0];
    let d = |p: &[f64; 2]| [p[0] - a[0], p[1] - a[1]];
    let fern = ring
        .iter()
        .map(d)
        .max_by(|u, v| u[0].hypot(u[1]).total_cmp(&v[0].hypot(v[1])));
    let Some(f) = fern.filter(|f| f[0].hypot(f[1]) > 1.0) else {
        return true;
    };
    let l = f[0].hypot(f[1]);
    ring.iter()
        .map(d)
        .all(|u| (u[0] * f[1] - u[1] * f[0]).abs() <= l)
}

/// Fläche eines Umrisses (mm²).
fn inhalt(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let s: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    s.abs() / 2.0
}

fn de(v: f64, stellen: usize) -> String {
    format!("{v:.stellen$}").replace('.', ",")
}

fn feld(f: GelaendeFeld, label: &'static str, value: f64, r: (f64, f64)) -> FieldRow {
    FieldRow {
        field: Field::Gelaende(f),
        label,
        value,
        min: r.0,
        max: r.1,
        zero: false,
        einheit: None,
    }
}

/// Höhe einer ebenen Fläche über ±0,00 (mm), sonst `None` (geneigt oder in
/// m ü. NHN eingegeben: nur in der Datei).
fn eben(p: &TerrainPatch) -> Option<f64> {
    match p.height {
        PatchHeight::Flat(z) if !p.abs => Some(z),
        _ => None,
    }
}

/// Böschung ringsum (n von 1:n, 0 senkrecht), `None`, wenn sie je Kante
/// verschieden ist.
fn boeschung(p: &TerrainPatch) -> Option<f64> {
    match p.slopes.as_slice() {
        [] => Some(PATCH_SLOPE),
        [n] => Some(*n),
        v => v.iter().all(|&n| n == v[0]).then_some(v[0]),
    }
}

/// Paneel „Eigenschaften“ der Fläche `i`.
pub(crate) fn props(m: &Model, i: usize) -> Option<Props> {
    let p = m.site_patches().get(i)?;
    let mut values = vec![
        ("Kategorie", "Geländefläche".to_string()),
        ("Fläche", format!("{} m²", de(inhalt(&p.outline) / 1e6, 2))),
        ("Ecken", p.outline.len().to_string()),
    ];
    let mut fields = Vec::new();
    match eben(p) {
        Some(z) => fields.push(feld(
            GelaendeFeld::Hoehe,
            "Höhe",
            z,
            (-sk_model::PATCH_Z_MAX, sk_model::PATCH_Z_MAX),
        )),
        None => values.push(("Höhe", "geneigt, aus der Datei".to_string())),
    }
    match boeschung(p) {
        Some(n) => fields.push(FieldRow {
            field: Field::Gelaende(GelaendeFeld::Boeschung),
            label: "Böschung 1:",
            value: n,
            min: PATCH_SLOPE_RANGE.0,
            max: PATCH_SLOPE_RANGE.1,
            zero: true,
            einheit: Some(""),
        }),
        None => values.push(("Böschung", "je Kante, aus der Datei".to_string())),
    }
    let s = m.site_soil();
    let befestigt = if p.paved {
        "befestigt: ja"
    } else {
        "befestigt: nein"
    };
    Some(Props {
        values,
        sections: vec![
            Section {
                title: "Geländefläche",
                fields,
                hint: if p.paved { HINWEIS_BEFESTIGT } else { HINWEIS },
                buttons: vec![
                    (Id::Gelaende(GelaendeKnopf::Befestigt), befestigt),
                    (Id::Gelaende(GelaendeKnopf::Loeschen), "Fläche löschen"),
                ],
            },
            Section {
                title: "Grundstück",
                fields: vec![
                    feld(GelaendeFeld::Andecken, SiteSoil::FELDER[0].1, s.spread, {
                        let f = SiteSoil::FELDER[0];
                        (f.2, f.3)
                    }),
                    FieldRow {
                        field: Field::Gelaende(GelaendeFeld::Faktor),
                        label: SiteSoil::FELDER[1].1,
                        value: s.reuse,
                        min: SiteSoil::FELDER[1].2,
                        max: SiteSoil::FELDER[1].3,
                        zero: false,
                        einheit: Some(""),
                    },
                ],
                hint: HINWEIS_BODEN,
                buttons: Vec::new(),
            },
        ],
        ..Default::default()
    })
}

/// Die Fläche `i` mit Feld `f` auf `v`; `None`, wenn der Wert nicht gilt.
pub(crate) fn mit_feld(p: &TerrainPatch, f: GelaendeFeld, v: f64) -> Option<TerrainPatch> {
    let mut q = p.clone();
    match f {
        GelaendeFeld::Hoehe => q.height = PatchHeight::Flat(v),
        GelaendeFeld::Boeschung => {
            let (a, b) = PATCH_SLOPE_RANGE;
            if v != 0.0 && !(a..=b).contains(&v) {
                return None;
            }
            q.slopes = vec![v];
        }
        _ => return None,
    }
    q.valid().then_some(q)
}

impl App {
    /// Ereignis an das Werkzeug „Fläche zeichnen“; `true`, wenn es das
    /// Ereignis genommen hat. Ein geschlossener Umriss wird eine Fläche
    /// (ein Schritt) und ist dann gewählt.
    pub(crate) fn flaeche_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        if !self.platte.leiste.werkzeug.aktiv || self.ui.view != ViewKind::Plan {
            return false;
        }
        let vor = self.platte.leiste.werkzeug.ecken().len();
        let out = self
            .platte
            .leiste
            .werkzeug
            .handle(ev, &self.cam, vw, vh, sc);
        self.flaeche_ausgang(out, vor)
    }

    /// Tasten beim Zeichnen, nur im Grundriss (H-189g).
    pub(crate) fn flaeche_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        let mut out = Ausgang::default();
        let vor = self.platte.leiste.werkzeug.ecken().len();
        if self.ui.view != ViewKind::Plan || !self.platte.leiste.werkzeug.key(key, mods, &mut out) {
            return false;
        }
        self.flaeche_ausgang(out, vor);
        true
    }

    /// Esc beim Zeichnen.
    pub(crate) fn flaeche_escape(&mut self) -> bool {
        if !self.platte.leiste.werkzeug.escape() {
            return false;
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
        self.redraw = true;
        true
    }

    /// `vor`: Ecken vor dem Ereignis; die Hinweise der Leiste folgen, wenn
    /// die erste gesetzt oder die letzte genommen ist.
    fn flaeche_ausgang(&mut self, out: Ausgang, vor: usize) -> bool {
        self.redraw |= out.redraw;
        if (vor == 0) != self.platte.leiste.werkzeug.ecken().is_empty() {
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        }
        if let Some(f) = out.fehler {
            self.status(crate::meldung::Meldung::satz(f), crate::NOTICE_TIME);
        }
        if let Some(ring) = out.commit {
            self.flaeche_anlegen(ring);
        }
        out.consumed
    }

    /// Neue Fläche aus dem Umriss `ring`: eben auf dem Gelände in ihrer
    /// Mitte, auf 1 cm; gewählt.
    fn flaeche_anlegen(&mut self, ring: Vec<[f64; 2]>) {
        let m = self.scene.model();
        let n = ring.len() as f64;
        let c = ring
            .iter()
            .fold([0.0, 0.0], |s, p| [s[0] + p[0] / n, s[1] + p[1] / n]);
        let z = (m.terrain_z_at(m.level_building(), c[0], c[1]) / 10.0).round() * 10.0 + 0.0;
        let p = TerrainPatch::flat(ring, z);
        let mut neu = None;
        if self.scene.edit_model(LABEL_NEU, |m| {
            neu = m.add_site_patch(p);
            neu.is_some()
        }) {
            self.upload_model();
            self.quantity.dirty = true;
        }
        if let Some(i) = neu {
            self.flaeche_waehlen(Some(i));
            // Mengen zählen dort doppelt (Bedienbarkeit 43.1)
            if self.scene.model().site_patch_overlaps(i) {
                let m = crate::meldung::Meldung::satz(sk_model::qto_earth::UEBERLAPPT);
                self.status(m, crate::NOTICE_TIME);
            }
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
    }

    /// Ein Werkzeug in Eingabe, etwas gezogen oder Ecken einer Fläche
    /// gesetzt: Rückgängig, Wiederherstellen und Löschen warten (Review
    /// #189 B1).
    pub(crate) fn im_zug(&self) -> bool {
        self.tool.is_active()
            || self.edit.is_dragging()
            || self.drains.is_dragging()
            || self.platte.leiste.werkzeug.zeichnet()
    }

    /// Entf: die gewählte Fläche löschen, beim Zeichnen nichts; `false`,
    /// wenn Entf den Bauteilen gilt.
    pub(crate) fn flaeche_entf(&mut self) -> bool {
        if self.im_zug() {
            return true;
        }
        if self.picking.flaeche.is_none() || !self.picking.selected.is_empty() {
            return false;
        }
        self.flaeche_knopf(GelaendeKnopf::Loeschen);
        true
    }

    /// Nach einem Wechsel der Sicht: Nordpfeil nur, wo er geht, das
    /// Werkzeug „Fläche zeichnen“ nur im Grundriss (H-189g).
    pub(crate) fn sicht_werkzeuge(&mut self) {
        if !self.tool_allowed() {
            self.nord.set_aktiv(false);
        }
        if self.ui.view != ViewKind::Plan && self.platte.leiste.werkzeug.aktiv {
            self.platte.leiste.werkzeug.set_aktiv(false);
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        }
    }

    /// Fläche `i` wählen (oder keine); Bauteile und Platte sind dann nicht
    /// gewählt.
    pub(crate) fn flaeche_waehlen(&mut self, i: Option<usize>) {
        if i.is_some() {
            self.picking.selected.clear();
            self.picking.plate = false;
            self.sel.set(None);
        }
        if self.picking.flaeche != i {
            self.picking.flaeche = i;
            self.quantity.dirty = true;
        }
        self.redraw = true;
    }

    /// Klick ins Leere im Grundriss: die Fläche darunter wählen. `true`,
    /// wenn dort eine ist.
    pub(crate) fn flaeche_klick(&mut self, x: f64, y: f64, vw: f64, vh: f64) -> bool {
        let frei = self.ui.frei.frei(crate::freischaltung::Modul::Gelaende);
        if !frei || self.ui.view != ViewKind::Plan || self.scene.terrain_hidden() {
            return false;
        }
        let Some(g) = self.cam.ground_point(x, y, vw, vh) else {
            return false;
        };
        let Some(i) = flaeche_bei(self.scene.model(), [g.x, g.y]) else {
            return false;
        };
        self.flaeche_waehlen(Some(i));
        true
    }

    /// Paneel „Eigenschaften“ der gewählten Fläche; `true`, wenn es sie
    /// zeigt. Je Modellstand einmal gebaut.
    pub(crate) fn flaeche_props(&mut self) -> bool {
        let m = self.scene.model();
        let i = self
            .picking
            .flaeche
            .filter(|&i| self.sel.id.is_none() && i < m.site_patches().len());
        if i.is_none() && self.picking.flaeche.is_some() {
            self.picking.flaeche = None;
        }
        let soll = i.map(|i| (i, m.revision()));
        if soll == self.platte.leiste.props {
            return soll.is_some();
        }
        let war = self.platte.leiste.props;
        self.platte.leiste.props = soll;
        match i.and_then(|i| props(m, i)) {
            Some(p) => {
                if war.map(|w| w.0) != i {
                    self.ui.reset_props_scroll();
                }
                self.ui.set_props(Some(p));
            }
            None => {
                self.ui.set_props(None);
                self.props_key = None;
            }
        }
        self.props_dirty = true;
        soll.is_some()
    }

    /// Knöpfe der gewählten Fläche und das Werkzeug.
    pub(crate) fn flaeche_knopf(&mut self, k: GelaendeKnopf) {
        match k {
            GelaendeKnopf::Flaeche => {
                let w = &mut self.platte.leiste.werkzeug;
                let an = !w.aktiv;
                w.set_aktiv(an);
                if an {
                    // gezeichnet wird im Grundriss, wo der Klick den Boden trifft
                    if self.ui.view != ViewKind::Plan {
                        self.set_view(ViewKind::Plan);
                    }
                    self.tool.set_enabled(false);
                    self.nord.set_aktiv(false);
                    self.refresh_cursor();
                }
            }
            GelaendeKnopf::Befestigt => {
                if let Some(i) = self.picking.flaeche {
                    let p = self.scene.model().site_patches().get(i).cloned();
                    if let Some(mut p) = p {
                        p.paved = !p.paved;
                        self.flaeche_schritt(LABEL_AENDERN, |m| m.set_site_patch(i, p));
                    }
                }
            }
            GelaendeKnopf::Loeschen => {
                if let Some(i) = self.picking.flaeche {
                    if self.flaeche_schritt(LABEL_LOESCHEN, |m| m.remove_site_patch(i)) {
                        self.flaeche_waehlen(None);
                    }
                }
            }
            _ => {}
        }
        self.redraw = true;
    }

    /// Feld im Paneel der Fläche übernommen; `true`, wenn es zu ihr
    /// gehört. Ein Wert, der nicht gilt, ändert nichts.
    pub(crate) fn flaeche_feld(&mut self, f: GelaendeFeld, v: f64) -> bool {
        match f {
            GelaendeFeld::Hoehe | GelaendeFeld::Boeschung => {
                let Some(i) = self.picking.flaeche else {
                    return true;
                };
                let p = self.scene.model().site_patches().get(i).cloned();
                if let Some(q) = p.and_then(|p| mit_feld(&p, f, v)) {
                    self.flaeche_schritt(LABEL_AENDERN, |m| m.set_site_patch(i, q));
                }
                true
            }
            GelaendeFeld::Andecken | GelaendeFeld::Faktor => {
                let mut s = self.scene.model().site_soil();
                if f == GelaendeFeld::Andecken {
                    s.spread = v;
                } else {
                    s.reuse = (v * 1000.0).round() / 1000.0;
                }
                self.flaeche_schritt(LABEL_BODEN, |m| m.set_site_soil(s));
                true
            }
            _ => false,
        }
    }

    /// Eine Änderung an den Flächen als ein Schritt; Bild und Mengen
    /// folgen.
    fn flaeche_schritt(&mut self, label: &'static str, f: impl FnOnce(&mut Model) -> bool) -> bool {
        let neu = self.scene.edit_model(label, f);
        if neu {
            self.upload_model();
            self.quantity.dirty = true;
            self.props_dirty = true;
        }
        neu
    }

    /// Hilfslinien: das Werkzeug beim Zeichnen und der Umriss der
    /// gewählten Fläche.
    pub(crate) fn flaeche_linien(&self, scale: f32) -> Vec<Helper> {
        let m = self.scene.model();
        let z = m.terrain_z();
        let mut out = self.platte.leiste.werkzeug.linien(z, scale, &self.theme);
        if let Some(p) = self.picking.flaeche.and_then(|i| m.site_patches().get(i)) {
            let h0 = m.terrain_nhn().unwrap_or(0.0);
            let n = p.outline.len();
            let q = |k: usize| {
                let [x, y] = p.outline[k % n];
                let z = p.z_at(x, y, h0);
                [x as f32, y as f32, z as f32 + 1.0]
            };
            for k in 0..n {
                out.push(Helper {
                    a: q(k),
                    b: q(k + 1),
                    color: self.theme.interact.select,
                    width: 3.0 * scale,
                    dash: 0.0,
                    pattern: sk_render::SOLID,
                    occlude: false,
                    round: false,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::Vec3;
    use std::f64::consts::FRAC_PI_2;

    const OHNE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn punkt(x: f64, y: f64) -> Vec3 {
        vec3(x, y, 0.0)
    }

    /// Grundriss von oben über 0..20 m, 800 × 800 Bildpunkte.
    fn kamera() -> (Camera, f64, f64) {
        let c = Camera::parallel(punkt(10000.0, 10000.0), FRAC_PI_2, -FRAC_PI_2, 12500.0);
        (c, 800.0, 800.0)
    }

    fn klick(w: &mut Werkzeug, cam: &Camera, p: Vec3) -> Ausgang {
        let (x, y) = cam.project(p, 800.0, 800.0).unwrap();
        w.handle(
            &Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: Modifiers::default(),
            },
            cam,
            800.0,
            800.0,
            1.0,
        )
    }

    /// Vier Klicks und ein Klick auf die erste Ecke: eine Fläche mit vier
    /// Ecken auf dem Raster.
    #[test]
    fn rechteck_schliesst_an_der_ersten_ecke() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [
            (2001.0, 2002.0),
            (8000.0, 2000.0),
            (8000.0, 6000.0),
            (2000.0, 6000.0),
        ] {
            let o = klick(&mut w, &cam, punkt(p.0, p.1));
            assert!(o.consumed && o.commit.is_none());
        }
        assert_eq!(w.ecken().len(), 4);
        let o = klick(&mut w, &cam, punkt(2003.0, 2004.0));
        let ring = o.commit.expect("geschlossen");
        assert_eq!(ring.len(), 4);
        for q in &ring {
            assert_eq!(q[0] % STEP, 0.0);
            assert_eq!(q[1] % STEP, 0.0);
        }
        assert!(w.ecken().is_empty() && w.aktiv);
    }

    /// Rücktaste nimmt die letzte Ecke, Enter schließt, Esc verwirft erst
    /// die Ecken, dann das Werkzeug.
    #[test]
    fn tasten() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [
            (0.0, 0.0),
            (5000.0, 0.0),
            (5000.0, 5000.0),
            (9000.0, 9000.0),
        ] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        assert!(w.key(Key::Backspace, OHNE, &mut o));
        assert_eq!(w.ecken().len(), 3);
        assert!(w.key(Key::Enter, OHNE, &mut o));
        assert_eq!(o.commit.map(|r| r.len()), Some(3));
        klick(&mut w, &cam, punkt(0.0, 0.0));
        assert!(w.escape());
        assert!(w.aktiv && w.ecken().is_empty());
        assert!(w.escape());
        assert!(!w.aktiv);
        assert!(!w.escape());
    }

    /// Ein Umriss mit sich kreuzenden Kanten wird keine Fläche; die Ecken
    /// bleiben stehen.
    #[test]
    fn kreuzende_kanten() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [(0.0, 0.0), (6000.0, 6000.0), (6000.0, 0.0), (0.0, 6000.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert!(o.commit.is_none());
        assert_eq!(o.fehler, Some(KREUZT));
        assert_eq!(w.ecken().len(), 4);
        // drei Ecken auf einer Linie: kein Inhalt statt „kreuzen“ (H-189d)
        w.escape();
        for p in [(0.0, 0.0), (3000.0, 0.0), (6000.0, 0.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert_eq!(o.fehler, Some(OHNE_INHALT));
    }

    /// Beim Zeichnen nimmt Strg+Z die letzte Ecke; Strg+Y, Strg+Umschalt+Z
    /// und Entf verbraucht das Werkzeug, damit sie nicht aufs Modell wirken
    /// (Review #189 B1). Ohne Ecken gehören die Tasten wieder der App.
    #[test]
    fn rueckgaengig_beim_zeichnen() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        let mut o = Ausgang::default();
        let strg = Modifiers { ctrl: true, ..OHNE };
        let umschalt = Modifiers {
            shift: true,
            ..strg
        };
        assert!(!w.zeichnet() && !w.key(Key::Char('Z'), strg, &mut o));
        for p in [(0.0, 0.0), (5000.0, 0.0), (5000.0, 5000.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        assert!(w.zeichnet());
        for (k, m) in [('Y', strg), ('Z', umschalt)] {
            assert!(w.key(Key::Char(k), m, &mut o));
        }
        assert!(w.key(Key::Delete, OHNE, &mut o));
        assert!(!w.key(Key::Char('Z'), OHNE, &mut o), "Z ohne Strg");
        assert_eq!(w.ecken().len(), 3);
        assert!(w.key(Key::Char('Z'), strg, &mut o));
        assert_eq!(w.ecken().len(), 2);
        assert!(o.commit.is_none());
        w.key(Key::Char('Z'), strg, &mut o);
        w.key(Key::Char('Z'), strg, &mut o);
        assert!(!w.zeichnet() && !w.key(Key::Delete, OHNE, &mut o));
    }

    /// Klick in eine Fläche trifft die zuletzt gesetzte, außerhalb keine.
    #[test]
    fn treffer() {
        let mut m = Model::with_seed(1);
        m.begin("Flächen");
        let r = |x0: f64, x1: f64| vec![[x0, 0.0], [x1, 0.0], [x1, 4000.0], [x0, 4000.0]];
        m.add_site_patch(TerrainPatch::flat(r(0.0, 6000.0), 0.0));
        m.add_site_patch(TerrainPatch::flat(r(4000.0, 9000.0), 0.0));
        m.commit();
        assert_eq!(flaeche_bei(&m, [1000.0, 1000.0]), Some(0));
        assert_eq!(flaeche_bei(&m, [5000.0, 1000.0]), Some(1));
        assert_eq!(flaeche_bei(&m, [9500.0, 1000.0]), None);
    }

    /// Paneel: Höhe und Böschung als Felder, solange sie für die ganze
    /// Fläche gelten; Werte, die nicht gelten, ändern nichts.
    #[test]
    fn paneel_und_felder() {
        let mut m = Model::with_seed(1);
        m.begin("Fläche");
        let ring = vec![[0.0, 0.0], [5000.0, 0.0], [5000.0, 4000.0], [0.0, 4000.0]];
        m.add_site_patch(TerrainPatch::flat(ring, -600.0));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert!(p.values.contains(&("Fläche", "20,00 m²".to_string())));
        let f = &p.sections[0].fields;
        assert_eq!(f[0].field, Field::Gelaende(GelaendeFeld::Hoehe));
        assert_eq!(f[0].value, -600.0);
        assert_eq!(f[1].value, PATCH_SLOPE);
        assert_eq!(p.sections[1].fields[0].value, 200.0);
        // ein Name wie im Modell, Hinweise zu Faktor und befestigt (43.1 B)
        assert_eq!(
            p.sections[1].fields[1].label,
            "Eigenboden fest zu eingebaut"
        );
        assert!(p.sections[1].hint.contains("unter 1: Boden setzt sich"));
        assert_eq!(p.sections[0].hint, HINWEIS);
        assert!(HINWEIS_BEFESTIGT.contains(sk_model::qto_earth::BEFESTIGT));
        let mut b = m.site_patches()[0].clone();
        b.paved = true;
        m.begin("befestigt");
        assert!(m.set_site_patch(0, b));
        m.commit();
        assert_eq!(props(&m, 0).unwrap().sections[0].hint, HINWEIS_BEFESTIGT);
        let q = &m.site_patches()[0];
        let h = mit_feld(q, GelaendeFeld::Hoehe, 300.0).unwrap();
        assert_eq!(h.height, PatchHeight::Flat(300.0));
        assert_eq!(
            mit_feld(q, GelaendeFeld::Boeschung, 0.0).unwrap().slopes,
            vec![0.0]
        );
        assert!(mit_feld(q, GelaendeFeld::Boeschung, 0.2).is_none());
        assert!(mit_feld(q, GelaendeFeld::Boeschung, 25.0).is_none());
        // je Kante verschieden: kein Feld, ein Wert sagt es
        let mut q2 = q.clone();
        q2.slopes = vec![1.0, 2.0, 1.0, 2.0];
        m.begin("Fläche");
        assert!(m.set_site_patch(0, q2));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert_eq!(p.sections[0].fields.len(), 1);
        assert!(p
            .values
            .contains(&("Böschung", "je Kante, aus der Datei".to_string())));
        assert!(props(&m, 1).is_none());
    }
}
