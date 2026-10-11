//! Fang an Wänden beim Zeichnen einer Innenwand (Grundriss G1, Jörn 11.10.
//! 16:47): Wandflächen und freie Wandenden des aktiven Geschosses ziehen den
//! Cursor an. Die gefangene Fläche leuchtet über ihre ganze Länge, die Marke
//! rastet mit einem kurzen Puls ein und haftet: Sie greift ab [`SNAP_PX`] und
//! lässt erst ab [`HALT_PX`] wieder los. Ein Klick auf eine quer liegende
//! Fläche oder ein freies Ende setzt den letzten Punkt und beendet den Zug.
//! Vor dem ersten Punkt öffnet eine Ziffer auf einer Fläche die Eingabe
//! „Abstand ab Ecke“. Teil von [`super::WallTool`].

use super::*;
use std::time::Duration;

/// Haftung: Ein gefangenes Ziel lässt erst ab diesem Abstand los (px bei 96 dpi).
pub(super) const HALT_PX: f64 = 20.0;

/// Dauer des Einrast-Pulses.
const PULS: Duration = Duration::from_millis(160);

/// Unter diesem Sinus (etwa 10°) läuft die neue Wand parallel zur Fläche und
/// setzt sich dort nicht ab.
const PARALLEL_SIN: f64 = 0.17;

/// Gefangenes Ziel: Index in [`Fangziele::flaechen`] bzw. [`Fangziele::enden`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Ziel {
    Flaeche(usize),
    Ende(usize),
}

/// Stand der Fangziele: Modellrevision und Geschoss.
pub(super) type FangStand = (u64, StoreyId);

impl WallTool {
    /// Holt die Fangkanten des Hintergrunds (E16) und die Fangziele an den
    /// Wänden des aktiven Geschosses, nur bei eingeschaltetem Werkzeug und nur
    /// nach Modell- oder Geschosswechsel neu (Einschalten setzt den Stand zurück).
    pub fn fang_aus(&mut self, scene: &mut Scene) {
        let stand = (scene.model().revision(), scene.active_storey());
        if !self.enabled || self.fang_stand == Some(stand) {
            return;
        }
        self.fang_stand = Some(stand);
        self.snaps = scene.background_snaps();
        self.fang = scene.model().fangziele(stand.1);
    }

    /// Braucht die App weitere Bilder für den Einrast-Puls?
    pub fn pulsiert(&self) -> bool {
        self.cursor.as_ref().is_some_and(|c| c.ziel.is_some())
            && self.eingerastet.is_some_and(|t| t.elapsed() < PULS)
    }

    /// Vergrößerung von Marke und Flächenlinie: 1,8 beim Einrasten, dann
    /// zurück auf 1. Ohne Bewegung (Einstellung) kein Puls.
    pub(super) fn puls(&self, theme: &Theme) -> f32 {
        let Some(t) = self.eingerastet.filter(|_| theme.size.anim_ms > 0.0) else {
            return 1.0;
        };
        let k = (t.elapsed().as_secs_f64() / PULS.as_secs_f64()).min(1.0);
        (1.0 + 0.8 * (1.0 - k).powi(2)) as f32
    }

    /// Fängt das Werkzeug an Wänden? Nur „Innenwand“: Außenwände sind ein
    /// geschlossener Ring, Erweiterungen fangen wie bisher.
    fn faengt_waende(&self) -> bool {
        self.category == Category::InteriorWall && self.ext.is_none()
    }

    fn auf_ebene(&self, p: Vec3) -> Vec3 {
        vec3(p.x, p.y, self.z)
    }

    /// Punkt des Ziels zur Maus `raw`. Mit erzwungener Richtung `l` der
    /// Schnitt der Richtung mit der Fläche (`None`, wenn parallel oder
    /// daneben), sonst der nächste Punkt der Fläche.
    fn ziel_punkt(&self, z: Ziel, raw: Vec3, l: Option<&Line>) -> Option<Vec3> {
        match z {
            Ziel::Ende(i) => self.fang.enden.get(i).map(|e| self.auf_ebene(e.p)),
            Ziel::Flaeche(i) => {
                let f = self.fang.flaechen.get(i)?;
                let Some(l) = l else {
                    return Some(self.auf_ebene(f.naechster(raw)));
                };
                let (a, b) = (self.auf_ebene(f.a), self.auf_ebene(f.b));
                let e = b - a;
                let c = cross2(l.dir, e);
                if c.abs() < 1e-9 * e.length() {
                    return None;
                }
                let u = cross2(a - l.origin, l.dir) / c;
                (0.0..=1.0).contains(&u).then(|| a + e * u)
            }
        }
    }

