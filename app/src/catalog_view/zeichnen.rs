//! Zeichnen des Bauteilkatalogs, aus `catalog_view.rs` ausgelagert.

use super::*;

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    Rgba(
        c.0,
        c.1,
        c.2,
        (c.3 as f32 * a.clamp(0.0, 1.0)).round() as u8,
    )
}

/// Marke an der Kachel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mark {
    Standard,
    OnlyProject,
    CompanyNewer,
    ProjectNewer,
    Same,
    NotInProject,
}

impl Mark {
    fn text(self) -> &'static str {
        match self {
            Mark::Standard => "Standard",
            Mark::OnlyProject => "nur im Projekt",
            Mark::CompanyNewer => "Firma neuer",
            Mark::ProjectNewer => "Projekt neuer",
            Mark::Same => "wie im Projekt",
            Mark::NotInProject => "nicht im Projekt",
        }
    }
}

fn pill_font(fonts: &Fonts, mark: Mark) -> Option<&sk_paint::font::Font> {
    let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
    if mark == Mark::Standard {
        bold
    } else {
        fonts.regular.as_ref()
    }
}

fn pill_w(fonts: &Fonts, mark: Mark, s: f32, t: &Theme) -> f32 {
    let px = t.size.font_detail * s;
    let tw = pill_font(fonts, mark).map_or(40.0, |f| f.width(mark.text(), px));
    (tw + 14.0 * s).round()
}

fn paint_pill(c: &mut Canvas, fonts: &Fonts, right: f32, y: f32, mark: Mark, s: f32, t: &Theme) {
    let u = &t.ui;
    let font = pill_font(fonts, mark);
    let px = t.size.font_detail * s;
    let (w, h) = (pill_w(fonts, mark, s, t), (16.0 * s).round());
    let r = Rect::new((right - w).round(), y.round(), w, h);
    let rad = h * 0.5;
    let (fill, edge, text) = match mark {
        Mark::Standard => (Some(u.accent), u.accent, u.on_accent),
        Mark::OnlyProject | Mark::CompanyNewer | Mark::ProjectNewer => (None, u.accent, u.accent),
        Mark::Same => (None, t.ui.text_same, t.ui.text_same),
        Mark::NotInProject => (None, u.text_dim, u.text_dim),
    };
    match fill {
        Some(f) => rounded(c, r, rad, f),
        None => outline(c, r, rad, s.round().max(1.0), edge),
    }
    label(
        c,
        font,
        mark.text(),
        px,
        r.x + 7.0 * s,
        r.y + 12.0 * s,
        text,
    );
}

/// Marke mit Deckkraft `alpha` (Überblendung, K3b).
#[allow(clippy::too_many_arguments)]
fn faded_pill(
    c: &mut Canvas,
    fonts: &Fonts,
    right: f32,
    y: f32,
    mark: Mark,
    s: f32,
    t: &Theme,
    alpha: f32,
) {
    let (w, h) = (pill_w(fonts, mark, s, t), (16.0 * s).round());
    let (x0, y0) = ((right - w).round() - 2.0, y.round() - 2.0);
    let mut img = Canvas::new(w as usize + 5, h as usize + 5);
    img.set_origin(x0, y0);
    paint_pill(&mut img, fonts, right, y, mark, s, t);
    c.blit_scaled(&img, x0, y0, 1.0, alpha);
}

impl Catalog {
    pub(super) fn mark_of(&self, it: Item) -> Option<Mark> {
        let lib = self.company_lib();
        match it {
            Item::Type(id) => {
                if self.work.is_default_type(id) {
                    return Some(Mark::Standard);
                }
                let g = self.work.layer_set(id)?.guid;
                (lib.is_some_and(|l| l.type_by_guid(g).is_none())).then_some(Mark::OnlyProject)
            }
            Item::Draft => Some(Mark::OnlyProject),
            Item::Ext(_) => None,
            Item::Company(g) => {
                let lib = lib?;
                let ct = lib.types.get(lib.type_by_guid(g)?)?;
                let state = compare(&self.work, lib)
                    .into_iter()
                    .find(|x| x.0 == g)
                    .map(|x| x.1);
                Some(match state {
                    Some(TypeState::Same) => Mark::Same,
                    Some(TypeState::Differs) => {
                        let pt = self
                            .work
                            .type_by_guid(g)
                            .and_then(|id| self.work.layer_set(id));
                        if pt.is_some_and(|p| p.changed > ct.changed) {
                            Mark::ProjectNewer
                        } else {
                            Mark::CompanyNewer
                        }
                    }
                    _ => {
                        if lib.default_type(ct.category) == lib.type_by_guid(g) {
                            Mark::Standard
                        } else {
                            Mark::NotInProject
                        }
                    }
                })
            }
        }
    }

    /// Name, Kurzzeichen · Dicke, Verwendung und Aussehen einer Kachel.
    fn tile_info(&self, it: Item, t: &Theme) -> Option<(String, String, usize, TypeLook)> {
        match it {
            Item::Type(id) => {
                // Der gewählte Typ zeigt den Entwurf (auch ungültig)
                let ts = if Some(id) == self.sel && !self.draft_new {
                    &self.draft
                } else {
                    self.work.layer_set(id)?
                };
                Some((
                    ts.name.clone(),
                    format!("{} · {}", ts.code, cm_text(ts.thickness())),
                    self.work.type_users(id).len(),
                    type_look(&self.work, t, ts),
                ))
            }
            Item::Draft => Some((
                self.draft.name.clone(),
                format!("{} · {}", self.draft.code, cm_text(self.draft.thickness())),
                0,
                type_look(&self.work, t, &self.draft),
            )),
            Item::Ext(_) => None,
            Item::Company(g) => {
                let lib = self.company_lib()?;
                let ct = lib.types.get(lib.type_by_guid(g)?)?;
                Some((
                    ct.name.clone(),
                    format!("{} · {}", ct.code, cm_text(ct.thickness())),
                    0,
                    lib_look(lib, t, ct),
                ))
            }
        }
    }

    pub(super) fn is_selected(&self, it: Item) -> bool {
        match it {
            Item::Type(id) => self.tab == Tab::Project && !self.draft_new && self.sel == Some(id),
            Item::Draft => self.draft_new,
            Item::Company(g) => self.tab == Tab::Company && self.csel == Some(g),
            Item::Ext(i) => self.tab == Tab::Ext && self.ext_sel == Some(i),
        }
    }

