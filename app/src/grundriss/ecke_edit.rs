//! Werkzeuge „Ecke bilden“ und „Verlängern“ an der gewählten Wand
//! (Grundriss G5, Jörn 11.10. 16:47, J4 und J5). Gestartet über die Knöpfe
//! im Paneel „Eigenschaften“, Karte wie beim Teilen.
//!
//! - Ecke bilden: Klick auf die zweite Wand, auf der Seite, die bleiben soll.
//!   Beide Wände laufen bis zum Schnittpunkt ihrer Bezugslinien; von der
//!   gewählten Wand bleibt der längere Teil.
//! - Verlängern: Klick auf die Zielwand; die gewählte Wand läuft mit dem
//!   näheren Ende bis an deren zugewandte Fläche (in einer Flucht bis an ihr
//!   Ende; bei gleichem Typ werden beide eine Wand, G5c). Danach bleibt die
//!   Zielwand: jeder weitere Klick verlängert eine Wand bis dorthin, Esc
//!   beendet.
//!
//! Über der Wand unter der Maus zeigt sich das Ergebnis als Vorschau; die
//! Pille nennt die Wand oder den Grund, warum es nicht geht. Die Rechnung
//! steht in [`sk_model::Model::ecke_punkte`] und
//! [`sk_model::Model::verlaengern_punkte`]. Teil von [`crate::wall_edit`].

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::Vec3;
use sk_model::{EckeFehler, ElementId, Vereint, ZugPunkte};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Greifabstand zu einer Wand neben ihrem Grundriss (px bei 96 dpi).
const PICK_PX: f64 = 8.0;

/// Welches der beiden Werkzeuge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Ecke,
    Verlaengern,
}

/// Was ein Ereignis im Werkzeug bewirkt hat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub redraw: bool,
    /// Ein Schritt ist gemacht; diese Wand wird gewählt.
    pub schritt: Option<ElementId>,
    /// Das Werkzeug ist zu Ende (nach „Ecke bilden“ oder mit Esc).
    pub fertig: bool,
    /// Mit Esc beendet, ohne Schritt.
    pub cancelled: bool,
    /// Abgelehnt, mit Grund.
    pub refused: Option<EckeFehler>,
    /// Gesperrt: das sperrende Bauteil und die Wand.
    pub locked: Option<(ElementId, ElementId)>,
    /// Neue Karte (Verlängern nach dem ersten Schritt).
    pub karte: Option<Vec<String>>,
}

/// Karte beim Start.
pub fn karte(art: Art) -> Vec<String> {
    match art {
        Art::Ecke => [
            "Ecke bilden",
            "Klick auf die zweite Wand, auf der Seite, die bleiben soll.",
            "Beide Wände laufen bis zum Schnittpunkt; Esc bricht ab.",
        ],
        Art::Verlaengern => [
            "Verlängern",
            "Klick auf die Wand, bis zu der sie reichen soll.",
            "Liegt sie quer darüber, wird gekürzt; Esc bricht ab.",
        ],
    }
    .map(String::from)
    .to_vec()
}

/// Karte nach dem ersten Verlängern: weitere Wände bis zur Zielwand `ziel`.
fn karte_weiter(ziel: &str) -> Vec<String> {
    vec![
        format!("Weitere Wände bis {ziel}"),
        "Klick auf eine Wand verlängert sie bis dorthin.".into(),
        "Esc beendet.".into(),
    ]
}

/// Statuszeile nach Esc.
pub const CANCELLED: &str = "Ecke bilden bzw. Verlängern beendet.";

/// Laufendes „Ecke bilden“ oder „Verlängern“.
#[derive(Clone, Debug)]
pub struct EckeWahl {
    pub art: Art,
    /// Gewählte Wand beim Start.
    wand: ElementId,
    /// Verlängern nach dem ersten Schritt: die Zielwand für weitere Wände.
    ziel: Option<ElementId>,
    /// Ebene, auf der die Maus gemessen wird (Fuß der Wand).
    z: f64,
    /// Wand und Punkt unter der Maus.
    hover: Option<(ElementId, Vec3)>,
    /// Ergebnis über der Wand unter der Maus.
    vorschau: Option<Result<Rechnung, EckeFehler>>,
}

