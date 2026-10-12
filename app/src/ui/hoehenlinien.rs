//! Höhenlinien im Geschossmanager (Jörn 11.10. 13:57–14:12): In jedem
//! Geschoss stehen Brüstung und Sturz der Öffnungen als untergeordnete,
//! gestrichelte Linien mit kleinem Griff zwischen den Ebenen, dazu die
//! Oberkante der Innentüren (12.10. 05:26); im aktiven Geschoss die
//! Schnitthöhe des Grundrisses. Zwei Schalter in der Titelzeile blenden
//! Fenster- und Innentürlinien getrennt aus. Ziehen meldet wie bei
//! den Ebenen `LevelEvent::Move` mit der neuen Kote. Nur im Fenster, das
//! Paneel neben dem Dialog „Gebäude erstellen“ hat dafür keinen Platz.

use super::*;

/// Kleinste Höhe eines Bands (dip), ab der seine Linien erscheinen;
/// darunter lägen sie auf Namen und Koten.
const MIN_BAND: f32 = 60.0;

/// Untergeordnete Linie: Griff, Kote (mm), Lage im Paneel (Pixel).
pub(super) struct SubLine {
    pub grip: Grip,
    pub z: f64,
    pub y: f32,
}

impl Ui {
    /// Untergeordnete Linien, anteilig zwischen Unter- und Oberkante ihres
    /// Bands (die Darstellung ist gespreizt); `lines`, `ys` wie im Diagramm.
    pub(super) fn sub_lines(&self, lines: &[LevelLine], ys: &[f32]) -> Vec<SubLine> {
        let mut out = Vec::new();
        if !self.fenster {
            return out;
        }
        let s = self.scale;
        for (i, b) in self.levels.bands.iter().enumerate() {
            let (Some(l0), Some(l1), Some(&y0), Some(&y1)) =
                (lines.get(i), lines.get(i + 1), ys.get(i), ys.get(i + 1))
            else {
                continue;
            };
            if l1.z <= l0.z || y0 - y1 < MIN_BAND * s {
                continue;
            }
            let at = |z: f64| y0 + (y1 - y0) * ((z - l0.z) / (l1.z - l0.z)) as f32;
            let mut push = |grip, z: f64| {
                if z > l0.z && z < l1.z {
                    out.push(SubLine { grip, z, y: at(z) });
                }
            };
            if let Some((sill, lintel, door)) = b.lines {
                if self.line_vis[0] {
                    push(Grip::Sill(b.id), sill);
                    push(Grip::Lintel(b.id), lintel);
                }
                if self.line_vis[1] {
                    push(Grip::Door(b.id), door);
                }
            }
            if let Some(z) = self.levels.plan_cut.filter(|_| b.active) {
                push(Grip::PlanCut, z);
            }
        }
        out
    }

    /// Kote eines untergeordneten Griffs.
    pub(super) fn sub_grip_z(&self, g: Grip) -> Option<f64> {
        let lines = |id: StoreyId| {
            self.levels
                .bands
                .iter()
                .find(|b| b.id == id)
                .and_then(|b| b.lines)
        };
        match g {
            Grip::Sill(id) => lines(id).map(|l| l.0),
            Grip::Lintel(id) => lines(id).map(|l| l.1),
            Grip::Door(id) => lines(id).map(|l| l.2),
            Grip::PlanCut => self.levels.plan_cut,
            _ => None,
        }
    }

    /// Klickflächen der Schalter „Fenster“ und „Innentüren“ in der
    /// Titelzeile, links vom Schalter „Flachdach“; nur im Fenster und nur,
    /// wenn rechts vom Titel Platz ist.
    pub(super) fn line_vis_rects(&self) -> Vec<(Id, Rect)> {
        let mut out = Vec::new();
        if !self.fenster {
            return out;
        }
        let (s, z) = (self.scale, &self.size);
        let font = self.fonts.regular.as_ref();
        let gap = 14.0 * s;
        let mut right = match self.flat_roof_rect() {
            Some(r) => r.x - gap,
            None => (self.width(Panel::Levels) - z.panel_pad) * s,
        };
        // Titel „Geschosse“ links
        let least = (z.panel_pad + 90.0) * s;
        for k in [1, 0] {
            let id = Id::LineVis(k);
            let word = check_word(id).unwrap_or_default();
            let tw = font.map_or(60.0 * s, |f| f.width(word, z.font_small * s));
            let w = 20.0 * s + tw;
            if right - w < least {
                return Vec::new();
            }
            out.push((
                id,
                Rect::new(right - w, z.panel_pad * s + 2.0 * s, w, 16.0 * s),
            ));
            right -= w + gap;
        }
        out.reverse();
        out
    }

