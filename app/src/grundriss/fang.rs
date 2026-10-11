//! Fang an Wänden beim Zeichnen einer Innenwand (Grundriss G1, Jörn 11.10.
//! 16:47): Wandflächen und freie Wandenden des aktiven Geschosses ziehen den
//! Cursor an. Die gefangene Fläche leuchtet über ihre ganze Länge, die Marke
//! rastet mit einem kurzen Puls ein und haftet: Sie greift ab [`SNAP_PX`] und
//! lässt erst ab [`HALT_PX`] wieder los. Ein Klick auf eine quer liegende
//! Fläche oder ein freies Ende setzt den letzten Punkt und beendet den Zug.
//! Vor dem ersten Punkt nennt die Pille auf einer Fläche das lichte Maß zur
//! nächsten Wand (G2); eine Ziffer öffnet die Eingabe „licht“. Teil von
//! [`super::WallTool`].

use super::*;
use sk_model::Wandflaeche;
use std::time::Duration;

/// Haftung: Ein gefangenes Ziel lässt erst ab diesem Abstand los (px bei 96 dpi).
pub(super) const HALT_PX: f64 = 20.0;

/// Dauer des Einrast-Pulses.
const PULS: Duration = Duration::from_millis(160);

/// Unter diesem Sinus (etwa 10°) läuft die neue Wand parallel zur Fläche und
/// setzt sich dort nicht ab.
const PARALLEL_SIN: f64 = 0.17;

/// Ab diesem Kosinus (etwa 10°) steht eine Wandfläche der neuen Wand
/// gegenüber und begrenzt das lichte Maß.
const GEGENUEBER_COS: f64 = 0.98;

/// Abstand des Suchstrahls für das lichte Maß von der Fläche (mm): knapp im
/// Raum, damit Wände, die an der Fläche stoßen, getroffen werden.
const STRAHL_MM: f64 = 10.0;

/// Gefangenes Ziel: Index in [`Fangziele::flaechen`] bzw. [`Fangziele::enden`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Ziel {
    Flaeche(usize),
    Ende(usize),
}

/// Stand der Fangziele: Modellrevision und Geschoss.
pub(super) type FangStand = (u64, StoreyId);

/// Nächste Wand längs der gefangenen Fläche (G2), Bezug für das lichte Maß
/// vor dem ersten Punkt. Die neue Wand läuft senkrecht von der Fläche weg.
#[derive(Clone, Copy, Debug)]
pub(super) struct Nachbar {
    /// Punkt auf der Linie der Fläche an der Wand gegenüber bzw. an der Ecke.
    b: Vec3,
    /// Einheitsrichtung längs der Fläche von `b` zur neuen Wand.
    pub(super) dir: Vec3,
    /// Normale der Fläche, die Richtung des ersten Segments.
    n: Vec3,
    /// `b` liegt an einer gegenüberliegenden Wandfläche (lichtes Maß), sonst
    /// am Ende der Fläche.
    licht: bool,
}