    #[cfg(test)]
    pub fn paint(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> (Canvas, i32, i32) {
        let mut c = Canvas::new(0, 0);
        let (x, y) = self.paint_onto(&mut c, t, fonts, w);
        (c, x, y)
    }

    /// Wie [`Catalog::paint`] auf eine vorhandene Leinwand, die ihren
    /// Speicher behält ([`Canvas::reuse`]).
    fn paint_onto(&mut self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) -> (i32, i32) {
        let outer = self.paint_now.is_none();
        if outer {
            self.paint_now = Some(self.clock());
        }
        let f = self.frame(t, w);
        let m = (t.size.panel_shadow * w.scale).round();
        c.reuse((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        // Zeichnen in Fensterkoordinaten
        c.set_origin(f.x - m, f.y - m);
        self.paint_into(c, t, fonts, w);
        let (x, y) = self.origin(t, w);
        if outer {
            self.paint_now = None;
        }
        (x, y)
    }

    /// Die Bytes eines hochgeladenen [`Frame::Full`] zum Wiederverwenden.
    pub fn give_back(&mut self, px: Vec<u8>) {
        self.bytes = px;
    }

    /// Nächstes Bild für die App (U7): Nach Hervorhebungen und Übergängen
    /// nur die betroffenen Bereiche, sonst das ganze Fenster.
    pub fn paint_frame(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> Frame {
        self.paint_now = Some(self.clock());
        let frame = self.paint_frame_now(t, fonts, w);
        self.paint_now = None;
        frame
    }

    fn paint_frame_now(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> Frame {
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let (cw, ch) = ((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let (ox, oy) = (f.x - m, f.y - m);
        let areas = std::mem::take(&mut self.damage);
        let mut rects = Vec::new();
        // Theme, Skalierung oder Lage gewechselt: ganz (Darstellung n9 §2.3)
        let key = (cw, ch, s.to_bits(), t.rev, ox as i32, oy as i32);
        let moved = self.img_key.replace(key) != Some(key);
        let mut full = areas.is_empty() || self.img.is_none() || moved;
        for a in &areas {
            match self.area_rects(*a, t, w, fonts) {
                Some(v) => rects.extend(v),
                None => full = true,
            }
        }
        // In Bildpunkte des Fensters, mit Rand für Umrisse und Glättung
        let pad = (4.0 * s).ceil();
        // Streifen über die ganze Breite: bitgenau wie das ganze Bild
        let parts = pixel_strips(&rects, pad, (ox, oy), (cw, ch));
        let area: usize = parts.iter().map(|p| (p.2 - p.0) * (p.3 - p.1)).sum();
        if full || area * 2 > cw * ch {
            // Bild und Bytes des letzten ganzen Bildes weiterverwenden
            let mut c = self.img.take().unwrap_or_else(|| Canvas::new(0, 0));
            let (x, y) = self.paint_onto(&mut c, t, fonts, w);
            let mut px = std::mem::take(&mut self.bytes);
            c.premul_rgba8_into(&mut px);
            let (cw, ch) = (c.width as u32, c.height as u32);
            self.img = Some(c);
            return Frame::Full {
                x,
                y,
                w: cw,
                h: ch,
                px,
            };
        }
        let mut out = Vec::with_capacity(parts.len());
        for (x0, y0, x1, y1) in parts {
            let mut sub = std::mem::replace(&mut self.scratch, Canvas::new(0, 0));
            sub.reuse(x1 - x0, y1 - y0);
            sub.set_origin(ox + x0 as f32, oy + y0 as f32);
            self.paint_into(&mut sub, t, fonts, w);
            if let Some(img) = self.img.as_mut() {
                img.put(&sub, x0, y0);
            }
            let px = sub.to_premul_rgba8();
            out.push((
                x0 as i32,
                y0 as i32,
                sub.width as u32,
                sub.height as u32,
                px,
            ));
            self.scratch = sub;
        }
        Frame::Parts(out)
    }

    /// Fensterbereiche einer Änderung; `None`: unbekannt, ganz malen.
    fn area_rects(&self, a: Area, t: &Theme, w: &Win, fonts: &Fonts) -> Option<Vec<Rect>> {
        let s = w.scale;
        Some(match a {
            Area::Full => return None,
            Area::Section => vec![self.section_rect(t, w)],
            Area::Flash(Flash::Name) => vec![self.field_rect(t, w, FieldId::Name)?],
            Area::Flash(f) => {
                let i = if f == Flash::Thick { 0.0 } else { 1.0 };
                vec![self.r(t, w, RIGHT_X, 124.0 + i * 30.0, 300.0, 26.0)]
            }
            Area::Foot => {
                let fr = self.frame(t, w);
                vec![Rect::new(fr.x, fr.y + fr.h - FOOT * s, fr.w, FOOT * s)]
            }
            Area::Fly => {
                let now = self.fly_rect(t, w).map(|f| f.1);
                self.fly_drawn.into_iter().chain(now).collect()
            }
            Area::Pulse(tab) => vec![self.tab_rect(t, w, tab)],
            Area::Tile(it) => {
                let body = self.list_body(t, w);
                let (tiles, ..) = self.list_layout(t, w);
                let r = tiles.into_iter().find(|(x, _)| *x == it)?.1;
                vec![intersect(r, body)?]
            }
            Area::Hover(None) => Vec::new(),
            Area::Hover(Some(h)) => {
                // Schichten heben ihre Lage im Schnittbild mit hervor
                let layer = |i: usize| {
                    let y = ROWS_Y + i as f32 * ROW_H - 4.0;
                    let row = self.r(t, w, CONTENT_X - 4.0, y, 562.0, ROW_H);
                    vec![row, self.section_rect(t, w)]
                };
                match h {
                    Target::Close => vec![self.close_rect(t, w)],
                    Target::Tab(tab) => vec![self.tab_rect(t, w, tab)],
                    Target::PathLink => vec![self.path_link(t, w, fonts)?],
                    Target::Tile(it) => {
                        let body = self.list_body(t, w);
                        let (tiles, ..) = self.list_layout(t, w);
                        let r = tiles.into_iter().find(|(x, _)| *x == it)?.1;
                        vec![intersect(r, body)?]
                    }
                    Target::Btn(b) => {
                        let mut all = self.foot_buttons(t, w).to_vec();
                        all.extend(self.list_buttons(t, w));
                        if let Some(r) = self.ext_button(t, w) {
                            all.push((Btn::Einfuegen, r, ""));
                        }
                        vec![all.into_iter().find(|(x, ..)| *x == b)?.1]
                    }
                    // Eine Schichtzeile hebt die ganze Zeile hervor
                    Target::Field(FieldId::Thick(i))
                    | Target::Combo(ComboId::Material(i) | ComboId::Function(i))
                    | Target::Grip(i)
                    | Target::Remove(i)
                    | Target::Section(i) => layer(i),
                    Target::Field(fi) => vec![self.field_rect(t, w, fi)?],
                    Target::Combo(id) => vec![self.combo_rect(t, w, id)],
                    Target::AddLayer => vec![self.add_layer_rect(t, w)],
                    Target::Standard => {
                        let st = self.standard_rect(t, w);
                        vec![Rect::new(st.x, st.y, 260.0 * s, st.h)]
                    }
                    Target::Bearing(_) => vec![self.bearing_box(t, w)],
                    // Auswahllisten und Karten liegen in eigenem Bild
                    Target::Choice(_)
                    | Target::NewCat(_)
                    | Target::NewFrom(_)
                    | Target::MatCat(_)
                    | Target::Card => Vec::new(),
                }
            }
        })
    }

    /// Malt das Fenster in `c` (Fensterkoordinaten über den Ursprung von
    /// `c`); was außerhalb von `c` liegt, fällt weg.
    fn paint_into(&mut self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let f = self.frame(t, w);
        let s = w.scale;
        let u = &t.ui;
        widgets::panel(c, f, s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let line = s.round().max(1.0);
        // Kopf
        label(
            c,
            bold,
            "Bauteilkatalog",
            t.size.font_title * s,
            f.x + 20.0 * s,
            f.y + 40.0 * s,
            u.text,
        );
        let seg = Rect::new(
            self.tab_rect(t, w, Tab::Project).x - 2.0 * s,
            self.tab_rect(t, w, Tab::Project).y - 2.0 * s,
            350.0 * s,
            32.0 * s,
        );
        rounded(c, seg, 7.0 * s, u.field);
        for tab in [Tab::Project, Tab::Company, Tab::Ext] {
            let r = self.tab_rect(t, w, tab);
            let on = self.tab == tab;
            let hov = self.hover == Some(Target::Tab(tab));
            if on {
                rounded(c, r, 6.0 * s, u.pressed);
                outline(c, r, 6.0 * s, line, u.accent);
            } else if hov {
                rounded(c, r, 6.0 * s, u.hover);
            }
            // Nach Übernehmen oder Speichern pulst der Zielreiter einmal
            let pulse = self
                .pulse
                .filter(|p| p.0 == tab)
                .and_then(|(_, at)| self.progress(at, t.size.flash_ms, t));
            if let Some(p) = pulse {
                outline(c, r, 6.0 * s, 2.0 * line, with_alpha(u.accent, 1.0 - p));
            }
            let text = match tab {
                Tab::Project => "Projekt",
                Tab::Company => "Firma",
                Tab::Ext => "Erweiterungen",
            };
            let font = if on { bold } else { regular };
            let px = t.size.font * s;
            let tw = font.map_or(0.0, |ft| ft.width(text, px));
            label(
                c,
                font,
                text,
                px,
                r.x + (r.w - tw) * 0.5,
                r.y + 19.0 * s,
                if on { u.text } else { u.text_dim },
            );
        }
        if let (Tab::Company, Some((_, path))) = (self.tab, &self.company) {
            let base = self.r(t, w, PATH_X, 18.0, 0.0, 28.0);
            let px = t.size.font_small * s;
            let p = widgets::ellipsize(regular, path, px, self.path_room(t, w));
            label(c, regular, &p, px, base.x, base.y + 19.0 * s, u.text_dim);
            if let Some(l) = self.path_link(t, w, fonts) {
                let col = if self.hover == Some(Target::PathLink) {
                    u.accent_hover
                } else {
                    u.accent
                };
                label(
                    c,
                    regular,
                    "ändern …",
                    px,
                    l.x + 2.0 * s,
                    l.y + 19.0 * s,
                    col,
                );
            }
        }
        let cr = self.close_rect(t, w);
        if self.hover == Some(Target::Close) {
            rounded(c, cr, 6.0 * s, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        c.fill_rect(f.x, f.y + HEAD * s, f.w, line, u.border);
        c.fill_rect(f.x, f.y + f.h - FOOT * s, f.w, line, u.border);
        let sep_x = f.x + (t.size.catalog_list_w + 12.0) * s;
        c.fill_rect(
            sep_x.round(),
            f.y + HEAD * s,
            line,
            f.h - (HEAD + FOOT) * s,
            u.border,
        );
        self.paint_list(c, t, fonts, w);
        if self.tab == Tab::Project {
            for (b, r, text) in self.list_buttons(t, w) {
                let disabled = b == Btn::Delete && !self.can_delete();
                widgets::button(c, fonts, r, text, self.btn_state(b, false, disabled), s, t);
            }
            self.paint_project(c, t, fonts, w);
        } else if self.tab == Tab::Ext {
            self.paint_ext(c, t, fonts, w);
        } else {
            self.paint_company(c, t, fonts, w);
        }
        // Fuß
        let no_company = self.company.is_none();
        for (b, r, text) in self.foot_buttons(t, w) {
            let disabled = b == Btn::Action
                && ((no_company && self.tab != Tab::Ext)
                    || (self.tab == Tab::Project && (self.draft_new || self.nutzer))
                    || (self.tab == Tab::Company && self.csel.is_none()));
            widgets::button(
                c,
                fonts,
                r,
                text,
                self.btn_state(b, b == Btn::Ok, disabled),
                s,
                t,
            );
        }
        let msg = self
            .edit
            .as_ref()
            .and_then(|e| e.invalid.clone())
            .or_else(|| self.message.clone());
        if let Some(msg) = msg {
            let [action, cancel, _] = self.foot_buttons(t, w);
            let x0 = action.1.x + action.1.w + 14.0 * s;
            let max = cancel.1.x - 14.0 * s - x0;
            let px = t.size.font_small * s;
            let text = widgets::ellipsize(regular, &msg, px, max);
            let blink = self
                .blink
                .map_or(0.0, |at| {
                    1.0 - at.elapsed().as_secs_f32() * 1000.0 / t.size.flash_ms.max(1.0)
                })
                .max(0.0);
            if blink > 0.0 {
                let tw = regular.map_or(0.0, |ft| ft.width(&text, px));
                let r = Rect::new(x0 - 6.0 * s, action.1.y + 3.0 * s, tw + 12.0 * s, 24.0 * s);
                rounded(c, r, 4.0 * s, with_alpha(u.field_invalid, 0.35 * blink));
            }
            let col = if self.edit.as_ref().is_some_and(|e| e.invalid.is_some())
                || self.draft_problems().first() == Some(&msg)
            {
                u.field_invalid
            } else {
                u.text_dim
            };
            label(c, regular, &text, px, x0, action.1.y + 20.0 * s, col);
        }
        self.paint_fly(c, t, fonts, w);
    }

    /// Fliegende Kopie der Kachel (Schnittbild und Name) über allem.
    fn paint_fly(&mut self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some((it, r, alpha)) = self.fly_rect(t, w) else {
            self.fly_drawn = None;
            return;
        };
        let s = w.scale;
        if self.fly_img.is_none() {
            let (tiles, ..) = self.list_layout(t, w);
            let Some(from) = tiles.into_iter().find(|(x, _)| *x == it).map(|x| x.1) else {
                return;
            };
            let from = Rect::new(
                from.x.round(),
                from.y.round(),
                from.w.round(),
                from.h.round(),
            );
            let mut img = Canvas::new(from.w as usize, from.h as usize);
            img.set_origin(from.x, from.y);
            rounded(&mut img, from, 6.0 * s, t.ui.hover);
            self.paint_tile(&mut img, it, from, t, fonts, w, false, false);
            self.fly_img = Some(img);
        }
        if let Some(img) = &self.fly_img {
            let k = r.w / img.width as f32;
            c.blit_scaled(img, r.x, r.y, k, alpha);
        }
        self.fly_drawn = Some(r);
    }

    pub(super) fn btn_state(&self, b: Btn, active: bool, disabled: bool) -> ButtonState {
        ButtonState {
            hover: self.hover == Some(Target::Btn(b)) && !disabled,
            pressed: self.pressed == Some(Target::Btn(b)) && self.hover == Some(Target::Btn(b)),
            active,
            disabled,
        }
    }

    fn paint_list(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let body = self.list_body(t, w);
        let (tiles, heads, _) = self.list_layout(t, w);
        // Eigene Leinwand: was über den Rand ragt, fällt weg; nur der Teil,
        // den `c` zeigt (Teilbild)
        let body = Rect::new(
            body.x.round(),
            body.y.round(),
            body.w.round(),
            body.h.round(),
        );
        let left = body.x;
        let (ox, oy) = c.origin();
        let shown = Rect::new(ox, oy, c.width as f32, c.height as f32);
        let Some(body) = intersect(body, shown) else {
            return;
        };
        let mut sub = self.list_sub.replace(Canvas::new(0, 0));
        sub.reuse(body.w as usize, body.h as usize);
        sub.set_origin(body.x, body.y);
        let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
        for (h, y) in heads {
            label(
                &mut sub,
                bold,
                &h,
                t.size.font_detail * s,
                left + 18.0 * s,
                y + 16.0 * s,
                u.text_dim,
            );
        }
        for (it, r) in tiles {
            if r.y + r.h < body.y || r.y > body.y + body.h {
                continue;
            }
            let sel = self.is_selected(it);
            let hover = self.hover == Some(Target::Tile(it));
            self.paint_tile(&mut sub, it, r, t, fonts, w, sel, hover);
        }
        c.blit(&sub, body.x as i32, body.y as i32);
        self.list_sub.replace(sub);
    }

    /// Eine Kachel der Liste in `r`: Schnittbild, Name, Angabe, Zahl der
    /// Verwendungen, Marke.
    #[allow(clippy::too_many_arguments)]
    fn paint_tile(
        &self,
        c: &mut Canvas,
        it: Item,
        r: Rect,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sel: bool,
        hover: bool,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        if let Item::Ext(i) = it {
            return self.paint_ext_tile(c, i, r, t, fonts, w, sel, hover);
        }
        let Some((name, detail, uses, look)) = self.tile_info(it, t) else {
            return;
        };
        if sel {
            rounded(c, r, 6.0 * s, u.pressed);
            c.fill_rect(
                r.x,
                r.y + 4.0 * s,
                (3.0 * s).round(),
                r.h - 8.0 * s,
                u.accent,
            );
        } else if hover {
            rounded(c, r, 6.0 * s, u.hover);
        }
        let (tw, th) = (
            (t.size.catalog_thumb_w * s).round(),
            (t.size.catalog_thumb_h * s).round(),
        );
        let ty = (r.y + (r.h - th) * 0.5).round();
        paint_thumb(c, Rect::new((r.x + 10.0 * s).round(), ty, tw, th), &look, s);
        let tx = r.x + 10.0 * s + tw + 10.0 * s;
        let mark = self.mark_of(it);
        let right = r.x + r.w - 10.0 * s;
        let px = t.size.font * s;
        let font = if sel { bold } else { regular };
        let max = right - tx - if uses > 0 { 30.0 * s } else { 0.0 };
        let name = widgets::ellipsize(font, &name, px, max);
        label(c, font, &name, px, tx, r.y + 20.0 * s, u.text);
        let pd = t.size.font_detail * s;
        let room = right - tx - mark.map_or(0.0, |mk| pill_w(fonts, mk, s, t) + 6.0 * s);
        let detail = widgets::ellipsize(regular, &detail, pd, room);
        label(c, regular, &detail, pd, tx, r.y + 38.0 * s, u.text_dim);
        if uses > 0 {
            let text = format!("{uses}×");
            let uw = regular.map_or(0.0, |ft| ft.width(&text, pd));
            label(
                c,
                regular,
                &text,
                pd,
                right - uw,
                r.y + 18.0 * s,
                u.text_dim,
            );
        }
        // Nach Übernehmen oder Speichern blendet die Marke über
        let fade = self
            .fade
            .filter(|f| f.0 == it)
            .and_then(|(_, old, at)| Some((old, self.progress(at, FADE_MS, t)?)));
        match fade {
            Some((old, k)) => {
                for (m, a) in [(old, 1.0 - k), (mark, k)] {
                    if let Some(m) = m {
                        faded_pill(c, fonts, right, r.y + 25.0 * s, m, s, t, a);
                    }
                }
            }
            None => {
                if let Some(mk) = mark {
                    paint_pill(c, fonts, right, r.y + 25.0 * s, mk, s, t);
                }
            }
        }
    }

    /// Hervorgehobene Schicht (Zeile, Feld, Schnittbild, Ziehen).
    fn hover_layer(&self) -> Option<usize> {
        match (self.hover, &self.edit, self.drag) {
            (_, _, Some(Drag::Row(i))) => Some(i),
            (Some(Target::Section(i) | Target::Grip(i) | Target::Remove(i)), ..) => Some(i),
            (Some(Target::Field(FieldId::Thick(i))), ..) => Some(i),
            (Some(Target::Combo(ComboId::Material(i) | ComboId::Function(i))), ..) => Some(i),
            (_, Some(e), _) => match e.field {
                FieldId::Thick(i) => Some(i),
                _ => None,
            },
            _ => None,
        }
    }

    /// Schnittbild des Entwurfs mit Übergang und Hervorhebung.
    fn paint_section_view(&self, c: &mut Canvas, t: &Theme, font: Option<&Font>, w: &Win) {
        let sec = self.section_rect(t, w);
        let look = type_look(&self.work, t, &self.draft);
        let th = self.anim_thick(t);
        let marks = SectionMarks {
            hover: self.hover_layer(),
            ghost: self.ghost.clone(),
        };
        let mut pats = self.patterns.borrow_mut();
        paint_section(
            c,
            font,
            sec,
            &look,
            &th,
            &marks,
            w.scale,
            t,
            Some(&mut pats),
        );
    }

    fn paint_project(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        // Kopf: Name, Kurzzeichen, Art
        for f in [FieldId::Name, FieldId::Code] {
            let Some(r) = self.field_rect(t, w, f) else {
                continue;
            };
            let v = self.field_value(f);
            let st = self.field_state(f, &v);
            widgets::text_field(c, fonts, r, &st, s, t);
            let k = self.flash_k(Flash::Name, t);
            if f == FieldId::Name && k > 0.0 && self.edit.as_ref().is_none_or(|e| e.field != f) {
                rounded(c, r, 4.0 * s, with_alpha(u.accent, 0.25 * k));
            }
        }
        let cr = self.combo_rect(t, w, ComboId::Category);
        let locked = self.category_locked();
        let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == ComboId::Category);
        widgets::combo(
            c,
            fonts,
            cr,
            self.draft.category.name(),
            !locked && self.hover == Some(Target::Combo(ComboId::Category)),
            open,
            s,
            t,
        );
        if locked {
            rounded(c, cr, 4.0 * s, with_alpha(u.bg, 0.45));
        }
        if self.name_hint {
            let r = self.r(t, w, CONTENT_X + 4.0, 104.0, 0.0, 14.0);
            label(
                c,
                regular,
                "Der Name folgt der Dicke.",
                t.size.font_detail * s,
                r.x,
                r.y + 11.0 * s,
                u.accent,
            );
        }
        // Schnittbild
        let hover_layer = self.hover_layer();
        self.paint_section_view(c, t, regular, w);
        // Kennwerte
        let users = self
            .sel
            .filter(|_| !self.draft_new)
            .map_or(0, |id| self.work.type_users(id).len());
        let u_value = self.work.u_value_of(&self.draft);
        let structure: Vec<String> = self
            .draft
            .layers
            .iter()
            .filter(|l| l.core && l.function == LayerFunction::Structure)
            .map(|l| {
                let name = self
                    .work
                    .material(l.material)
                    .map_or("", |m| m.name.as_str());
                format!("{name} {}", cm_field(l.thickness))
            })
            .collect();
        let rows: Vec<(&str, String, Option<Flash>)> = vec![
            ("Dicke", cm_text(self.draft.thickness()), Some(Flash::Thick)),
            (
                "U-Wert",
                match u_value {
                    Some(v) => format!("≈ {} W/m²K", de(v, 2)),
                    None => "–".into(),
                },
                Some(Flash::U),
            ),
            (
                "Tragend",
                if structure.is_empty() {
                    "nein".into()
                } else {
                    structure.join(", ")
                },
                None,
            ),
            (
                "Verbaut",
                match users {
                    0 => "noch nicht".into(),
                    1 => "1 Wand".into(),
                    n => format!("{n} Wände"),
                },
                None,
            ),
        ];
        for (i, (k, v, fl)) in rows.iter().enumerate() {
            let r = self.r(t, w, RIGHT_X, 124.0 + i as f32 * 30.0, 300.0, 26.0);
            let kf = fl.map_or(0.0, |f| self.flash_k(f, t));
            if kf > 0.0 {
                let vr = Rect::new(r.x + 118.0 * s, r.y, r.w - 118.0 * s, r.h);
                rounded(c, vr, 4.0 * s, with_alpha(u.accent, 0.3 * kf));
            }
            label(c, regular, k, small, r.x, r.y + 18.0 * s, u.text_dim);
            let vw = bold.map_or(0.0, |ft| ft.width(v, small));
            label(
                c,
                bold,
                v,
                small,
                r.x + r.w - 10.0 * s - vw,
                r.y + 18.0 * s,
                u.text,
            );
        }
        if self.exterior() {
            self.paint_bearing(c, t, fonts, w);
        }
        // Standard
        let sr = self.standard_rect(t, w);
        let on = self
            .sel
            .is_some_and(|id| !self.draft_new && self.work.is_default_type(id));
        widgets::checkbox(c, sr, on, self.hover == Some(Target::Standard), s, t);
        let text = match self.draft.category {
            TypeCategory::ExteriorWall => "Standard für neue Gebäude",
            TypeCategory::InteriorWall => "Standard für neue Innenwände",
            // Waagerechte Arten zeigt das Katalogfenster nicht
            _ => "Standard",
        };
        label(
            c,
            regular,
            text,
            small,
            sr.x + sr.w + 10.0 * s,
            sr.y + 12.5 * s,
            u.text,
        );
        // Schichten
        let hy = self.r(t, w, CONTENT_X, ROWS_Y - 36.0, 0.0, 0.0).y;
        label(
            c,
            bold,
            "Schichten",
            t.size.font * s,
            self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x,
            hy,
            u.text,
        );
        let hw = bold.map_or(70.0 * s, |ft| ft.width("Schichten", t.size.font * s));
        let x_after = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x + hw + 10.0 * s;
        let sub = if self.draft.category == TypeCategory::ExteriorWall {
            "von außen nach innen"
        } else {
            "von einer Seite zur anderen"
        };
        label(
            c,
            regular,
            sub,
            t.size.font_detail * s,
            x_after,
            hy,
            u.text_dim,
        );
        let col_y = self.r(t, w, 0.0, ROWS_Y - 12.0, 0.0, 0.0).y;
        for (x, text) in [(330.0, "Baustoff"), (588.0, "Dicke"), (684.0, "Funktion")] {
            let r = self.r(t, w, x, 0.0, 0.0, 0.0);
            label(
                c,
                bold,
                text,
                t.size.font_detail * s,
                r.x,
                col_y,
                u.text_dim,
            );
        }
        for (i, l) in self.draft.layers.iter().enumerate() {
            let row = self.r(
                t,
                w,
                CONTENT_X - 4.0,
                ROWS_Y + i as f32 * ROW_H - 4.0,
                562.0,
                ROW_H,
            );
            let dragging = self.drag == Some(Drag::Row(i));
            if dragging {
                rounded(
                    c,
                    Rect::new(row.x + 2.0 * s, row.y + 3.0 * s, row.w, row.h),
                    6.0 * s,
                    u.shadow,
                );
                rounded(c, row, 6.0 * s, u.pressed);
            } else if hover_layer == Some(i) {
                rounded(c, row, 6.0 * s, u.hover);
            }
            // Griff ⠿
            let g = self.r(
                t,
                w,
                CONTENT_X + 6.0,
                ROWS_Y + i as f32 * ROW_H + 7.0,
                0.0,
                0.0,
            );
            for (dx, dy) in [
                (0.0, 0.0),
                (5.0, 0.0),
                (0.0, 5.0),
                (5.0, 5.0),
                (0.0, 10.0),
                (5.0, 10.0),
            ] {
                let d = (2.0 * s).round().max(1.0);
                rounded(
                    c,
                    Rect::new((g.x + dx * s).round(), (g.y + dy * s).round(), d, d),
                    d * 0.5,
                    u.text_dim,
                );
            }
            let mr = self.combo_rect(t, w, ComboId::Material(i));
            let mat = self.work.material(l.material);
            let icon = mat.map(|_| {
                let look = mat_look(&self.work, t, l.material);
                let (iw, ih) = ((22.0 * s).round(), (14.0 * s).round());
                let mut ic = Canvas::new(iw as usize, ih as usize);
                paint_thumb(&mut ic, Rect::new(0.0, 0.0, iw, ih), &look, s);
                ic
            });
            let open = |id| matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
            widgets::combo_icon(
                c,
                fonts,
                mr,
                mat.map_or("–", |m| m.name.as_str()),
                icon.as_ref(),
                self.hover == Some(Target::Combo(ComboId::Material(i)))
                    && l.function != LayerFunction::AirGap,
                open(ComboId::Material(i)),
                s,
                t,
            );
            if let Some(r) = self.field_rect(t, w, FieldId::Thick(i)) {
                let v = self.field_value(FieldId::Thick(i));
                let st = self.field_state(FieldId::Thick(i), &v);
                widgets::field(c, fonts, r, &st, s, t);
            }
            let fr = self.combo_rect(t, w, ComboId::Function(i));
            widgets::combo(
                c,
                fonts,
                fr,
                function_name(l.function),
                self.hover == Some(Target::Combo(ComboId::Function(i))),
                open(ComboId::Function(i)),
                s,
                t,
            );
            let rm = self.r(t, w, 836.0, ROWS_Y + i as f32 * ROW_H, 26.0, 26.0);
            let col = if self.hover == Some(Target::Remove(i)) {
                u.text
            } else {
                u.text_dim
            };
            let (cx0, cy0, d) = (rm.x + rm.w * 0.5, rm.y + rm.h * 0.5, 4.0 * s);
            let mut p = Path::new();
            p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.2 * s);
            p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.2 * s);
            c.fill(&p, col);
        }
        let ar = self.add_layer_rect(t, w);
        let col = if self.hover == Some(Target::AddLayer) {
            u.accent_hover
        } else {
            u.accent
        };
        label(
            c,
            bold,
            "+ Schicht",
            t.size.font * s,
            ar.x + 4.0 * s,
            ar.y + 16.0 * s,
            col,
        );
        // Merkmale
        let mr = self.r(t, w, PROPS_X, ROWS_Y - 36.0, 0.0, 0.0);
        label(c, bold, "Merkmale", t.size.font * s, mr.x, mr.y, u.text);
        for (i, key) in TYPE_PROPS.iter().enumerate() {
            let Some(r) = self.field_rect(t, w, FieldId::Prop(i)) else {
                continue;
            };
            label(
                c,
                regular,
                key,
                t.size.font_detail * s,
                r.x,
                r.y - 5.0 * s,
                u.text_dim,
            );
            let v = self.field_value(FieldId::Prop(i));
            let st = self.field_state(FieldId::Prop(i), &v);
            widgets::text_field(c, fonts, r, &st, s, t);
        }
    }

    /// Deckenauflager (Skizze 3b): Umschalter, bei „fest“ Auflagertiefe und
    /// Baustoff des Randstreifens.
    fn paint_bearing(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let small = t.size.font_small * s;
        let line = s.round().max(1.0);
        let head = self.r(t, w, RIGHT_X, 244.0, 0.0, 0.0);
        label(
            c,
            regular,
            "Deckenauflager",
            small,
            head.x,
            head.y + 18.0 * s,
            u.text_dim,
        );
        rounded(c, self.bearing_box(t, w), 7.0 * s, u.field);
        let blocked = self.fixed_blocked().is_some();
        for fixed in [false, true] {
            let r = self.bearing_rect(t, w, fixed);
            let on = self.fixed() == fixed;
            let off = fixed && blocked;
            if on {
                rounded(c, r, 6.0 * s, u.pressed);
                outline(c, r, 6.0 * s, line, u.accent);
            } else if !off && self.hover == Some(Target::Bearing(fixed)) {
                rounded(c, r, 6.0 * s, u.hover);
            }
            let text = if fixed {
                "fest, Rest Randstreifen"
            } else {
                "ganze tragende Schicht"
            };
            let tw = regular.map_or(0.0, |ft| ft.width(text, small));
            let col = if off {
                u.text_disabled
            } else if on {
                u.text
            } else {
                u.text_dim
            };
            label(
                c,
                regular,
                text,
                small,
                r.x + (r.w - tw) * 0.5,
                r.y + 17.5 * s,
                col,
            );
        }
        if !self.fixed() {
            return;
        }
        if let Some(r) = self.field_rect(t, w, FieldId::Depth) {
            label(
                c,
                regular,
                "Auflagertiefe",
                small,
                head.x,
                r.y + 18.0 * s,
                u.text_dim,
            );
            let v = self.field_value(FieldId::Depth);
            let mut st = self.field_state(FieldId::Depth, &v);
            // Passt die Tiefe nicht mehr zu den Schichten: rot, OK gesperrt
            st.invalid |= self.draft.bearing_problem().is_some();
            widgets::field(c, fonts, r, &st, s, t);
        }
        let cr = self.combo_rect(t, w, ComboId::Strip);
        label(
            c,
            regular,
            "Randstreifen",
            small,
            head.x,
            cr.y + 18.0 * s,
            u.text_dim,
        );
        let mid = self.draft.strip_material();
        let mat = mid.and_then(|m| self.work.material(m));
        let icon = mid.filter(|_| mat.is_some()).map(|m| {
            let look = mat_look(&self.work, t, m);
            let (iw, ih) = ((22.0 * s).round(), (14.0 * s).round());
            let mut ic = Canvas::new(iw as usize, ih as usize);
            paint_thumb(&mut ic, Rect::new(0.0, 0.0, iw, ih), &look, s);
            ic
        });
        let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == ComboId::Strip);
        widgets::combo_icon(
            c,
            fonts,
            cr,
            mat.map_or("–", |m| m.name.as_str()),
            icon.as_ref(),
            self.hover == Some(Target::Combo(ComboId::Strip)),
            open,
            s,
            t,
        );
    }

    fn field_state<'a>(&'a self, f: FieldId, value: &'a str) -> FieldState<'a> {
        let e = self.edit.as_ref().filter(|e| e.field == f);
        FieldState {
            text: e.map_or(value, |e| e.text.text.as_str()),
            unit: f.unit(),
            hover: self.hover == Some(Target::Field(f)),
            focus: e.is_some(),
            invalid: e.is_some_and(|e| e.invalid.is_some()),
            caret: e.map(|e| e.text.caret),
            select: e.map(|e| e.text.selection()),
            disabled: false,
        }
    }

    fn paint_company(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let x0 = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x;
        let Some(lib) = self.company_lib() else {
            let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
            label(
                c,
                bold,
                "Kein Firmenkatalog",
                t.size.font_title * s,
                x0,
                y,
                u.text,
            );
            let text = "Ohne Einstellungsdatei (Bildschirmfotos, Tests) arbeitet Skizzeo mit dem eingebauten Startbestand.";
            label(c, regular, text, small, x0, y + 26.0 * s, u.text_dim);
            return;
        };
        let Some(g) = self.csel else {
            return;
        };
        let Some(ct) = lib.type_by_guid(g).and_then(|id| lib.types.get(id)) else {
            return;
        };
        let pt = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id));
        let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
        label(c, bold, &ct.name, t.size.font_title * s, x0, y, u.text);
        let users = self
            .work
            .type_by_guid(g)
            .map_or(0, |id| self.work.type_users(id).len());
        let state = match self.mark_of(Item::Company(g)) {
            Some(Mark::Same) => "Gleich wie im Projekt.",
            Some(Mark::CompanyNewer) => "Im Firmenkatalog neuer.",
            Some(Mark::ProjectNewer) => "Im Projekt neuer.",
            _ => "Noch nicht im Projekt.",
        };
        let used = match (pt.is_some(), users) {
            (false, _) => String::new(),
            (true, 0) => " Im Projekt nicht verbaut.".into(),
            (true, n) => format!(" Im Projekt {n}-mal verbaut."),
        };
        label(
            c,
            regular,
            &format!("{state}{used}"),
            small,
            x0,
            y + 24.0 * s,
            u.text_dim,
        );
        // Zwei Karten mit Schnittbild
        let newer = self.mark_of(Item::Company(g)) == Some(Mark::CompanyNewer);
        for (i, title) in ["Im Projekt", "Im Firmenkatalog"].iter().enumerate() {
            let r = self.r(t, w, CONTENT_X + i as f32 * 368.0, 128.0, 356.0, 300.0);
            rounded(c, r, 6.0 * s, u.sheet_bg);
            if i == 1 && newer {
                outline(c, r, 6.0 * s, (2.0 * s).round(), u.accent);
            }
            label(
                c,
                bold,
                title,
                small,
                r.x + 14.0 * s,
                r.y + 22.0 * s,
                u.sheet_text,
            );
            let inner = Rect::new(
                r.x + 6.0 * s,
                r.y + 34.0 * s,
                r.w - 12.0 * s,
                r.h - 40.0 * s,
            );
            let look_t = if i == 0 {
                pt.map(|p| {
                    (
                        type_look(&self.work, t, p),
                        p.layers.iter().map(|l| l.thickness).collect::<Vec<_>>(),
                    )
                })
            } else {
                Some((
                    lib_look(lib, t, ct),
                    ct.layers.iter().map(|l| l.thickness).collect(),
                ))
            };
            match look_t {
                Some((look, th)) => paint_section(
                    c,
                    regular,
                    inner,
                    &look,
                    &th,
                    &SectionMarks::default(),
                    s,
                    t,
                    None,
                ),
                None => {
                    let text = "nicht im Projekt";
                    let tw = regular.map_or(0.0, |f| f.width(text, small));
                    label(
                        c,
                        regular,
                        text,
                        small,
                        r.x + (r.w - tw) * 0.5,
                        r.y + r.h * 0.5,
                        u.sheet_text_dim,
                    );
                }
            }
        }
        // Unterschiede
        let diffs = differences(&self.work, pt, lib, ct);
        let hy = self.r(t, w, 0.0, 462.0, 0.0, 0.0).y;
        if !diffs.is_empty() {
            label(c, bold, "Unterschied", t.size.font * s, x0, hy, u.text);
        }
        for (i, (k, a, b)) in diffs.iter().take(5).enumerate() {
            let y = hy + (28.0 + i as f32 * 28.0) * s;
            label(c, regular, k, small, x0, y, u.text_dim);
            let ax = self.r(t, w, 560.0, 0.0, 0.0, 0.0).x;
            let aw = regular.map_or(0.0, |f| f.width(a, small));
            label(c, regular, a, small, ax - aw, y, u.text);
            label(c, regular, "→", small, ax + 22.0 * s, y, u.text_dim);
            label(c, bold, b, small, ax + 52.0 * s, y, u.accent);
        }
        let ny = hy + (40.0 + diffs.len().min(5) as f32 * 28.0) * s;
        for (i, l) in [
            "„Ins Projekt übernehmen“ setzt den Projekttyp auf den Stand der Firma.",
            "Bei verbauten Typen fragt Skizzeo vorher, wie beim Ändern.",
        ]
        .iter()
        .enumerate()
        {
            label(
                c,
                regular,
                l,
                small,
                x0,
                ny + i as f32 * 20.0 * s,
                u.text_dim,
            );
        }
    }

    /// Aufklapper und Karten über dem Fenster.
    pub fn paint_popup(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        real: &Model,
    ) -> Option<(Canvas, i32, i32)> {
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let popup = self.popup.as_ref()?;
        let r = match popup {
            Popup::Combo(cb) => self.combo_rows(cb, w).0,
            Popup::NewType { .. } => self.new_type_card(t, w),
            Popup::NewMat(_) => self.new_mat_card(t, w)?,
            Popup::Export(..) => self.export_card(t, w),
            Popup::Ask(_) => self.ask_card(w),
        };
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        c.set_origin(r.x - m, r.y - m);
        widgets::panel_filled(&mut c, r, s, t, u.menu_bg);
        if !matches!(popup, Popup::Combo(_)) {
            outline(
                &mut c,
                r,
                t.size.corner_radius * s,
                s.round().max(1.0),
                u.accent,
            );
        }
        match popup {
            Popup::Combo(cb) => {
                let (_, rows) = self.combo_rows(cb, w);
                for (i, (row, (text, look))) in rows.iter().zip(&cb.items).enumerate() {
                    let hot = self.hover == Some(Target::Choice(i))
                        || (self.hover.is_none() && cb.sel == Some(i));
                    if hot {
                        rounded(
                            &mut c,
                            Rect::new(row.x + 4.0 * s, row.y + s, row.w - 8.0 * s, row.h - 2.0 * s),
                            4.0 * s,
                            u.hover,
                        );
                    }
                    let mut x = row.x + t.size.field_pad * s;
                    if let Some(look) = look {
                        let (iw, ih) = ((22.0 * s).round(), (14.0 * s).round());
                        paint_thumb(
                            &mut c,
                            Rect::new(x.round(), (row.y + (row.h - ih) * 0.5).round(), iw, ih),
                            look,
                            s,
                        );
                        x += iw + 8.0 * s;
                    }
                    let col = if look.is_none() && matches!(cb.id, ComboId::Material(_)) {
                        u.accent
                    } else {
                        u.text
                    };
                    label(
                        &mut c,
                        regular,
                        text,
                        small,
                        x,
                        row.y + row.h * 0.5 + 5.0 * s,
                        col,
                    );
                }
            }
            Popup::NewType { cat, from } => {
                label(
                    &mut c,
                    bold,
                    "Neuer Bauteiltyp",
                    t.size.font_title * s,
                    r.x + 18.0 * s,
                    r.y + 30.0 * s,
                    u.text,
                );
                label(
                    &mut c,
                    regular,
                    "Was soll es werden?",
                    small,
                    r.x + 18.0 * s,
                    r.y + 54.0 * s,
                    u.text_dim,
                );
                for (i, kind) in TypeCategory::WALLS.into_iter().enumerate() {
                    let b = Rect::new(
                        r.x + (18.0 + i as f32 * 222.0) * s,
                        r.y + 64.0 * s,
                        212.0 * s,
                        70.0 * s,
                    );
                    let on = *cat == kind;
                    rounded(&mut c, b, 6.0 * s, if on { u.pressed } else { u.bg });
                    outline(
                        &mut c,
                        b,
                        6.0 * s,
                        s.round().max(1.0),
                        if on { u.accent } else { u.border },
                    );
                    let look = self.sample_look(kind, t);
                    if let Some(look) = look {
                        paint_thumb(
                            &mut c,
                            Rect::new(b.x + 12.0 * s, b.y + 14.0 * s, 30.0 * s, 42.0 * s),
                            &look,
                            s,
                        );
                    }
                    let (title, sub) = match kind {
                        TypeCategory::ExteriorWall => ("Außenwand", "von außen nach innen"),
                        TypeCategory::InteriorWall => ("Innenwand", "eine oder mehr Schichten"),
                        _ => (kind.name(), "von oben nach unten"),
                    };
                    label(
                        &mut c,
                        bold,
                        title,
                        t.size.font * s,
                        b.x + 54.0 * s,
                        b.y + 30.0 * s,
                        u.text,
                    );
                    label(
                        &mut c,
                        regular,
                        sub,
                        t.size.font_detail * s,
                        b.x + 54.0 * s,
                        b.y + 50.0 * s,
                        u.text_dim,
                    );
                }
                label(
                    &mut c,
                    regular,
                    "Ausgehen von (Bürostandard)",
                    small,
                    r.x + 18.0 * s,
                    r.y + 164.0 * s,
                    u.text_dim,
                );
                let items = self.new_from_items();
                for i in 0..=items.len() {
                    let row = Rect::new(
                        r.x + 18.0 * s,
                        r.y + (174.0 + i as f32 * 36.0) * s,
                        r.w - 36.0 * s,
                        32.0 * s,
                    );
                    let g = items.get(i).map(|x| x.0);
                    let on = *from == g;
                    if on {
                        rounded(&mut c, row, 6.0 * s, u.pressed);
                        outline(&mut c, row, 6.0 * s, s.round().max(1.0), u.accent);
                    } else if self.hover == Some(Target::NewFrom(g)) {
                        rounded(&mut c, row, 6.0 * s, u.hover);
                    }
                    let font = if on { bold } else { regular };
                    match items.get(i) {
                        Some((g, name, th)) => {
                            if let Some(look) = self.company_lib().and_then(|l| {
                                l.types
                                    .get(l.type_by_guid(*g)?)
                                    .map(|ct| lib_look(l, t, ct))
                            }) {
                                paint_thumb(
                                    &mut c,
                                    Rect::new(
                                        row.x + 10.0 * s,
                                        row.y + 6.0 * s,
                                        20.0 * s,
                                        20.0 * s,
                                    ),
                                    &look,
                                    s,
                                );
                            }
                            label(
                                &mut c,
                                font,
                                name,
                                t.size.font * s,
                                row.x + 40.0 * s,
                                row.y + 21.0 * s,
                                u.text,
                            );
                            let tt = cm_text(*th);
                            let tw = regular.map_or(0.0, |f| f.width(&tt, small));
                            label(
                                &mut c,
                                regular,
                                &tt,
                                small,
                                row.x + row.w - 12.0 * s - tw,
                                row.y + 21.0 * s,
                                u.text_dim,
                            );
                        }
                        None => label(
                            &mut c,
                            font,
                            "leer, Schichten selbst anlegen",
                            t.size.font * s,
                            row.x + 40.0 * s,
                            row.y + 21.0 * s,
                            u.text_dim,
                        ),
                    }
                }
            }
            Popup::NewMat(nm) => {
                label(
                    &mut c,
                    bold,
                    "Neuer Baustoff",
                    t.size.font_title * s,
                    r.x + 18.0 * s,
                    r.y + 30.0 * s,
                    u.text,
                );
                for (f, k) in [
                    (FieldId::MatName, "Name"),
                    (FieldId::MatDensity, "Rohdichte"),
                    (FieldId::MatLambda, "λ (für den U-Wert)"),
                ] {
                    if let Some(fr) = self.field_rect(t, w, f) {
                        label(
                            &mut c,
                            regular,
                            k,
                            small,
                            r.x + 18.0 * s,
                            fr.y + 19.0 * s,
                            u.text_dim,
                        );
                        let v = self.field_value(f);
                        let st = self.field_state(f, &v);
                        if f.numeric() {
                            widgets::field(&mut c, fonts, fr, &st, s, t);
                        } else {
                            widgets::text_field(&mut c, fonts, fr, &st, s, t);
                        }
                    }
                }
                for (i, cat) in NEW_MAT_CATS.into_iter().enumerate() {
                    let b = mat_cat_rect(r, i, s);
                    let st = ButtonState {
                        hover: self.hover == Some(Target::MatCat(cat)),
                        active: nm.cat == cat,
                        ..ButtonState::default()
                    };
                    widgets::button(&mut c, fonts, b, cat.name(), st, s, t);
                }
                let hint = "Priorität und Darstellung kommen von der Kategorie.";
                label(
                    &mut c,
                    regular,
                    hint,
                    t.size.font_detail * s,
                    r.x + 18.0 * s,
                    r.y + 204.0 * s,
                    u.text_dim,
                );
            }
            Popup::Export(g, changed) => {
                let name = self
                    .work
                    .type_by_guid(*g)
                    .and_then(|id| self.work.layer_set(id))
                    .map_or("", |t| t.name.as_str());
                let (title, body) = if *changed {
                    (
                        "Der Firmenkatalog wurde inzwischen geändert.".to_string(),
                        "Neu laden übernimmt die Änderungen der anderen und speichert den Typ dann zurück.".to_string(),
                    )
                } else {
                    (
                        format!("„{name}“ in den Firmenkatalog speichern?"),
                        format!(
                            "{}. Das lässt sich nicht rückgängig machen.",
                            self.company.as_ref().map_or("", |c| c.1.as_str())
                        ),
                    )
                };
                label(
                    &mut c,
                    bold,
                    &title,
                    t.size.font * s,
                    r.x + 18.0 * s,
                    r.y + 34.0 * s,
                    u.text,
                );
                for (i, l) in widgets::wrap(regular, &body, small, r.w - 36.0 * s)
                    .iter()
                    .enumerate()
                {
                    label(
                        &mut c,
                        regular,
                        l,
                        small,
                        r.x + 18.0 * s,
                        r.y + (62.0 + i as f32 * 20.0) * s,
                        u.text_dim,
                    );
                }
            }
            Popup::Ask(a) => self.paint_ask(&mut c, t, fonts, w, real, a, r),
        }
        for (b, br, text) in self.card_buttons(t, w) {
            match b {
                Btn::AskNew | Btn::AskChange | Btn::AskClose => {}
                _ => {
                    let st = self.btn_state(b, b == Btn::PopOk, false);
                    widgets::button(&mut c, fonts, br, text, st, s, t);
                }
            }
        }
        Some((c, (r.x - m) as i32, (r.y - m) as i32))
    }

    fn sample_look(&self, cat: TypeCategory, t: &Theme) -> Option<TypeLook> {
        let id = self.work.default_type(cat);
        self.work
            .layer_set(id)
            .map(|ts| type_look(&self.work, t, ts))
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_ask(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        real: &Model,
        a: &Ask,
        r: Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let Some(&g) = a.queue.get(a.at) else {
            return;
        };
        let Some(old) = real.type_by_guid(g).and_then(|id| real.layer_set(id)) else {
            return;
        };
        let Some(new) = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id))
        else {
            return;
        };
        let n = real
            .type_by_guid(g)
            .map_or(0, |id| real.type_users(id).len());
        // Kacheln alt → neu
        let (tw, th) = ((36.0 * s).round(), (58.0 * s).round());
        paint_thumb(
            c,
            Rect::new(r.x + 22.0 * s, r.y + 26.0 * s, tw, th),
            &type_look(real, t, old),
            s,
        );
        label(
            c,
            regular,
            "→",
            small,
            r.x + 62.0 * s,
            r.y + 60.0 * s,
            u.text_dim,
        );
        paint_thumb(
            c,
            Rect::new(r.x + 80.0 * s, r.y + 26.0 * s, tw, th),
            &type_look(&self.work, t, new),
            s,
        );
        let x = r.x + 138.0 * s;
        label(
            c,
            bold,
            &format!("{} ist {n}-mal verbaut.", old.name),
            t.size.font * s,
            x,
            r.y + 42.0 * s,
            u.text,
        );
        let d = new.thickness() - old.thickness();
        let how = if d.abs() < 1e-9 {
            "Die Dicke bleibt.".to_string()
        } else if old.is_external() {
            format!(
                "Sie bleiben außen stehen und werden nach innen {} cm {}.",
                cm_field(d.abs()),
                if d > 0.0 { "dicker" } else { "dünner" }
            )
        } else {
            format!(
                "Sie werden {} cm {}.",
                cm_field(d.abs()),
                if d > 0.0 { "dicker" } else { "dünner" }
            )
        };
        let body = format!("Die markierten Wände ändern sich mit. {how}");
        for (i, l) in widgets::wrap(regular, &body, small, r.x + r.w - 30.0 * s - x)
            .iter()
            .enumerate()
        {
            label(
                c,
                regular,
                l,
                small,
                x,
                r.y + (66.0 + i as f32 * 18.0) * s,
                u.text_dim,
            );
        }
        if a.queue.len() > 1 {
            let text = format!("{} von {}", a.at + 1, a.queue.len());
            label(
                c,
                regular,
                &text,
                t.size.font_detail * s,
                r.x + r.w - 110.0 * s,
                r.y + 27.0 * s,
                u.text_dim,
            );
        }
        let new_name = self.new_name(real, g);
        for (b, br, text) in self.card_buttons(t, w) {
            match b {
                Btn::AskNew | Btn::AskChange => {
                    let accent = b == Btn::AskChange;
                    let hov = self.hover == Some(Target::Btn(b));
                    let (fill, fg, sub) = if accent {
                        (
                            if hov { u.accent_hover } else { u.accent },
                            u.on_accent,
                            u.on_accent,
                        )
                    } else {
                        (if hov { u.hover } else { u.bg }, u.text, u.text_dim)
                    };
                    rounded(c, br, 6.0 * s, fill);
                    if !accent {
                        outline(c, br, 6.0 * s, s.round().max(1.0), u.border);
                    }
                    label(
                        c,
                        bold,
                        text,
                        t.size.font * s,
                        br.x + 16.0 * s,
                        br.y + 28.0 * s,
                        fg,
                    );
                    // Zwei Zeilen, damit der neue Name nicht abgeschnitten wird
                    let lines = if accent {
                        [
                            format!("alle {n} Wände bekommen"),
                            cm_text(new.thickness()).to_string(),
                        ]
                    } else {
                        [
                            format!("„{new_name}“"),
                            "Wände bleiben, wie sie sind".into(),
                        ]
                    };
                    for (k, line) in lines.iter().enumerate() {
                        let line = widgets::ellipsize(
                            regular,
                            line,
                            t.size.font_detail * s,
                            br.w - 32.0 * s,
                        );
                        label(
                            c,
                            regular,
                            &line,
                            t.size.font_detail * s,
                            br.x + 16.0 * s,
                            br.y + (50.0 + k as f32 * 18.0) * s,
                            sub,
                        );
                    }
                }
                Btn::AskClose => {
                    if self.hover == Some(Target::Btn(b)) {
                        rounded(c, br, 6.0 * s, u.hover);
                    }
                    let (cx0, cy0, d) = (br.x + br.w * 0.5, br.y + br.h * 0.5, 4.5 * s);
                    let mut p = Path::new();
                    p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.3 * s);
                    p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.3 * s);
                    c.fill(&p, u.text_dim);
                }
                _ => {}
            }
        }
        let foot = "Beides ist ein Rückgängig-Schritt. × kehrt zum Bearbeiten zurück.";
        let fw = regular.map_or(0.0, |f| f.width(foot, t.size.font_detail * s));
        label(
            c,
            regular,
            foot,
            t.size.font_detail * s,
            r.x + (r.w - fw) * 0.5,
            r.y + r.h - 20.0 * s,
            u.text_dim,
        );
    }
}

/// Aussehen eines Katalogtyps (Baustoffe aus dem Katalog, Darstellung über
/// ein Modell mit dessen Stiften und Schraffuren).
fn lib_look(lib: &Library, t: &Theme, ct: &LayerSet) -> TypeLook {
    let mut m = Model::new();
    m.allow_unstepped();
    let g = ct.guid;
    match import_type(&mut m, lib, g).and_then(|id| m.layer_set(id).cloned()) {
        Some(ts) => type_look(&m, t, &ts),
        None => type_look(&m, t, ct),
    }
}

/// Unterschiede Projekt ↔ Firma: Bezeichnung, vorher, nachher.
fn differences(
    m: &Model,
    pt: Option<&LayerSet>,
    lib: &Library,
    ct: &LayerSet,
) -> Vec<(String, String, String)> {
    let Some(pt) = pt else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if pt.name != ct.name {
        v.push(("Name".into(), pt.name.clone(), ct.name.clone()));
    }
    if pt.code != ct.code {
        v.push(("Kurzzeichen".into(), pt.code.clone(), ct.code.clone()));
    }
    let n = pt.layers.len().max(ct.layers.len());
    for i in 0..n {
        let a = pt.layers.get(i);
        let b = ct.layers.get(i);
        let an = a
            .and_then(|l| m.material(l.material))
            .map(|x| x.name.clone());
        let bn = b
            .and_then(|l| lib.materials.get(l.material))
            .map(|x| x.name.clone());
        let key = format!("Schicht {}", i + 1);
        match (a, b) {
            (Some(a), Some(b)) => {
                if an != bn {
                    v.push((
                        key.clone(),
                        an.clone().unwrap_or_default(),
                        bn.clone().unwrap_or_default(),
                    ));
                }
                if (a.thickness - b.thickness).abs() > 1e-9 {
                    let k = format!("{} ({})", key, bn.clone().unwrap_or_default());
                    v.push((k, cm_text(a.thickness), cm_text(b.thickness)));
                }
                if a.function != b.function {
                    v.push((
                        key,
                        function_name(a.function).into(),
                        function_name(b.function).into(),
                    ));
                }
            }
            (Some(a), None) => v.push((
                key,
                format!("{} {}", an.unwrap_or_default(), cm_text(a.thickness)),
                "–".into(),
            )),
            (None, Some(b)) => v.push((
                key,
                "–".into(),
                format!("{} {}", bn.unwrap_or_default(), cm_text(b.thickness)),
            )),
            _ => {}
        }
    }
    if (pt.thickness() - ct.thickness()).abs() > 1e-9 {
        v.push((
            "Dicke".into(),
            cm_text(pt.thickness()),
            cm_text(ct.thickness()),
        ));
    }
    for k in TYPE_PROPS {
        let a = pt.props.get(k).map(prop_text).unwrap_or_default();
        let b = ct.props.get(k).map(prop_text).unwrap_or_default();
        if a != b {
            v.push((
                k.into(),
                if a.is_empty() { "–".into() } else { a },
                if b.is_empty() { "–".into() } else { b },
            ));
        }
    }
    v
}
