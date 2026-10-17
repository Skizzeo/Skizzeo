//! Paneele, Teil Paneel „Geschosse“ (aus ui.rs, Paket T3).

use super::*;

/// Paneel „Geschosse“ (E14): Diagramm der Ebenen mit Griffen, Koten und
/// Maßketten, in jeder Fenstergröße (Jörn 11.10.: die Zahlenliste im
/// kleinen Fenster entfällt). Es steht nur im Geschossfenster
/// ([`fenster`]); im Hauptfenster ist dort der Knopf (Jörn 12.10.).
impl Ui {
    /// Aufteilung: größter Maßstab, bei dem das Paneel über dem Fensterrand
    /// endet. Beim Ziehen bleibt sie stehen.
    pub(super) fn levels_layout(&self) -> LevelsLayout {
        if let Some(d) = self.level_drag {
            return d.layout;
        }
        // Das Diagramm steht nur noch im Geschossfenster (Jörn 12.10.)
        self.levels_layout_fenster()
    }

    /// Aufteilung für ein Paneel, das bei `y` (Pixel) beginnt, mit dem
    /// Maßstab `ppm` (dip je m) als größtem. Reicht auch der kleinste nicht,
    /// rücken alle Linien auf den Zeilenabstand zusammen; was dann noch
    /// fehlt, schneidet der Fensterrand ab, Linien und Zahlen bleiben im
    /// Paneel.
    pub(super) fn levels_layout_at(&self, y: f32, ppm: f32) -> LevelsLayout {
        let (s, z) = (self.scale, &self.size);
        let m = (z.panel_margin * s).round();
        let avail = (self.win_h - y - m).max(0.0);
        let lines = level_lines(&self.levels);
        let head = z.panel_pad + self.level_head() + LEVEL_TOP;
        let foot = self.level_footer();
        let layout = |ppm: f32, fuss: f32| {
            let (above, below) = self.level_spans(&lines, ppm);
            LevelsLayout {
                ppm,
                anchor: ((head + above) * s).round(),
                height: ((head + above + below + LEVEL_BOTTOM + fuss + z.panel_pad) * s).round(),
                fuss,
            }
        };
        let mut l = layout(ppm, foot);
        while l.height > avail && l.ppm > z.level_px_per_m_min {
            l = layout((l.ppm - 1.0).max(z.level_px_per_m_min), foot);
        }
        if l.height > avail {
            // 1 dip je m: jeder Abstand unter 22 m wird zum Zeilenabstand;
            // reicht auch das nicht, weicht der Fuß mit dem Gelände
            l = layout(1.0, foot);
            if l.height > avail {
                l = layout(1.0, 0.0);
            }
        }
        let least = ((2.0 * z.panel_pad + self.level_head() + l.fuss) * s).round();
        l.height = l.height.min(avail).max(least);
        l
    }

    /// Fuß des Paneels (dip): Kote OK Gelände und Einbindetiefe (Gelände
    /// Thema 1), ohne Gründung nichts.
    fn level_footer(&self) -> f32 {
        if self.levels.terrain.is_some() {
            2.0 * LEVEL_FOOTER_ROW
        } else {
            0.0
        }
    }

    /// Klickfläche des Schalters „Flachdach“ (Kästchen und Wort) rechts in
    /// der Titelzeile; das Diagramm behält seine Höhe.
    pub(super) fn flat_roof_rect(&self) -> Option<Rect> {
        self.levels.flat_roof?;
        let (s, z) = (self.scale, &self.size);
        let w = 14.0 * s
            + 6.0 * s
            + self
                .fonts
                .regular
                .as_ref()
                .map_or(70.0 * s, |f| f.width("Flachdach", z.font_small * s));
        let right = (self.width(Panel::Levels) - z.panel_pad) * s;
        Some(Rect::new(right - w, z.panel_pad * s + 2.0 * s, w, 16.0 * s))
    }

    /// Obere Kante des Fußes (Pixel, ohne Schatten).
    fn footer_top(&self, l: &LevelsLayout) -> f32 {
        l.height - ((l.fuss + self.size.panel_pad) * self.scale).round()
    }