impl Nachbar {
    /// Feld der Eingabe.
    pub(super) fn etikett(self) -> &'static str {
        if self.licht {
            "licht"
        } else {
            "ab Ecke"
        }
    }
}

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

    /// Strecke der gefangenen Wandfläche zum Hervorheben: der Abschnitt
    /// zwischen den Wänden, die an ihr stoßen (G2), sonst die ganze Fläche.
    pub(super) fn ziel_strecke(&self) -> Option<(Vec3, Vec3)> {
        let c = self.cursor.as_ref()?;
        match c.ziel? {
            Ziel::Flaeche(i) => {
                let f = self.fang.flaechen.get(i)?;
                let r = f.richtung();
                let (l, _) = self.laengs(f, c.pos, r * -1.0);
                let (w, _) = self.laengs(f, c.pos, r);
                Some((c.pos - r * l, c.pos + r * w))
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

    /// Abstand der zugewandten Fläche einer neuen Wand von ihrer Bezugslinie
    /// längs `dir`, wenn sie in Richtung `n` gezeichnet wird.
    fn nah(&self, n: Vec3, dir: Vec3) -> f64 {
        let dicke: f64 = self.layers.iter().map(|l| l.thickness).sum();
        let (lo, hi) = self.ref_side.span(dicke);
        // rechts der Zeichenrichtung (Versatz positiv), längs `dir` gemessen
        let k = (perp(n) * -1.0).dot(dir);
        (k * lo).min(k * hi)
    }

    /// Anfang der neuen Wand mit dem lichten Maß `licht` zum Nachbarn.
    pub(super) fn anfang_bei(&self, nb: Nachbar, licht: f64) -> Vec3 {
        nb.b + nb.dir * (licht - self.nah(nb.n, nb.dir))
    }

    /// Abstand von `p` auf der Fläche `f` längs `s` bis zur nächsten Wand, die
    /// der Fläche gegenüber steht (`true`), sonst bis zum Ende der Fläche.
    /// Gesucht wird knapp im Raum, damit Wände treffen, die an `f` stoßen.
    fn laengs(&self, f: &Wandflaeche, p: Vec3, s: Vec3) -> (f64, bool) {
        let ende = self.auf_ebene(if s.dot(f.richtung()) > 0.0 { f.b } else { f.a });
        let mut weit = (ende - p).dot(s).max(0.0);
        let mut licht = false;
        let strahl = p + f.normale * STRAHL_MM;
        for g in &self.fang.flaechen {
            if g.normale.dot(s) > -GEGENUEBER_COS {
                continue;
            }
            let a = self.auf_ebene(g.a);
            let e = self.auf_ebene(g.b) - a;
            let k = cross2(s, e);
            if k.abs() < 1e-9 {
                continue;
            }
            let t = cross2(a - strahl, e) / k;
            let u = cross2(a - strahl, s) / k;
            // Gleichstand mit dem Ende: die Wand an der Ecke zählt
            if t > -1.0 && (0.0..=1.0).contains(&u) && t <= weit + 0.5 {
                (weit, licht) = (t.max(0.0), true);
            }
        }
        (weit, licht)
    }

    /// Nächste Wand längs der gefangenen Fläche auf beiden Seiten des Cursors:
    /// eine gegenüberliegende Wandfläche oder sonst das Ende der Fläche. Die
    /// Seite mit dem kleineren lichten Maß gewinnt; dazu das lichte Maß.
    fn nachbar(&self) -> Option<(Nachbar, f64)> {
        let c = self.cursor.as_ref()?;
        let Ziel::Flaeche(i) = c.ziel? else {
            return None;
        };
        let f = self.fang.flaechen.get(i)?;
        let (p, r, n) = (c.pos, f.richtung(), f.normale);
        let seite = |s: Vec3| {
            let (weit, licht) = self.laengs(f, p, s);
            let nb = Nachbar {
                b: p + s * weit,
                dir: s * -1.0,
                n,
                licht,
            };
            (nb, weit + self.nah(n, nb.dir))
        };
        let (l, r) = (seite(r * -1.0), seite(r));
        Some(if l.1 <= r.1 { l } else { r })
    }

    /// Ist die Eingabe „licht“ vor dem ersten Punkt offen?
    pub(super) fn abstand_offen(&self) -> bool {
        self.abstand.is_some()
            && self.input.is_some()
            && self.points.is_empty()
            && self.ext.is_none()
    }

    /// Öffnet vor dem ersten Punkt auf einer Wandfläche mit einer Ziffer die
    /// Eingabe „licht“ (zur nächsten Wand, sonst „ab Ecke“).
    pub(super) fn oeffnet_abstand(&mut self, ch: char, mods: Modifiers) -> bool {
        if !self.points.is_empty() || !self.faengt_waende() || !opens(ch, MeasureKind::Length, mods)
        {
            return false;
        }
        let Some((nb, _)) = self.nachbar() else {
            return false;
        };
        let mut i = MeasureInput::new(MeasureKind::Length, None);
        i.push(ch);
        self.input = Some(i);
        self.abstand = Some(nb);
        true
    }

    /// Pille vor dem ersten Punkt: Eingabe „licht“ oder, auf einer
    /// Wandfläche, ihr Name und das lichte Maß zur nächsten Wand. Die Pille
    /// spannt die Lücke zwischen der Wand gegenüber und der neuen Wand und
    /// steht auf der Raumseite der Fläche.
    pub(super) fn label_vor_start(&self) -> Option<([Vec3; 2], String)> {
        let (nb, licht, text) = match (&self.input, self.abstand) {
            (Some(i), Some(nb)) if self.abstand_offen() => {
                let licht = i.value(0).and_then(Result::ok).unwrap_or(0.0);
                (nb, licht, i.text(self.labels()))
            }
            _ => {
                let (nb, licht) = self.nachbar()?;
                let mass = format!("{} m", crate::ui::m_text(licht.max(0.0)));
                let mass = if nb.licht {
                    format!("licht {mass}")
                } else {
                    format!("{mass} ab Ecke")
                };
                let text = match self.ziel_name() {
                    Some(name) => format!("an {name} · {mass}"),
                    None => mass,
                };
                (nb, licht, text)
            }
        };
        let (von, bis) = (nb.b, nb.b + nb.dir * licht.max(0.0));
        // Die App setzt die Pille links der Richtung (bei Bezug rechts:
        // rechts); die Enden so legen, dass das die Raumseite ist
        let links = if self.ref_side == RefSide::Right {
            -1.0
        } else {
            1.0
        };
        let d = bis - von;
        if d.length() < 1.0 {
            return Some(([von, bis], text));
        }
        Some(if perp(d).dot(nb.n) * links >= 0.0 {
            ([von, bis], text)
        } else {
            ([bis, von], text)
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
    /// von der Innenfläche unten bis 4 m hoch; Werkzeug „Innenwand“ mit dem
    /// Aufbau dieser Innenwand. Dazu die Dicke der Außenwand.
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
        w.set_category(Category::InteriorWall, m.wall_layers(iw));
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

    fn dicke(t: &WallTool) -> f64 {
        t.layers.iter().map(|l| l.thickness).sum()
    }

    /// G2: Vor dem ersten Punkt nennt die Pille das lichte Maß zur näheren
    /// Wand bis zur Fläche der neuen Wand; Ziffer und Enter setzen es genau,
    /// das erste Segment fängt senkrecht zur Fläche.
    #[test]
    fn lichtes_mass_zur_ecke() {
        let c = cam();
        let (mut t, d) = haus();
        let ti = dicke(&t);
        assert!(ti > 50.0, "Innenwand mit Aufbau");
        zeige(&mut t, &c, vec3(d + 30.0, 6500.0, 0.0));
        // Innenfläche oben bei y = 8000 − d; Achse bei 6500, Fläche ti/2 höher
        let licht = 8000.0 - d - 6500.0 - ti / 2.0;
        let ([a, b], text) = t.label().expect("Pille auf der Fläche");
        let soll = format!("an AW-001 innen · licht {} m", crate::ui::m_text(licht));
        assert_eq!(text, soll);
        assert!(
            ((b - a).length() - licht).abs() < 1e-6,
            "Pille spannt die Lücke"
        );
        for k in [Key::Char('1'), Key::Char(','), Key::Char('5'), Key::Enter] {
            taste(&mut t, &c, k);
        }
        let s = t.points()[0];
        assert!(
            (s.x - d).abs() < 1e-6 && (s.y - (8000.0 - d - 1500.0 - ti / 2.0)).abs() < 1e-6,
            "{s:?}"
        );
        assert_eq!(t.start_dir.map(|v| v.x.abs() < 1e-9), Some(true));
    }

    /// G2 (Hinweis Geometrie zu #76): Die Innenfläche der Außenwand leuchtet
    /// nur zwischen den Wänden, die an ihr stoßen.
    #[test]
    fn flaeche_leuchtet_bis_zur_innenwand() {
        let c = cam();
        let (mut t, d) = haus();
        let ti = dicke(&t);
        zeige(&mut t, &c, vec3(4000.0, d + 30.0, 0.0));
        let (a, b) = t.ziel_strecke().unwrap();
        let (a, b) = if a.x < b.x { (a, b) } else { (b, a) };
        assert!((a.x - d).abs() < 1e-6, "{a:?}");
        assert!((b.x - (5000.0 - ti / 2.0)).abs() < 1e-6, "{b:?}");
        zeige(&mut t, &c, vec3(7000.0, d + 30.0, 0.0));
        let (a, b) = t.ziel_strecke().unwrap();
        let (a, b) = if a.x < b.x { (a, b) } else { (b, a) };
        assert!((a.x - (5000.0 + ti / 2.0)).abs() < 1e-6, "{a:?}");
        assert!((b.x - (10000.0 - d)).abs() < 1e-6, "{b:?}");
    }

    /// G2: Steht eine Innenwand näher als die Ecke, gilt das lichte Maß zu
    /// ihrer zugewandten Fläche; auch mit Bezugsseite links.
    #[test]
    fn lichtes_mass_zur_innenwand() {
        let c = cam();
        let (mut t, d) = haus();
        let ti = dicke(&t);
        zeige(&mut t, &c, vec3(4000.0, d + 30.0, 0.0));
        let (_, text) = t.label().unwrap();
        let licht = crate::ui::m_text(1000.0 - ti);
        assert_eq!(text, format!("an AW-004 innen · licht {licht} m"));
        assert_eq!(t.labels(), LABELS, "ohne Eingabe Länge und Winkel");
        for k in [Key::Char('0'), Key::Char(','), Key::Char('5')] {
            taste(&mut t, &c, k);
        }
        assert_eq!(t.labels(), ["licht", ""]);
        taste(&mut t, &c, Key::Enter);
        let s = t.points()[0];
        // Ostfläche der neuen Wand 0,50 m vor der Westfläche der Innenwand
        assert!(
            (s.x + ti / 2.0 - (5000.0 - ti / 2.0 - 500.0)).abs() < 1e-6,
            "{s:?}"
        );
        assert!((s.y - d).abs() < 1e-6, "{s:?}");

        // Bezugsseite links, nach Norden gezeichnet: Körper rechts (Osten)
        let (mut t, d) = haus();
        t.ref_side = RefSide::Left;
        zeige(&mut t, &c, vec3(4000.0, d + 30.0, 0.0));
        let (_, text) = t.label().unwrap();
        // Bezugslinie bei x = 4000, Ostfläche bei 4000 + ti
        let licht = crate::ui::m_text(1000.0 - 1.5 * ti);
        assert!(text.ends_with(&format!("licht {licht} m")), "{text}");
        for k in [Key::Char('0'), Key::Char(','), Key::Char('5'), Key::Enter] {
            taste(&mut t, &c, k);
        }
        let s = t.points()[0];
        assert!(
            (s.x + ti - (5000.0 - ti / 2.0 - 500.0)).abs() < 1e-6,
            "{s:?}"
        );
    }
}