    /// Zeichnet die untergeordneten Linien: dünn gestrichelt bis vor die
    /// Kette der lichten Höhen, Name und Maß über OK Rohdecke klein darüber,
    /// Griff eingerückt und kleiner als der einer Ebene. `(lo, hi)`:
    /// sichtbarer Bereich des Diagramms.
    pub(super) fn paint_sub_lines(
        &self,
        t: &Theme,
        c: &mut Canvas,
        lines: &[LevelLine],
        ys: &[f32],
        (lo, hi): (f32, f32),
    ) {
        let (u, z, s) = (&t.ui, &self.size, self.scale);
        let m = (z.panel_shadow * s).round();
        let (x0, xi, _) = self.level_columns();
        let w1 = s.round().max(1.0);
        let drag = self.level_drag.map(|d| d.grip);
        let regular = self.fonts.regular.as_ref();
        let px = z.font_detail * s;
        let d = z.level_handle * s * 0.7;
        let gx = x0 + z.level_handle * s * 0.15;
        for sub in self.sub_lines(lines, ys) {
            if sub.y < lo || sub.y > hi {
                continue;
            }
            let dragged = drag == Some(sub.grip);
            let hover = self.hover == Some(Id::Grip(sub.grip));
            let cut = sub.grip == Grip::PlanCut;
            let col = match (dragged, cut) {
                (true, _) => u.level_handle_drag,
                (false, true) => u.level_line_active,
                (false, false) => u.text_dim,
            };
            // Strich 4, Lücke 3 (dip)
            let (x1, dash, gap) = (xi - 12.0 * s, 4.0 * s, 3.0 * s);
            let yl = (sub.y + m - w1 * 0.5).round();
            let mut x = gx + d + 3.0 * s;
            while x < x1 {
                c.fill_rect(x + m, yl, dash.min(x1 - x), w1, col);
                x += dash + gap;
            }
            let band = self.levels.bands.iter().find(|b| match sub.grip {
                Grip::Sill(id) | Grip::Lintel(id) | Grip::Door(id) => b.id == id,
                _ => b.active,
            });
            let ab = band.map_or(0.0, |b| sub.z - b.bottom);
            let name = match sub.grip {
                Grip::Sill(_) => "Brüstung",
                Grip::Lintel(_) => "Sturz",
                Grip::Door(_) => "Innentür",
                _ => "Schnitt",
            };
            let txt = match sub.grip {
                // Fertighöhe ab OK Fertigfußboden, dazu die Rohbauhöhe
                Grip::Door(_) => {
                    let fertig = ab - sk_model::hoehen::FLOOR_BUILDUP;
                    format!("{name} {} (roh {})", m_text(fertig), m_text(ab))
                }
                _ => format!("{name} {}", m_text(ab)),
            };
            let nx = x0 + (z.level_handle + z.level_label_gap) * s;
            widgets::text(c, regular, &txt, px, nx + m, sub.y - 3.0 * s + m, col);
            let hc = if dragged {
                u.level_handle_drag
            } else if hover {
                u.level_handle_hover
            } else {
                col
            };
            // Raute als Griff
            let (cx, cy, r) = (gx + d * 0.5 + m, sub.y + m, d * 0.5);
            let mut p = Path::new();
            p.move_to(cx, cy - r);
            p.line_to(cx + r, cy);
            p.line_to(cx, cy + r);
            p.line_to(cx - r, cy);
            p.close();
            c.fill(&p, hc);
        }
    }
}