    /// Lage von OK Gelände im Diagramm (Pixel): zwischen den Linien, die
    /// es einschließen, anteilig; außerhalb im Maßstab.
    fn terrain_y(&self, lines: &[LevelLine], ys: &[f32], l: &LevelsLayout) -> Option<f32> {
        let z = self.levels.terrain?;
        let px = l.ppm * self.scale / 1000.0;
        let (first, last) = (lines.first()?, lines.last()?);
        if z <= first.z {
            return Some(ys[0] + (first.z - z) as f32 * px);
        }
        if z >= last.z {
            return Some(ys[ys.len() - 1] - (z - last.z) as f32 * px);
        }
        let i = lines.windows(2).position(|w| z >= w[0].z && z <= w[1].z)?;
        let (z0, z1) = (lines[i].z, lines[i + 1].z);
        let t = if z1 > z0 {
            ((z - z0) / (z1 - z0)) as f32
        } else {
            0.0
        };
        Some(ys[i] + (ys[i + 1] - ys[i]) * t)
    }

    /// Abstand zweier Linien (dip), für die Anzeige auf `level_row_min` gespreizt.
    fn level_gap(&self, dz: f64, ppm: f32) -> f32 {
        (dz as f32 / 1000.0 * ppm).max(self.size.level_row_min)
    }

    /// Strecke von ±0,00 bis zur obersten und zur untersten Linie (dip).
    fn level_spans(&self, lines: &[LevelLine], ppm: f32) -> (f32, f32) {
        let a = anchor_index(lines);
        let gap = |w: &[LevelLine]| self.level_gap(w[1].z - w[0].z, ppm);
        let above = lines[a.min(lines.len())..].windows(2).map(gap).sum();
        let below = lines[..(a + 1).min(lines.len())].windows(2).map(gap).sum();
        (above, below)
    }

    /// Lage der Linien im Paneel (Pixel, ohne Schatten), von unten nach oben.
    pub(super) fn line_ys(&self, lines: &[LevelLine], l: &LevelsLayout) -> Vec<f32> {
        let s = self.scale;
        let mut ys = vec![l.anchor; lines.len()];
        let a = anchor_index(lines);
        for i in a + 1..lines.len() {
            ys[i] = ys[i - 1] - self.level_gap(lines[i].z - lines[i - 1].z, l.ppm) * s;
        }
        for i in (0..a.min(lines.len())).rev() {
            ys[i] = ys[i + 1] + self.level_gap(lines[i + 1].z - lines[i].z, l.ppm) * s;
        }
        ys
    }

    /// Spalten (Pixel, ohne Schatten): Innenkante links, Kette der lichten
    /// Höhe, Kette der Geschosshöhen.
    pub(super) fn level_columns(&self) -> (f32, f32, f32) {
        let (s, z) = (self.scale, &self.size);
        let x0 = z.panel_pad * s;
        let xo = (self.width(Panel::Levels) - z.panel_pad - CHAIN_OUTER) * s;
        (x0, xo - CHAIN_GAP * s, xo)
    }

    /// Text einer Zahl im Paneel aus dem Stand der Geschosse.
    fn level_text(&self, f: Field) -> String {
        let b = &self.levels.bands;
        let band = |id: StoreyId| b.iter().find(|b| b.id == id);
        match f {
            Field::LevelBottom => b
                .iter()
                .find(|b| b.foundation)
                .map_or(String::new(), |b| kote_text(b.bottom)),
            Field::LevelTop(id) => band(id).map_or(String::new(), |b| kote_text(b.top)),
            Field::StoreyHeight(id) => band(id).map_or(String::new(), |b| m_text(b.top - b.bottom)),
            Field::ClearHeight(id) => self
                .levels
                .clear
                .iter()
                .find(|c| c.0 == id)
                .map_or(String::new(), |c| m_text(c.1)),
            Field::Terrain => self.levels.terrain.map_or(String::new(), kote_text),
            _ => String::new(),
        }
    }