/// Neue Punkte je Zug; werden beim Verlängern zwei Wände eine, wie.
type Rechnung = (Vec<ZugPunkte>, Option<Vereint>);

impl EckeWahl {
    /// Beginnt an der Wand `wand`; `Err`, wenn sie kein freies Ende hat
    /// oder nicht im aktiven Geschoss liegt (im Baum gewählt; gezeigt wird
    /// nur das aktive, Review #82 H2).
    pub fn start(scene: &Scene, wand: ElementId, art: Art) -> Result<EckeWahl, EckeFehler> {
        let m = scene.model();
        let (run, _) = m.segment_of(wand).ok_or(EckeFehler::KeineWand)?;
        if m.run(run).is_none_or(|r| r.storey != scene.active_storey()) {
            return Err(EckeFehler::Geschoss);
        }
        if !m.freies_ende(wand) {
            return Err(EckeFehler::KeinEnde);
        }
        let z = scene.chain(run).map_or(0.0, |c| c.base);
        Ok(EckeWahl {
            art,
            wand,
            ziel: None,
            z,
            hover: None,
            vorschau: None,
        })
    }

    /// Nummer einer Wand für Pille und Karte.
    fn nummer(scene: &Scene, id: ElementId) -> String {
        scene
            .model()
            .element(id)
            .map_or_else(String::new, |e| e.number.clone())
    }