/// Wort neben dem Kästchen eines Schalters in der Titelzeile des Paneels
/// „Geschosse“.
pub(super) fn check_word(id: Id) -> Option<&'static str> {
    match id {
        Id::FlatRoof => Some("Flachdach"),
        Id::LineVis(0) => Some("Fenster"),
        Id::LineVis(_) => Some("Innentüren"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn fenster() -> (Ui, Scene) {
        let s = Scene::new();
        let mut ui = Ui::fenster(1.0, &Theme::dark());
        ui.fit_fenster(1.0, 460, 900, 32);
        ui.set_levels(s.levels());
        (ui, s)
    }

    /// Je Geschoss Brüstung und Sturz, im aktiven die Schnitthöhe; der Griff
    /// der Brüstung lässt sich ziehen und meldet die gefangene Kote. Neben
    /// dem Dialog im Hauptfenster keine Linien.
    #[test]
    fn linien_und_griffe_im_fenster() {
        let (mut ui, s) = fenster();
        let lines = level_lines(&ui.levels);
        let ys = ui.line_ys(&lines, &ui.levels_layout());
        let subs = ui.sub_lines(&lines, &ys);
        let eg = ui
            .levels
            .bands
            .iter()
            .find(|b| b.name == "EG")
            .unwrap()
            .clone();
        let grips: Vec<Grip> = subs.iter().map(|u| u.grip).collect();
        assert!(grips.contains(&Grip::Sill(eg.id)) && grips.contains(&Grip::Lintel(eg.id)));
        assert!(grips.contains(&Grip::PlanCut));
        assert!(grips.contains(&Grip::Door(eg.id)));
        assert_eq!(subs.len(), 7, "EG, OG je drei und die Schnitthöhe");
        let door = subs.iter().find(|u| u.grip == Grip::Door(eg.id)).unwrap();
        assert_eq!(door.z, eg.bottom + sk_model::hoehen::DOOR);
        assert_eq!(ui.sub_grip_z(Grip::Door(eg.id)), Some(door.z));
        let sill = subs.iter().find(|u| u.grip == Grip::Sill(eg.id)).unwrap();
        assert_eq!(sill.z, eg.bottom + sk_model::SILL);
        let cut = subs.iter().find(|u| u.grip == Grip::PlanCut).unwrap();
        assert_eq!(cut.z, s.plan_cut());
        assert!(sill.y > cut.y, "Brüstung unter der Schnitthöhe");

        let r = ui.rect(Panel::Levels, 460, 32);
        let (x0, _, _) = ui.level_columns();
        let (x, y) = ((r.x + x0 + 5.0) as f64, (r.y + sill.y) as f64);
        ui.handle(&Event::MouseMove { x, y, mods: M }, 460, 32);
        assert_eq!(ui.hover, Some(Id::Grip(Grip::Sill(eg.id))));
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        };
        let out = ui.handle(&down, 460, 32);
        assert_eq!(out.level, Some(LevelEvent::Begin(Grip::Sill(eg.id))));
        let ppm = ui.levels_layout().ppm as f64;
        let out = ui.handle(
            &Event::MouseMove {
                x,
                y: y - 12.0,
                mods: M,
            },
            460,
            32,
        );
        let z = sill.z + 12.0 / ppm * 1000.0;
        assert_eq!(
            out.level,
            Some(LevelEvent::Move(
                Grip::Sill(eg.id),
                (z / 10.0).round() * 10.0
            ))
        );

        let mut haupt = Ui::new(1.0, &Theme::dark());
        haupt.fit(1.0, 1440, 900);
        haupt.set_levels(s.levels());
        haupt.dialog = true;
        let lines = level_lines(&haupt.levels);
        let ys = haupt.line_ys(&lines, &haupt.levels_layout());
        assert!(haupt.sub_lines(&lines, &ys).is_empty());
        assert!(haupt.line_vis_rects().is_empty());
    }

    /// Die Schalter „Fenster“ und „Innentüren“ (Jörn 05:26) stehen links vom
    /// Schalter „Flachdach“ und blenden ihre Linien je mit einem Klick aus
    /// und wieder ein.
    #[test]
    fn schalter_blenden_linien_aus() {
        let (mut ui, _) = fenster();
        let zahl = |ui: &Ui, f: fn(&Grip) -> bool| {
            let lines = level_lines(&ui.levels);
            let ys = ui.line_ys(&lines, &ui.levels_layout());
            ui.sub_lines(&lines, &ys)
                .iter()
                .filter(|u| f(&u.grip))
                .count()
        };
        let fenster = |g: &Grip| matches!(g, Grip::Sill(_) | Grip::Lintel(_));
        let tuer = |g: &Grip| matches!(g, Grip::Door(_));
        assert_eq!((zahl(&ui, fenster), zahl(&ui, tuer)), (4, 2));
        let rects = ui.line_vis_rects();
        let ids: Vec<Id> = rects.iter().map(|r| r.0).collect();
        assert_eq!(ids, [Id::LineVis(0), Id::LineVis(1)]);
        assert!(rects[0].1.x + rects[0].1.w < rects[1].1.x);
        if let Some(f) = ui.flat_roof_rect() {
            assert!(rects[1].1.x + rects[1].1.w < f.x);
        }
        let r = ui.rect(Panel::Levels, 460, 32);
        let klick = |ui: &mut Ui, k: usize| {
            let b = rects[k].1;
            let (x, y) = ((r.x + b.x + 4.0) as f64, (r.y + b.y + 8.0) as f64);
            ui.handle(&Event::MouseMove { x, y, mods: M }, 460, 32);
            assert_eq!(ui.hover, Some(Id::LineVis(k as u8)));
            assert_eq!(ui.cursor(), Cursor::Hand);
            let (button, mods) = (MouseButton::Left, M);
            ui.handle(&Event::MouseDown { button, x, y, mods }, 460, 32);
            let up = ui.handle(&Event::MouseUp { button, x, y, mods }, 460, 32);
            assert_eq!(up.clicked, Some(Id::LineVis(k as u8)));
        };
        klick(&mut ui, 0);
        assert!(!ui.is_on(Id::LineVis(0)) && ui.is_on(Id::LineVis(1)));
        assert_eq!((zahl(&ui, fenster), zahl(&ui, tuer)), (0, 2));
        klick(&mut ui, 1);
        assert_eq!((zahl(&ui, fenster), zahl(&ui, tuer)), (0, 0));
        klick(&mut ui, 0);
        assert_eq!((zahl(&ui, fenster), zahl(&ui, tuer)), (4, 0));
    }
}