    /// Fläche einer Maßzahl oder Kote: rechtsbündig an `right`, Grundlinie
    /// `base`. In Eingabe ein Zahlenfeld an derselben Stelle.
    fn level_text_rect(&self, f: Field, right: f32, base: f32) -> Rect {
        let s = self.scale;
        let px = self.level_px(f);
        let font = self.fonts.regular.as_ref();
        let text = self.level_text(f);
        let w = font.map_or(0.0, |ft| ft.width(&text, px));
        let cap = font.map_or(px * 0.7, |ft| ft.cap_height(px));
        let r = Rect::new(
            right - w - 2.0 * s,
            base - cap - 3.0 * s,
            w + 4.0 * s,
            cap + 6.0 * s,
        );
        if self.edit.as_ref().is_none_or(|e| e.field != f) {
            return r;
        }
        let (fw, fh) = (FIELD_W * s, self.size.field_height * s);
        let x = (r.x + r.w + 2.0 * s - fw).max(self.size.panel_pad * s * 0.5);
        Rect::new(x, r.y + r.h * 0.5 - fh * 0.5, fw, fh)
    }

    /// Schriftgröße: Koten wie Namen, Maßzahlen etwas kleiner.
    fn level_px(&self, f: Field) -> f32 {
        let z = &self.size;
        if f.is_kote() {
            z.font_small * self.scale
        } else {
            z.font_detail * self.scale
        }
    }

    /// Grundlinie eines Textes, der mittig zwischen `a` und `b` steht.
    fn mid_base(&self, a: f32, b: f32, px: f32) -> f32 {
        let cap = self
            .fonts
            .regular
            .as_ref()
            .map_or(px * 0.7, |f| f.cap_height(px));
        (a + b) * 0.5 + cap * 0.5
    }

    /// Klickbare Zahlen des Paneels „Geschosse“ in Paneelkoordinaten.
    pub(super) fn level_buttons(&self) -> Vec<(Id, Rect, &'static str)> {
        let s = self.scale;
        let l = self.levels_layout();
        let lines = level_lines(&self.levels);
        let (x0, xi, xo) = self.level_columns();
        let bands = &self.levels.bands;
        let mut out = Vec::new();
        let mut names = Vec::new();
        if let Some(r) = self.flat_roof_rect() {
            names.push((Id::FlatRoof, r, ""));
        }
        names.extend(self.line_vis_rects().into_iter().map(|(id, r)| (id, r, "")));
        let mut push = |f: Field, right: f32, base: f32| {
            out.push((Id::Field(f), self.level_text_rect(f, right, base), ""));
        };
        // Fuß: Kote OK Gelände (Gelände Thema 1)
        if self.levels.terrain.is_some() && l.fuss > 0.0 {
            let base = self.footer_top(&l) + self.size.level_row_base * s;
            push(Field::Terrain, xi - 6.0 * s, base);
        }

        let ys = self.line_ys(&lines, &l);
        let clamp = |y: f32| y.clamp((self.size.panel_pad + self.level_head()) * s, l.height);
        for (line, &y) in lines.iter().zip(&ys) {
            if let Some(f) = line.field {
                push(f, xi - 6.0 * s, clamp(y) - 4.0 * s);
            }
        }
        // Namen der Geschosse an ihrer Unterkante (auch das Fundament, E18)
        let nx = x0 + (self.size.level_handle + self.size.level_label_gap) * s;
        for (b, &y) in bands.iter().zip(&ys) {
            if let Some(r) = self.storey_name_rect(b, nx, clamp(y) - 4.0 * s) {
                names.push((Id::Storey(b.id), r, ""));
            }
        }
        for (i, b) in bands.iter().enumerate() {
            let f = Field::StoreyHeight(b.id);
            let base = self.mid_base(clamp(ys[i]), clamp(ys[i + 1]), self.level_px(f));
            push(f, xo - 5.0 * s, base);
        }
        for (y0, yc, id) in self.clear_spans(&lines, &ys) {
            let f = Field::ClearHeight(id);
            push(
                f,
                xi - 5.0 * s,
                self.mid_base(clamp(y0), clamp(yc), self.level_px(f)),
            );
        }
        out.extend(names);
        out
    }