    /// Wand unter der Maus im Geschoss der gewählten Wand, ohne die, die
    /// gerade nicht in Frage kommt, mit dem Punkt auf der Ebene. Die Züge
    /// kommen berechnet aus der Szene; nur die, deren Quader die Maus
    /// erreicht, werden genau geprüft (Review #82 1).
    fn wand_bei(
        &self,
        scene: &Scene,
        cam: &Camera,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Option<(ElementId, Vec3)> {
        let g = if cam.ortho.is_some() && cam.forward().z.abs() > 0.5 {
            cam.ray(x, y, w, h).0
        } else {
            cam.plane_point(x, y, w, h, self.z)?
        };
        let g = sk_math::vec3(g.x, g.y, self.z);
        let m = scene.model();
        let storey = scene.active_storey();
        let ohne = self.ziel.unwrap_or(self.wand);
        let px = |p: Vec3| {
            let (Some(a), Some(b)) = (cam.project(p, w, h), cam.project(g, w, h)) else {
                return f64::MAX;
            };
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
        };
        let um = PICK_PX * 2.0 * mm_je_px(cam, g, w, h);
        let weit = |(lo, hi): (Vec3, Vec3)| {
            g.x < lo.x - um || g.x > hi.x + um || g.y < lo.y - um || g.y > hi.y + um
        };
        let mut best: Option<(f64, ElementId)> = None;
        for (run, c, quader) in scene.zuege_im((g.x - um, g.y - um), (g.x + um, g.y + um)) {
            if quader.is_some_and(weit) || m.run(run).is_none_or(|r| r.storey != storey) {
                continue;
            }
            for k in 0..c.segment_count() {
                let (Some(q), Some(id)) = (c.segment_footprint(k), m.wall_at(run, k)) else {
                    continue;
                };
                if id == ohne {
                    continue;
                }
                let d = if innen(&q, g) {
                    0.0
                } else {
                    (0..4)
                        .map(|i| {
                            let (a, b) = (q[i], q[(i + 1) % 4]);
                            let t = nah_auf(a, b, g);
                            px(sk_math::vec3(t.x, t.y, self.z))
                        })
                        .fold(f64::MAX, f64::min)
                };
                if d < PICK_PX && best.is_none_or(|b| d < b.0) {
                    best = Some((d, id));
                }
            }
        }
        best.map(|(_, id)| (id, g))
    }

    /// Verlängern: die Wand, die wandert, und die Zielwand, mit der Wand
    /// `b` unter der Maus.
    fn paar(&self, b: ElementId) -> (ElementId, ElementId) {
        match self.ziel {
            None => (self.wand, b),
            Some(ziel) => (b, ziel),
        }
    }

    /// Ergebnis für die Wand `b` unter der Maus am Punkt `bei`.
    fn rechne(&self, scene: &Scene, b: ElementId, bei: Vec3) -> Result<Rechnung, EckeFehler> {
        let m = scene.model();
        if self.art == Art::Ecke {
            let mitte = mitte(scene, self.wand).ok_or(EckeFehler::KeineWand)?;
            return m
                .ecke_punkte(self.wand, mitte, b, bei)
                .map(|z| (Vec::from(z), None));
        }
        let (x, y) = self.paar(b);
        match m.vereinen_punkte(x, y) {
            Some(v) => Ok((vec![v.zug.clone()], Some(v))),
            None => m.verlaengern_punkte(x, y).map(|z| (vec![z], None)),
        }
    }

    /// Wand und Vorschau unter der Maus; `true`, wenn neu zu zeichnen ist
    /// (die Vorschau der Ecke hängt auch vom Punkt auf der Wand ab).
    fn zielen(&mut self, scene: &Scene, cam: &Camera, x: f64, y: f64, w: f64, h: f64) -> bool {
        let hover = self.wand_bei(scene, cam, x, y, w, h);
        let vorher = self.hover.map(|h| h.0);
        self.vorschau = hover.map(|(b, bei)| self.rechne(scene, b, bei));
        self.hover = hover;
        vorher.is_some() || self.hover.is_some()
    }

    /// Führt den Schritt an der Wand unter der Maus aus: die neuen Punkte
    /// je Zug im offenen Schritt wie beim Ziehen, nur die betroffenen Züge
    /// werden neu gerechnet (Review #82 2). Lehnt ein Zug ab, rollt der
    /// Schritt ganz zurück.
    fn klick(&mut self, scene: &mut Scene, out: &mut Outcome) {
        let Some((b, bei)) = self.hover else {
            return;
        };
        let (zuege, vereint) = match self.rechne(scene, b, bei) {
            Ok(z) => z,
            Err(e) => {
                out.refused = Some(e);
                return;
            }
        };
        // Die Wand, deren Ende wandert (sie wird danach gewählt)
        let (art, ziel) = (self.art, self.ziel);
        let wand = match (art, ziel) {
            (Art::Verlaengern, Some(_)) => b,
            _ => self.wand,
        };
        let m = scene.model();
        let weg = vereint.as_ref().and_then(|v| m.run(v.weg));
        let spalte: Vec<ElementId> = geaendert(scene, &zuege)
            .into_iter()
            .chain(weg.into_iter().flat_map(|r| r.segments.iter().copied()))
            .flat_map(|w| m.stack_column(w))
            .collect();
        if let Some(id) = sk_model::edit_blocked(m, &spalte) {
            out.locked = Some((id, wand));
            return;
        }
        if let Some(v) = vereint {
            self.vereinen(scene, b, &v, out);
            return;
        }
        scene.begin(match art {
            Art::Ecke => "Ecke gebildet",
            Art::Verlaengern => "Wand verlängert",
        });
        if !zuege
            .iter()
            .all(|(run, pts)| scene.set_run_points(*run, pts))
        {
            scene.rollback();
            out.refused = Some(EckeFehler::Ungueltig);
            return;
        }
        scene.commit();
        out.schritt = Some(wand);
        match (art, ziel) {
            (Art::Ecke, _) => out.fertig = true,
            (Art::Verlaengern, None) => {
                self.ziel = Some(b);
                out.karte = Some(karte_weiter(&Self::nummer(scene, b)));
            }
            (Art::Verlaengern, Some(_)) => {}
        }
        self.hover = None;
        self.vorschau = None;
    }

    /// Verlängern in einer Flucht mit gleichem Typ: Beide Wände werden eine,
    /// als ein Schritt. Die vereinte Wand wird gewählt und ist danach die
    /// Zielwand für weitere Wände.
    fn vereinen(&mut self, scene: &mut Scene, b: ElementId, v: &Vereint, out: &mut Outcome) {
        let (x, y) = self.paar(b);
        if !scene.edit_model("Wände vereint", |m| m.verlaengern(x, y).is_ok()) {
            out.refused = Some(EckeFehler::Ungueltig);
            return;
        }
        let Some(neu) = scene.model().wall_at(v.zug.0, v.seg) else {
            return;
        };
        out.schritt = Some(neu);
        if self.ziel.is_none() {
            out.karte = Some(karte_weiter(&Self::nummer(scene, neu)));
        }
        self.ziel = Some(neu);
        self.hover = None;
        self.vorschau = None;
    }

    /// Verarbeitet ein Ereignis der Ansicht; das Werkzeug nimmt Linksklicks
    /// und Esc.
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
    ) -> (Outcome, bool) {
        let mut out = Outcome::default();
        let consumed = match *e {
            Event::MouseMove { x, y, .. } => {
                out.redraw = self.zielen(scene, cam, x, y, w, h);
                false
            }
            Event::MouseLeave => {
                out.redraw = self.hover.take().is_some();
                self.vorschau = None;
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.zielen(scene, cam, x, y, w, h);
                self.klick(scene, &mut out);
                out.redraw = true;
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => true,
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                out.cancelled = self.ziel.is_none();
                out.fertig = true;
                out.redraw = true;
                true
            }
            _ => false,
        };
        (out, consumed)
    }

