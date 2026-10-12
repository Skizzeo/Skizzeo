//! Höhenlinien im Geschossmanager (Jörn 11.10. 13:57–14:12): In jedem
//! Geschoss stehen Brüstung und Sturz der Öffnungen als untergeordnete,
//! gestrichelte Linien mit kleinem Griff zwischen den Ebenen; im aktiven
//! Geschoss dazu die Schnitthöhe des Grundrisses. Ziehen meldet wie bei
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
            if let Some((sill, lintel)) = b.lines {
                push(Grip::Sill(b.id), sill);
                push(Grip::Lintel(b.id), lintel);
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
            Grip::PlanCut => self.levels.plan_cut,
            _ => None,
        }
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
                Grip::Sill(id) | Grip::Lintel(id) => b.id == id,
                _ => b.active,
            });
            let ab = band.map_or(0.0, |b| sub.z - b.bottom);
            let name = match sub.grip {
                Grip::Sill(_) => "Brüstung",
                Grip::Lintel(_) => "Sturz",
                _ => "Schnitt",
            };
            let txt = format!("{name} {}", m_text(ab));
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
        assert_eq!(subs.len(), 5, "EG, OG je zwei und die Schnitthöhe");
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
    }
}