    /// Klickfläche eines Geschossnamens (links `x`, Grundlinie `base`); auch
    /// das Fundament ist anklickbar (E18).
    fn storey_name_rect(&self, b: &Band, x: f32, base: f32) -> Option<Rect> {
        let s = self.scale;
        let px = self.size.font_small * s;
        let font = if b.active {
            self.fonts.bold.as_ref().or(self.fonts.regular.as_ref())
        } else {
            self.fonts.regular.as_ref()
        };
        let w = font.map_or(px * b.name.len() as f32 * 0.6, |f| f.width(&b.name, px));
        let cap = font.map_or(px * 0.7, |f| f.cap_height(px));
        Some(Rect::new(
            x - 2.0 * s,
            base - cap - 3.0 * s,
            w + 4.0 * s,
            cap + 6.0 * s,
        ))
    }

    /// Mauszeiger für den Stand der Oberfläche: Ebene ziehen senkrecht,
    /// Maßzahlen, Koten und Geschossnamen als klickbar, Felder als Eingabe.
    pub fn cursor(&self) -> Cursor {
        if self.level_drag.is_some() {
            return Cursor::SizeNS;
        }
        match self.hover {
            Some(Id::Grip(_)) => Cursor::SizeNS,
            Some(Id::Field(f))
                if f.is_level() && self.edit.as_ref().is_none_or(|e| e.field != f) =>
            {
                Cursor::Hand
            }
            Some(Id::Field(_)) => Cursor::IBeam,
            Some(Id::Storey(_) | Id::FlatRoof | Id::LineVis(_)) => Cursor::Hand,
            _ => Cursor::Arrow,
        }
    }

    /// Ketten der lichten Höhen: je Geschoss Unterkante des Bandes und
    /// Unterkante der Decke.
    fn clear_spans(&self, lines: &[LevelLine], ys: &[f32]) -> Vec<(f32, f32, StoreyId)> {
        self.levels
            .clear
            .iter()
            .filter_map(|&(id, clear)| {
                let i = self.levels.bands.iter().position(|b| b.id == id)?;
                let y0 = *ys.get(i)?;
                // Decke unter der Oberkante: in der gespreizten Darstellung anteilig
                let (z0, z1, y1) = (lines[i].z, lines.get(i + 1)?.z, *ys.get(i + 1)?);
                let t = if z1 > z0 {
                    (clear / (z1 - z0)) as f32
                } else {
                    1.0
                };
                Some((y0, y0 + (y1 - y0) * t.clamp(0.0, 1.0), id))
            })
            .collect()
    }