    /// Pille an der Maus: die Wand unter der Maus oder der Grund, warum es
    /// dort nicht geht.
    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        let (b, _) = self.hover?;
        let nr = Self::nummer(scene, b);
        let text = match (self.vorschau.as_ref()?, self.art, self.ziel) {
            (Err(e), _, _) => e.message().to_string(),
            (Ok(_), Art::Ecke, _) => format!("Ecke mit {nr}"),
            (Ok((_, Some(_))), Art::Verlaengern, None) => format!("mit {nr} vereint"),
            (Ok((_, Some(_))), Art::Verlaengern, Some(z)) => {
                format!("{nr} mit {} vereint", Self::nummer(scene, z))
            }
            (Ok(_), Art::Verlaengern, None) => format!("bis {nr}"),
            (Ok(_), Art::Verlaengern, Some(z)) => {
                format!("{nr} bis {}", Self::nummer(scene, z))
            }
        };
        Some((text, false))
    }

    /// Hilfslinien: die Bezugslinien der Wände, wie sie nach dem Klick
    /// laufen, und die Zielwand nach dem ersten Verlängern.
    pub fn helpers(&self, scene: &Scene, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let lift = |p: Vec3| [p.x as f32, p.y as f32, (self.z + 2.0) as f32];
        let line = |a: Vec3, b: Vec3, color, width: f32| Helper {
            a: lift(a),
            b: lift(b),
            color,
            width: width * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude,
            round: false,
        };
        let mut out = Vec::new();
        if let Some(z) = self.ziel.and_then(|z| bezug(scene, z)) {
            out.push(line(z.0, z.1, col.shadow_band, 6.0));
            out.push(line(z.0, z.1, col.drag_hot, 4.0));
        }
        if let Some(Ok((zuege, _))) = &self.vorschau {
            for (_, pts) in zuege {
                for p in pts.windows(2) {
                    out.push(line(p[0], p[1], col.shadow_band, 5.0));
                    out.push(line(p[0], p[1], col.drag_hot, 3.0));
                }
            }
        }
        out
    }
}

/// Wände, deren Segment sich mit den neuen Punkten ändert.
fn geaendert(scene: &Scene, zuege: &[ZugPunkte]) -> Vec<ElementId> {
    let m = scene.model();
    let mut out = Vec::new();
    for (run, pts) in zuege {
        let Some(r) = m.run(*run) else {
            continue;
        };
        let n = r.points.len();
        let anders = |k: usize| {
            let (a, b) = (r.points[k % n], pts[k % n.max(1)]);
            (a.x - b.x).abs() > 1e-9 || (a.y - b.y).abs() > 1e-9
        };
        for (k, w) in r.segments.iter().enumerate() {
            if n == pts.len() && (anders(k) || anders(k + 1)) {
                out.push(*w);
            }
        }
    }
    out
}

/// Millimeter je Bildpunkt an `g` (größere der beiden Achsen); ohne
/// Projektion unendlich (dann prüft der Vortest nichts aus).
fn mm_je_px(cam: &Camera, g: Vec3, w: f64, h: f64) -> f64 {
    let Some(a) = cam.project(g, w, h) else {
        return f64::INFINITY;
    };
    let l = [
        sk_math::vec3(100.0, 0.0, 0.0),
        sk_math::vec3(0.0, 100.0, 0.0),
    ]
    .into_iter()
    .filter_map(|v| cam.project(g + v, w, h))
    .map(|b| (b.0 - a.0).hypot(b.1 - a.1))
    .fold(f64::INFINITY, f64::min);
    if l > 1e-9 && l.is_finite() {
        100.0 / l
    } else {
        f64::INFINITY
    }
}