    /// Bestes Wandziel in Reichweite: freie Enden vor Flächen, die gehaltene
    /// Fläche vor anderen Flächen. Das gehaltene Ziel lässt erst ab
    /// [`HALT_PX`] los.
    pub(super) fn wand_treffer(
        &self,
        raw: Vec3,
        l: Option<&Line>,
        px: &dyn Fn(Vec3) -> f64,
        scale: f64,
    ) -> Option<(Vec3, Ziel)> {
        if !self.faengt_waende() {
            return None;
        }
        let gehalten = self.cursor.as_ref().and_then(|c| c.ziel);
        let treffer = |z: Ziel| {
            let px_max = if Some(z) == gehalten {
                HALT_PX
            } else {
                SNAP_PX
            };
            let reichweite = px_max * scale;
            let p = self.ziel_punkt(z, raw, l)?;
            let d = px(p);
            (d < reichweite).then_some((d, p, z))
        };
        let naechster = |it: &mut dyn Iterator<Item = Ziel>| {
            it.filter_map(treffer)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, p, z)| (p, z))
        };
        if let Some(t) = naechster(&mut (0..self.fang.enden.len()).map(Ziel::Ende)) {
            return Some(t);
        }
        if let Some(t) = gehalten
            .filter(|z| matches!(z, Ziel::Flaeche(_)))
            .and_then(treffer)
        {
            return Some((t.1, t.2));
        }
        naechster(&mut (0..self.fang.flaechen.len()).map(Ziel::Flaeche))
    }

    /// Richtung der gefangenen Fläche (für die Richtungen des ersten Segments).
    pub(super) fn flaeche_dir(&self, z: Option<Ziel>) -> Option<Vec3> {
        match z? {
            Ziel::Flaeche(i) => self.fang.flaechen.get(i).map(|f| f.richtung()),
            Ziel::Ende(_) => None,
        }
    }

    /// Endet der Zug am eben gesetzten Punkt? An einem freien Wandende immer,
    /// an einer Wandfläche, wenn die neue Wand nicht an ihr entlang läuft.
    pub(super) fn setzt_ab(&self, c: &Cursor, last: Vec3) -> bool {
        match c.ziel {
            Some(Ziel::Ende(_)) => true,
            Some(Ziel::Flaeche(i)) => {
                let d = vec3(c.pos.x - last.x, c.pos.y - last.y, 0.0);
                let f = &self.fang.flaechen[i];
                d.length() > 1.0 && cross2(d.normalized(), f.richtung()).abs() > PARALLEL_SIN
            }
            None => false,
        }
    }

    /// Strecke der gefangenen Wandfläche (zum Hervorheben).
    pub(super) fn ziel_strecke(&self) -> Option<(Vec3, Vec3)> {
        match self.cursor.as_ref()?.ziel? {
            Ziel::Flaeche(i) => {
                let f = self.fang.flaechen.get(i)?;
                Some((self.auf_ebene(f.a), self.auf_ebene(f.b)))
            }
            Ziel::Ende(_) => None,
        }
    }

    /// Name des gefangenen Ziels für die Pille, z. B. „AW-003 innen“.
    pub(super) fn ziel_name(&self) -> Option<&str> {
        let name = match self.cursor.as_ref()?.ziel? {
            Ziel::Flaeche(i) => &self.fang.flaechen.get(i)?.name,
            Ziel::Ende(i) => &self.fang.enden.get(i)?.name,
        };
        (!name.is_empty()).then_some(name.as_str())
    }

    /// Ecke der gefangenen Fläche, die der Maus näher liegt, die Richtung von
    /// ihr längs der Fläche und die Normale der Fläche.
    fn ecke_und_richtung(&self) -> Option<[Vec3; 3]> {
        let c = self.cursor.as_ref()?;
        let Ziel::Flaeche(i) = c.ziel? else {
            return None;
        };
        let f = self.fang.flaechen.get(i)?;
        let (a, b) = (self.auf_ebene(f.a), self.auf_ebene(f.b));
        Some(if (c.pos - a).length() <= (c.pos - b).length() {
            [a, f.richtung(), f.normale]
        } else {
            [b, f.richtung() * -1.0, f.normale]
        })
    }

    /// Ist die Eingabe „Abstand ab Ecke“ offen?
    pub(super) fn abstand_offen(&self) -> bool {
        self.abstand.is_some()
            && self.input.is_some()
            && self.points.is_empty()
            && self.ext.is_none()
    }

    /// Öffnet vor dem ersten Punkt auf einer Wandfläche mit einer Ziffer die
    /// Eingabe „Abstand ab Ecke“.
    pub(super) fn oeffnet_abstand(&mut self, ch: char, mods: Modifiers) -> bool {
        if !self.points.is_empty() || !self.faengt_waende() || !opens(ch, MeasureKind::Length, mods)
        {
            return false;
        }
        let Some(ecke) = self.ecke_und_richtung() else {
            return false;
        };
        let mut i = MeasureInput::new(MeasureKind::Length, None);
        i.push(ch);
        self.input = Some(i);
        self.abstand = Some(ecke);
        true
    }

    /// Pille vor dem ersten Punkt: Eingabe „Abstand ab Ecke“ oder, auf einer
    /// Wandfläche, ihr Name und der Abstand ab der näheren Ecke. Die Pille
    /// steht auf der Raumseite der Fläche.
    pub(super) fn label_vor_start(&self) -> Option<([Vec3; 2], String)> {
        let (ecke, ende, n, text) = match (&self.input, self.abstand) {
            (Some(i), Some([ecke, _, n])) if self.abstand_offen() => {
                let ende = self.typed_point().unwrap_or(ecke);
                (ecke, ende, n, i.text(self.labels()))
            }
            _ => {
                let [ecke, _, n] = self.ecke_und_richtung()?;
                let pos = self.cursor.as_ref()?.pos;
                let ab = crate::ui::live_length_text((pos - ecke).length());
                let text = match self.ziel_name() {
                    Some(name) => format!("an {name} · {ab} ab Ecke"),
                    None => format!("{ab} ab Ecke"),
                };
                (ecke, pos, n, text)
            }
        };
        // Die App setzt die Pille links der Richtung (bei Bezug rechts:
        // rechts); die Enden so legen, dass das die Raumseite ist
        let links = if self.ref_side == RefSide::Right {
            -1.0
        } else {
            1.0
        };
        let d = ende - ecke;
        if d.length() < 1.0 {
            return Some(([ecke, ende], text));
        }
        Some(if perp(d).dot(n) * links >= 0.0 {
            ([ecke, ende], text)
        } else {
            ([ende, ecke], text)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::Model;

    const W: f64 = 1200.0;
    const H: f64 = 800.0;

    fn cam() -> Camera {
        Camera::looking_at(
            vec3(5000.0, -6000.0, 18000.0),
            vec3(5000.0, 4000.0, 0.0),
            45.0,
        )
    }

    /// Haus 10 × 8 m (Außenwand, Bezug außen) mit einer Innenwand bei x = 5 m
    /// von der Innenfläche unten bis 4 m hoch; Werkzeug „Innenwand“.
    fn haus() -> (WallTool, f64) {
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
        let t: f64 = m
            .layer_set(aw)
            .map(|s| s.layers.iter().map(|l| l.thickness).sum())
            .unwrap();
        let iw = m.defaults().interior_wall;
        m.add_wall_run(
            &[vec3(5000.0, t, 0.0), vec3(5000.0, 4000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
        let mut w = WallTool::new();
        w.set_enabled(true);
        w.set_category(Category::InteriorWall, Vec::new());
        w.fang = m.fangziele(st);
        (w, t)
    }

    fn zeige(t: &mut WallTool, c: &Camera, p: Vec3) {
        t.mouse = c.project(p, W, H);
        t.refresh(c, W, H, 1.0);
    }

    fn klick(t: &mut WallTool, c: &Camera, p: Vec3) -> Outcome {
        let (x, y) = c.project(p, W, H).unwrap();
        t.handle(
            &Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: Modifiers::default(),
            },
            c,
            W,
            H,
            1.0,
        )
    }

    fn taste(t: &mut WallTool, c: &Camera, key: Key) {
        t.handle(
            &Event::Key {
                key,
                down: true,
                mods: Modifiers::default(),
                repeat: false,
            },
            c,
            W,
            H,
            1.0,
        );
    }

    /// Jörn: Der Cursor schnappt an der Innenseite der Außenwand ein, für den
    /// Anfang wie für das Ende; am Ende ist die Wand abgesetzt.
    #[test]
    fn faengt_an_der_innenseite_und_setzt_ab() {
        let c = cam();
        let (mut t, d) = haus();
        // Anfang knapp neben der Innenseite der linken Außenwand
        zeige(&mut t, &c, vec3(d + 40.0, 6000.0, 0.0));
        let cur = t.cursor.clone().unwrap();
        assert_eq!(cur.kind, SnapKind::Wand);
        assert!((cur.pos.x - d).abs() < 1e-6, "{:?}", cur.pos);
        assert_eq!(t.ziel_name(), Some("AW-001 innen"));
        assert!(t.pulsiert());
        assert!(klick(&mut t, &c, vec3(d + 40.0, 6000.0, 0.0))
            .commit
            .is_none());
        // Erstes Segment senkrecht zur Fläche; das Ende knapp vor der
        // Innenseite der rechten Außenwand fängt dort und setzt ab
        let out = klick(&mut t, &c, vec3(10000.0 - d - 40.0, 6030.0, 0.0));
        let w = out.commit.expect("Zug endet an der Wandfläche");
        assert_eq!(w.points.len(), 2);
        assert!((w.points[0].x - d).abs() < 1e-6);
        assert!(
            (w.points[1].x - (10000.0 - d)).abs() < 1e-6,
            "{:?}",
            w.points[1]
        );
        assert!((w.points[1].y - 6000.0).abs() < 1e-6, "{:?}", w.points[1]);
        assert!(!t.is_active());
    }

    /// Ein Klick auf eine Fläche, an der die neue Wand entlang läuft, setzt
    /// nur einen Punkt.
    #[test]
    fn parallel_zur_flaeche_kein_absetzen() {
        let c = cam();
        let (mut t, d) = haus();
        t.ortho = false;
        klick(&mut t, &c, vec3(d + 600.0, 1000.0, 0.0));
        // an der Innenseite, aber fast parallel zu ihr gezogen
        let out = klick(&mut t, &c, vec3(d + 20.0, 7500.0, 0.0));
        assert_eq!(t.cursor.as_ref().unwrap().kind, SnapKind::Wand);
        assert!(out.commit.is_none());
        assert!(t.is_active());
    }

    /// Das freie Ende der Innenwand fängt vor der Fläche daneben und beendet
    /// den Zug.
    #[test]
    fn freies_ende_faengt_und_beendet() {
        let c = cam();
        let (mut t, _) = haus();
        klick(&mut t, &c, vec3(8000.0, 4000.0, 0.0));
        zeige(&mut t, &c, vec3(5030.0, 4020.0, 0.0));
        assert!(matches!(
            t.cursor.as_ref().unwrap().ziel,
            Some(Ziel::Ende(_))
        ));
        let out = klick(&mut t, &c, vec3(5030.0, 4020.0, 0.0));
        let w = out.commit.expect("Zug endet am Wandende");
        assert!((w.points[1] - vec3(5000.0, 4000.0, 0.0)).length() < 1e-6);
    }

    /// Haftung: Das gefangene Ziel greift ab 12 px und lässt erst ab 20 px los.
    #[test]
    fn haftet_bis_zwanzig_pixel() {
        let c = cam();
        let (mut t, d) = haus();
        let auf = vec3(d, 6000.0, 0.0);
        let (ax, ay) = c.project(auf, W, H).unwrap();
        // Bildschirmrichtung von der Fläche weg (in den Raum)
        let (bx, by) = c.project(auf + vec3(100.0, 0.0, 0.0), W, H).unwrap();
        let l = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
        let (ux, uy) = ((bx - ax) / l, (by - ay) / l);
        let bei = |t: &mut WallTool, px: f64| {
            t.mouse = Some((ax + ux * px, ay + uy * px));
            t.refresh(&c, W, H, 1.0);
            t.cursor.as_ref().unwrap().ziel.is_some()
        };
        assert!(!bei(&mut t, 16.0), "greift erst ab 12 px");
        assert!(bei(&mut t, 10.0));
        assert!(bei(&mut t, 16.0), "haftet bis 20 px");
        assert!(!bei(&mut t, 24.0));
    }

    /// Außenwandzug und Erweiterungen fangen nicht an Wänden.
    #[test]
    fn aussenwand_faengt_nicht() {
        let c = cam();
        let (mut t, d) = haus();
        t.set_category(Category::ExteriorWall, Vec::new());
        zeige(&mut t, &c, vec3(d + 40.0, 6000.0, 0.0));
        assert_ne!(t.cursor.as_ref().unwrap().kind, SnapKind::Wand);
    }

    /// Vor dem ersten Punkt: Ziffer auf einer Fläche, Abstand ab der näheren
    /// Ecke; das erste Segment fängt senkrecht zur Fläche.
    #[test]
    fn abstand_ab_ecke() {
        let c = cam();
        let (mut t, d) = haus();
        zeige(&mut t, &c, vec3(d + 30.0, 6500.0, 0.0));
        let (_, text) = t.label().expect("Pille auf der Fläche");
        assert!(text.starts_with("an AW-001 innen · "), "{text}");
        for k in [Key::Char('1'), Key::Char(','), Key::Char('5'), Key::Enter] {
            taste(&mut t, &c, k);
        }
        let s = t.points()[0];
        // Ecke oben links innen bei y = 8000 − d
        assert!(
            (s.x - d).abs() < 1e-6 && (s.y - (8000.0 - d - 1500.0)).abs() < 1e-6,
            "{s:?}"
        );
        assert_eq!(t.start_dir.map(|v| v.x.abs() < 1e-9), Some(true));
    }
}