    /// Griff unter der Maus (Paneelkoordinaten), nur im Diagramm.
    pub(super) fn grip_at(&self, x: f64, y: f64) -> Option<Grip> {
        let l = self.levels_layout();
        let s = self.scale;
        let (x0, _, xo) = self.level_columns();
        if (x as f32) < x0 - 4.0 * s || (x as f32) > xo + 4.0 * s {
            return None;
        }
        let lines = level_lines(&self.levels);
        let ys = self.line_ys(&lines, &l);
        let hit = self.size.level_hit * s;
        let terrain = self
            .terrain_y(&lines, &ys, &l)
            .map(|ty| (Grip::Terrain, (ty - y as f32).abs()));
        let sub = self.sub_lines(&lines, &ys);
        lines
            .iter()
            .zip(ys.iter().copied())
            .filter_map(|(line, ly)| Some((line.grip?, (ly - y as f32).abs())))
            .chain(terrain)
            .chain(sub.iter().map(|u| (u.grip, (u.y - y as f32).abs())))
            .filter(|(_, d)| *d <= hit)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(g, _)| g)
    }

    /// Ziehen einer Ebene: jede Bewegung meldet die gefangene Höhe (1 cm,
    /// mit Umschalt 5 cm), Loslassen beendet.
    pub(super) fn handle_level_drag(&mut self, e: &Event, d: LevelDrag) -> UiOut {
        let mut out = UiOut {
            consumed: true,
            ..UiOut::default()
        };
        match *e {
            Event::MouseMove { y, mods, .. } => {
                let px_per_mm = (d.layout.ppm * self.scale) as f64 / 1000.0;
                let z = d.z0 + (d.y0 - y) / px_per_mm;
                let step = if mods.shift { 50.0 } else { 10.0 };
                out.level = Some(LevelEvent::Move(d.grip, (z / step).round() * step));
                out.changed.push(Id::Grip(d.grip));
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                self.level_drag = None;
                out.level = Some(LevelEvent::End);
                // Erst jetzt passt sich das Diagramm an
                out.relayout = true;
            }
            _ => {}
        }
        out
    }

    /// Zeichnet das ganze Paneel „Geschosse“ neu und liefert es als
    /// Ausschnitt; `None`, wenn sich seine Größe geändert hat.
    pub(super) fn repaint_levels(&mut self, t: &Theme) -> Option<Patch> {
        let i = panel_index(Panel::Levels);
        let old = self.images[i].as_ref()?;
        if old.scale != self.scale {
            return None;
        }
        let (w, h) = (old.cur.width, old.cur.height);
        let old = self.images[i].take()?;
        let base = self.paint_base(t, Panel::Levels, 0, self.top, Some(old.base));
        if (base.width, base.height) != (w, h) {
            // Das ganze Paneel wird ohnehin neu gezeichnet
            return None;
        }
        let mut cur = old.cur;
        cur.copy_from(&base);
        for (id, b, label) in self.buttons(Panel::Levels) {
            self.paint_button(t, &mut cur, id, b, label);
        }
        let (x, y, w, h, px) = cur.region_premul_rgba8(0, 0, w, h);
        self.images[i] = Some(PanelImage {
            scale: self.scale,
            base,
            cur,
        });
        Some(Patch {
            panel: Panel::Levels,
            x,
            y,
            w,
            h,
            px,
        })
    }

    /// Maßzahl oder Kote (nicht in Eingabe), `b` im Bild.
    pub(super) fn paint_dim_text(&self, t: &Theme, c: &mut Canvas, f: Field, b: Rect) {
        let s = self.scale;
        let color = if self.hover == Some(Id::Field(f)) {
            t.ui.dim_text_hover
        } else {
            t.ui.dim_text
        };
        let text = self.level_text(f);
        widgets::text(
            c,
            self.fonts.regular.as_ref(),
            &text,
            self.level_px(f),
            b.x + 2.0 * s,
            b.y + b.h - 3.0 * s,
            color,
        );
    }

    /// Linien, Griffe, Ketten und feste Texte (ohne die klickbaren Zahlen).
    pub(super) fn paint_levels(&self, t: &Theme, c: &mut Canvas, panel_h: f32) {
        let (u, z, s) = (&t.ui, &self.size, self.scale);
        let m = (z.panel_shadow * s).round();
        let l = self.levels_layout();
        let lines = level_lines(&self.levels);
        let (x0, xi, xo) = self.level_columns();
        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let px = z.font_small * s;
        let px_d = z.font_detail * s;
        let w1 = s.round().max(1.0);
        let right_text = |c: &mut Canvas,
                          f: Option<&sk_paint::font::Font>,
                          txt: &str,
                          px: f32,
                          right: f32,
                          base: f32,
                          col: Rgba| {
            let w = f.map_or(0.0, |f| f.width(txt, px));
            widgets::text(c, f, txt, px, right - w + m, base + m, col);
        };
        let ys = self.line_ys(&lines, &l);
        let (lo, hi) = (
            (z.panel_pad + self.level_head()) * s,
            panel_h - z.panel_pad * s * 0.5,
        );
        let clamp = |y: f32| y.clamp(lo, hi);
        // Kette der Geschosshöhen
        if let (Some(&a), Some(&b)) = (ys.first(), ys.last()) {
            c.fill_rect(
                xo + m - w1 * 0.5,
                clamp(b) + m,
                w1,
                clamp(a) - clamp(b),
                u.dim_line,
            );
        }
        let tick = |c: &mut Canvas, x: f32, y: f32| {
            let mut p = Path::new();
            let d = z.dim_tick * s;
            p.segment(
                (x - d + m, y + d + m),
                (x + d + m, y - d + m),
                z.dim_line * s,
            );
            c.fill(&p, u.dim_line);
        };
        let drag = self.level_drag.map(|d| d.grip);
        // OK Gelände (Gelände Thema 1): Linie mit Erdschraffur darunter und
        // Griff, unter den Ebenen (bei Versatz 0 liegt sie auf ±0,00); die
        // Kote steht im Fuß
        if let Some(ty) = self.terrain_y(&lines, &ys, &l) {
            let y = clamp(ty);
            let dragged = drag == Some(Grip::Terrain);
            let col = if dragged {
                u.level_handle_drag
            } else {
                u.text_dim
            };
            let x1 = xi - 12.0 * s;
            c.fill_rect(x0 + m, (y + m - w1 * 0.5).round(), x1 - x0, w1, col);
            let step = 7.0 * s;
            let d = 5.0 * s;
            let mut x = x0 + z.level_handle * s + 4.0 * s;
            while x + d <= x1 {
                let mut p = Path::new();
                p.segment((x + m, y + d + m), (x + d + m, y + m), w1);
                c.fill(&p, col);
                x += step;
            }
            let hc = if dragged {
                u.level_handle_drag
            } else if self.hover == Some(Id::Grip(Grip::Terrain)) {
                u.level_handle_hover
            } else {
                u.text_dim
            };
            let hd = z.level_handle * s;
            let mut p = Path::new();
            p.rounded_rect(x0 + m, y + m - hd * 0.5, hd, hd * 0.5 + 1.0, 1.0);
            c.fill(&p, hc);
        }
        self.paint_sub_lines(t, c, &lines, &ys, (lo, hi));
        for (line, &y) in lines.iter().zip(&ys) {
            let y = clamp(y);
            let dragged = line.grip.is_some() && line.grip == drag;
            let (col, th) = if dragged {
                (u.level_handle_drag, 2.0 * w1)
            } else if line.active {
                (u.level_line_active, 2.0 * w1)
            } else {
                (u.level_line, w1)
            };
            let yl = (y + m - th * 0.5).round();
            c.fill_rect(x0 + m, yl, xo + 4.0 * s - x0, th, col);
            tick(c, xo, y);
            let name_col = if line.active {
                u.level_line_active
            } else {
                u.text
            };
            let nx = x0 + (z.level_handle + z.level_label_gap) * s;
            // Ein langer Name („OK Decke OG“) rückt eine Zeile über die Kote,
            // wenn er sie berühren würde; beide bleiben über der Linie
            let width =
                |f: Option<&sk_paint::font::Font>, t: &str| f.map_or(0.0, |f| f.width(t, px));
            let kote_x = xi - 4.0 * s - width(regular, &kote_text(line.z));
            // Aktiv fett, außer der fette Name stieße an die Kote und der
            // normale nicht („Fundament −0,80“, E18): dann bleibt er auf
            // seiner Zeile und ist nur an der Farbe zu erkennen
            let fits = |f| nx + width(f, &line.name) + 6.0 * s <= kote_x;
            let font = if line.active && (fits(bold.or(regular)) || !fits(regular)) {
                bold.or(regular)
            } else {
                regular
            };
            let base = if nx + width(font, &line.name) + 6.0 * s > kote_x {
                y - 4.0 * s - regular.map_or(px * 0.7, |f| f.cap_height(px)) - 5.0 * s
            } else {
                y - 4.0 * s
            };
            widgets::text(c, font, &line.name, px, nx + m, base + m, name_col);
            if line.field.is_none() {
                right_text(
                    c,
                    regular,
                    &kote_text(line.z),
                    px,
                    xi - 4.0 * s,
                    y - 4.0 * s,
                    u.dim_text,
                );
            }
            if let Some(g) = line.grip {
                let col = if dragged {
                    u.level_handle_drag
                } else if self.hover == Some(Id::Grip(g)) {
                    u.level_handle_hover
                } else {
                    u.level_handle
                };
                let d = z.level_handle * s;
                let mut p = Path::new();
                p.rounded_rect(x0 + m, y + m - d * 0.5, d, d, d * 0.5);
                c.fill(&p, col);
            }
        }
        // Ketten der lichten Höhen mit Unterkante der Decke
        for (y0, yc, id) in self.clear_spans(&lines, &ys) {
            let (y0, yc) = (clamp(y0), clamp(yc));
            c.fill_rect(xi + m - w1 * 0.5, yc + m, w1, y0 - yc, u.dim_line);
            c.fill_rect(
                xi - 10.0 * s + m,
                (yc + m - w1 * 0.5).round(),
                20.0 * s,
                w1,
                u.dim_line,
            );
            tick(c, xi, y0);
            tick(c, xi, yc);
            let f = Field::ClearHeight(id);
            let r = self.level_text_rect(f, xi - 5.0 * s, self.mid_base(y0, yc, px_d));
            let base = self.mid_base(y0, yc, px_d);
            right_text(c, regular, "lichte", px_d, r.x - 2.0 * s, base, u.text_dim);
        }
        self.paint_level_footer(t, c, &l);
        self.paint_level_error(t, c);
    }

    /// Fuß des Paneels: „OK Gelände“ (die Kote ist ein Feld) und die
    /// Einbindetiefe, rot unter 0,80 m.
    fn paint_level_footer(&self, t: &Theme, c: &mut Canvas, l: &LevelsLayout) {
        let (Some(_), u, z, s) = (self.levels.terrain, &t.ui, &self.size, self.scale) else {
            return;
        };
        if l.fuss == 0.0 {
            return;
        }
        let m = (z.panel_shadow * s).round();
        let (x0, _, _) = self.level_columns();
        let regular = self.fonts.regular.as_ref();
        let top = self.footer_top(l);
        let base = top + z.level_row_base * s;
        let px = z.font_small * s;
        widgets::text(c, regular, "OK Gelände", px, x0 + m, base + m, u.text);
        if let Some(e) = self.levels.embedment {
            let ok = e >= sk_model::FROST_DEPTH - 0.5;
            let txt = if ok {
                format!("Einbindetiefe {} m · frostfrei", m_text(e))
            } else {
                format!("Einbindetiefe {} m · unter 0,80 m", m_text(e))
            };
            let col = if ok { u.text_dim } else { u.field_invalid };
            let px_d = z.font_detail * s;
            let b2 = base + LEVEL_FOOTER_ROW * s;
            widgets::text(c, regular, &txt, px_d, x0 + m, b2 + m, col);
        }
    }

    /// Grund einer ungültigen Eingabe unter dem Feld.
    fn paint_level_error(&self, t: &Theme, c: &mut Canvas) {
        let Some((f, why)) = self
            .edit
            .as_ref()
            .filter(|e| e.field.is_level())
            .and_then(|e| Some((e.field, e.error.clone()?)))
        else {
            return;
        };
        let Some(r) = self.field_rect(f) else {
            return;
        };
        let (s, z) = (self.scale, &self.size);
        let m = (z.panel_shadow * s).round();
        let px = z.font_detail * s;
        let font = self.fonts.regular.as_ref();
        let w = font.map_or(0.0, |f| f.width(&why, px)) + 8.0 * s;
        let x = (r.x + r.w - w).max(z.panel_pad * s * 0.5);
        let y = r.y + r.h + 2.0 * s;
        let mut p = Path::new();
        p.rounded_rect(x + m, y + m, w, px + 6.0 * s, 3.0 * s);
        c.fill(&p, t.ui.bg);
        widgets::text(
            c,
            font,
            &why,
            px,
            x + 4.0 * s + m,
            y + px + m,
            t.ui.field_invalid,
        );
    }
}