/// Mitte der Bezugslinie einer Wand: Von der gewählten Wand bleibt so der
/// längere Teil.
fn mitte(scene: &Scene, id: ElementId) -> Option<Vec3> {
    let (a, b) = bezug(scene, id)?;
    Some((a + b) * 0.5)
}

/// Bezugslinie einer Wand (Anfang, Ende).
fn bezug(scene: &Scene, id: ElementId) -> Option<(Vec3, Vec3)> {
    let m = scene.model();
    let (run, k) = m.segment_of(id)?;
    let r = m.run(run)?;
    let n = r.points.len();
    let a = *r.points.get(k)?;
    let b = *r.points.get((k + 1) % n)?;
    Some((a, b))
}

/// Liegt `p` im (konvexen) Viereck `q`?
pub(super) fn innen(q: &[Vec3; 4], p: Vec3) -> bool {
    let s: Vec<f64> = (0..4)
        .map(|i| {
            let (a, b) = (q[i], q[(i + 1) % 4]);
            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
        })
        .collect();
    s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0)
}

/// Nächster Punkt der Strecke `a`–`b` zu `p` (Grundriss).
fn nah_auf(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let e = b - a;
    let l2 = e.x * e.x + e.y * e.y;
    if l2 < 1e-12 {
        return a;
    }
    let t = (((p.x - a.x) * e.x + (p.y - a.y) * e.y) / l2).clamp(0.0, 1.0);
    a + e * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, Model, RefSide};

    const W: f64 = 1200.0;
    const H: f64 = 800.0;

    fn cam() -> Camera {
        Camera::looking_at(
            vec3(5000.0, -6000.0, 18000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        )
    }

    /// Haus 10 × 8 m mit Innenwänden: a senkrecht und b waagerecht, die
    /// sich nicht erreichen, d senkrecht unter b, e parallel zu b.
    fn haus() -> (Scene, [ElementId; 4]) {
        let (m, w) = modell();
        (Scene::with_model(m), w)
    }

    fn modell() -> (Model, [ElementId; 4]) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let iw = m.defaults().interior_wall;
        let mut innen = |a: Vec3, b: Vec3| {
            let r = m
                .add_wall_run(
                    &[a, b],
                    false,
                    RefSide::Center,
                    st,
                    iw,
                    Category::InteriorWall,
                )
                .unwrap();
            m.wall_at(r, 0).unwrap()
        };
        let w = [
            innen(vec3(2000.0, 1000.0, 0.0), vec3(2000.0, 4000.0, 0.0)),
            innen(vec3(1500.0, 5000.0, 0.0), vec3(6000.0, 5000.0, 0.0)),
            innen(vec3(5000.0, 1000.0, 0.0), vec3(5000.0, 3000.0, 0.0)),
            innen(vec3(7000.0, 6500.0, 0.0), vec3(9000.0, 6500.0, 0.0)),
        ];
        (m, w)
    }

    fn ereignis(t: &mut EckeWahl, s: &mut Scene, c: &Camera, e: Event) -> Outcome {
        t.handle(&e, s, c, W, H).0
    }

    fn klick(t: &mut EckeWahl, s: &mut Scene, c: &Camera, p: Vec3) -> Outcome {
        let (x, y) = c.project(p, W, H).unwrap();
        ereignis(
            t,
            s,
            c,
            Event::MouseMove {
                x,
                y,
                mods: Default::default(),
            },
        );
        ereignis(
            t,
            s,
            c,
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: Default::default(),
            },
        )
    }

    fn enden(s: &Scene, w: ElementId) -> (Vec3, Vec3) {
        let m = s.model();
        let (run, _) = m.segment_of(w).unwrap();
        let p = &m.run(run).unwrap().points;
        (p[0], p[p.len() - 1])
    }

    fn zeige(t: &mut EckeWahl, s: &mut Scene, c: &Camera, p: Vec3) -> Outcome {
        let (x, y) = c.project(p, W, H).unwrap();
        let mods = Default::default();
        ereignis(t, s, c, Event::MouseMove { x, y, mods })
    }

    fn dicke_innen(s: &Scene) -> f64 {
        let m = s.model();
        m.wall_layers(m.defaults().interior_wall)
            .iter()
            .map(|l| l.thickness)
            .sum()
    }

    /// Ecke bilden: Pille und Vorschau über der zweiten Wand, Klick bildet
    /// die Ecke in einem Schritt, das Werkzeug ist danach fertig.
    #[test]
    fn ecke_mit_vorschau_und_einem_schritt() {
        let c = cam();
        let (mut s, [a, b, ..]) = haus();
        let mut t = EckeWahl::start(&s, a, Art::Ecke).unwrap();
        zeige(&mut t, &mut s, &c, vec3(5000.0, 5020.0, 0.0));
        let nr = s.model().element(b).unwrap().number.clone();
        assert_eq!(t.pill(&s), Some((format!("Ecke mit {nr}"), false)));
        assert!(!t.helpers(&s, 1.0, true, s.theme()).is_empty(), "Vorschau");
        let out = klick(&mut t, &mut s, &c, vec3(5000.0, 5020.0, 0.0));
        assert_eq!(out.schritt, Some(a));
        assert!(out.fertig);
        let x = vec3(2000.0, 5000.0, 0.0);
        assert!((enden(&s, a).1 - x).length() < 1e-6, "{:?}", enden(&s, a));
        assert!((enden(&s, b).0 - x).length() < 1e-6, "{:?}", enden(&s, b));
        assert_eq!(s.undo_label(), Some("Ecke gebildet"));
    }

    /// Außenwand ohne freies Ende: kein Start.
    #[test]
    fn aussenwand_startet_nicht() {
        let (s, _) = haus();
        let m = s.model();
        let aw = m.wall_at(m.runs().ids().next().unwrap(), 0).unwrap();
        assert_eq!(
            EckeWahl::start(&s, aw, Art::Ecke).unwrap_err(),
            EckeFehler::KeinEnde
        );
    }

    /// Verlängern: erst bis zur Zielwand, dann weitere Wände bis dorthin;
    /// parallel lehnt mit Grund ab, Esc beendet.
    #[test]
    fn verlaengern_und_weitere_bis_zum_ziel() {
        let c = cam();
        let (mut s, [a, b, d, e]) = haus();
        let ti = dicke_innen(&s);
        let mut t = EckeWahl::start(&s, a, Art::Verlaengern).unwrap();
        let out = klick(&mut t, &mut s, &c, vec3(4000.0, 5010.0, 0.0));
        assert_eq!(out.schritt, Some(a));
        assert!(!out.fertig);
        let nr = s.model().element(b).unwrap().number.clone();
        assert_eq!(out.karte.unwrap()[0], format!("Weitere Wände bis {nr}"));
        let soll = vec3(2000.0, 5000.0 - ti / 2.0, 0.0);
        assert!(
            (enden(&s, a).1 - soll).length() < 1e-6,
            "{:?}",
            enden(&s, a)
        );
        // Weitere Wand bis zur selben Zielwand
        let out = klick(&mut t, &mut s, &c, vec3(5000.0, 2000.0, 0.0));
        assert_eq!(out.schritt, Some(d), "{out:?}");
        assert!((enden(&s, d).1.y - (5000.0 - ti / 2.0)).abs() < 1e-6);
        // Parallel zur Zielwand: Pille nennt den Grund, der Klick lehnt ab
        zeige(&mut t, &mut s, &c, vec3(8000.0, 6520.0, 0.0));
        let grund = EckeFehler::Parallel.message().to_string();
        assert_eq!(t.pill(&s), Some((grund, false)));
        let out = klick(&mut t, &mut s, &c, vec3(8000.0, 6520.0, 0.0));
        assert_eq!(out.refused, Some(EckeFehler::Parallel));
        assert!(enden(&s, e).0.x == 7000.0);
        let esc = Event::Key {
            key: Key::Escape,
            down: true,
            mods: Default::default(),
            repeat: false,
        };
        let out = ereignis(&mut t, &mut s, &c, esc);
        assert!(out.fertig && !out.cancelled);
    }

    /// Verlängern in einer Flucht mit gleichem Typ: Die Pille sagt
    /// „vereint“, der Klick macht beide Wände zu einer in einem Schritt; die
    /// vereinte Wand ist danach die Zielwand für weitere (G5c).
    #[test]
    fn verlaengern_vereint_in_einer_flucht() {
        let c = cam();
        let (mut m, [_, b, d, _]) = modell();
        let iw = m.defaults().interior_wall;
        let st = m.defaults().storey;
        let r = m
            .add_wall_run(
                &[vec3(7000.0, 5000.0, 0.0), vec3(9000.0, 5000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let f = m.wall_at(r, 0).unwrap();
        let mut s = Scene::with_model(m);
        let nr = s.model().element(b).unwrap().number.clone();
        let mut t = EckeWahl::start(&s, f, Art::Verlaengern).unwrap();
        zeige(&mut t, &mut s, &c, vec3(4000.0, 5010.0, 0.0));
        assert_eq!(t.pill(&s), Some((format!("mit {nr} vereint"), false)));
        let out = klick(&mut t, &mut s, &c, vec3(4000.0, 5010.0, 0.0));
        assert_eq!(out.schritt, Some(b), "{out:?}");
        assert_eq!(out.karte.unwrap()[0], format!("Weitere Wände bis {nr}"));
        assert!(s.model().segment_of(f).is_none(), "f geht in b auf");
        assert!((enden(&s, b).1 - vec3(9000.0, 5000.0, 0.0)).length() < 1e-6);
        assert_eq!(s.undo_label(), Some("Wände vereint"));
        // Weitere Wand bis zur vereinten
        let out = klick(&mut t, &mut s, &c, vec3(5000.0, 2000.0, 0.0));
        assert_eq!(out.schritt, Some(d), "{out:?}");
        let ti = dicke_innen(&s);
        assert!((enden(&s, d).1.y - (5000.0 - ti / 2.0)).abs() < 1e-6);
    }

    /// Geteilte Wand: Stück 0 startet; Verlängern läuft am freien Ende des
    /// Zuges, die Fuge bleibt (Review #82 4).
    #[test]
    fn geteilte_wand_verlaengert_am_zugende() {
        let c = cam();
        let (mut m, [a, ..]) = modell();
        m.split_wall(a, &[1000.0]).unwrap();
        let mut s = Scene::with_model(m);
        let mut t = EckeWahl::start(&s, a, Art::Verlaengern).unwrap();
        let out = klick(&mut t, &mut s, &c, vec3(4000.0, 5010.0, 0.0));
        assert_eq!(out.schritt, Some(a), "{out:?}");
        let ti = dicke_innen(&s);
        let (run, _) = s.model().segment_of(a).unwrap();
        let p = s.model().run(run).unwrap().points.clone();
        assert_eq!(p.len(), 3);
        assert!((p[1] - vec3(2000.0, 2000.0, 0.0)).length() < 1e-6, "{p:?}");
        assert!((p[2] - vec3(2000.0, 5000.0 - ti / 2.0, 0.0)).length() < 1e-6);
        assert_eq!(s.undo_label(), Some("Wand verlängert"));
    }

    /// Eine im Baum gewählte Wand eines anderen Geschosses startet nicht:
    /// Gezeigt und getroffen wird nur das aktive (Review #82 H2).
    #[test]
    fn nur_im_aktiven_geschoss() {
        let (mut m, _) = modell();
        let b = m.add_building(0);
        let st = m.ground_of(Some(b)).unwrap();
        let iw = m.defaults().interior_wall;
        let r = m
            .add_wall_run(
                &[vec3(20000.0, 0.0, 0.0), vec3(23000.0, 0.0, 0.0)],
                false,
                RefSide::Center,
                st,
                iw,
                Category::InteriorWall,
            )
            .unwrap();
        let w = m.wall_at(r, 0).unwrap();
        let s = Scene::with_model(m);
        assert_ne!(s.active_storey(), st);
        assert_eq!(
            EckeWahl::start(&s, w, Art::Verlaengern).unwrap_err(),
            EckeFehler::Geschoss
        );
    }
}
