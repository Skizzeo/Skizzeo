//! Einstellungsfenster, Reiter „Linientypen“, „Schraffuren“, „Oberflächen“
//! und „Baustoffe“ (E6). Alle vier haben denselben Aufbau wie „Stifte“: links
//! die Liste mit Vorschaubild je Eintrag und darunter Neu, Duplizieren und
//! Löschen, rechts Bearbeiten und Vorschau. Löschen geht nur, wenn nichts
//! darauf verweist. Baustoffe zeigen nur ihre Darstellungsverweise zum Ändern;
//! die übrigen Werte pflegt BIM.
//!
//! Die Vorschaubilder rechnen mit denselben Formeln wie die Grafikkarte
//! ([`sk_render::fill_color`], [`sk_render::dash_ink`]).

use super::*;
use crate::draw_table::{look_rows, mat_look};
use sk_model::{
    Dash, Fill, FillId, FillKind, FillSpace, HatchLine, LineType, LineTypeId, Material,
    MaterialDisplay, MaterialId, Surface,
};
use sk_render::{DashPattern, SOLID};

/// Breite der Liste links (dip).
const LIST_W: f32 = 250.0;
/// Zeilenabstand umbrochener Einträge (dip).
const WRAP_LINE: f32 = 18.0;
/// Breite der Spalte „Muster“ im Reiter „Linientypen“ (dip).
const LT_THUMB_W: f32 = 80.0;
/// Höchstzahl der Musterzeilen bzw. Linienscharen (Zeichentabelle).
const MAX_ROWS: usize = 2;
/// Strichbreiten der Linientyp-Vorschau (mm).
const PREVIEW_WIDTHS: [f32; 3] = [0.13, 0.35, 0.70];
/// Neue Musterzeile und neue Schar (mm, Grad).
const NEW_DASH: Dash = Dash {
    len_mm: 3.0,
    gap_mm: 1.0,
    dot: false,
};
const NEW_HATCH: HatchLine = HatchLine::solid(45.0, 2.0, 0.0);

// --- Daten ----------------------------------------------------------------------

fn lt_list(m: &Model) -> Vec<(LineTypeId, LineType)> {
    let a = m.attr();
    a.line_types().iter().map(|(i, l)| (i, l.clone())).collect()
}

fn fill_list(m: &Model) -> Vec<(FillId, Fill)> {
    let a = m.attr();
    a.fills().iter().map(|(i, f)| (i, f.clone())).collect()
}

fn surf_list(m: &Model) -> Vec<(SurfaceId, Surface)> {
    let a = m.attr();
    a.surfaces().iter().map(|(i, o)| (i, o.clone())).collect()
}

fn mat_list(m: &Model) -> Vec<(MaterialId, Material)> {
    m.materials().iter().map(|(i, x)| (i, x.clone())).collect()
}

/// Namen der Tabelle eines Reiters.
fn names(m: &Model, tab: Tab) -> Vec<String> {
    let a = m.attr();
    match tab {
        Tab::Pens => a.pens().iter().map(|(_, p)| p.name.clone()).collect(),
        Tab::LineTypes => a.line_types().iter().map(|(_, l)| l.name.clone()).collect(),
        Tab::Fills => a.fills().iter().map(|(_, f)| f.name.clone()).collect(),
        Tab::Surfaces => a.surfaces().iter().map(|(_, o)| o.name.clone()).collect(),
        Tab::Materials => m.materials().iter().map(|(_, x)| x.name.clone()).collect(),
        Tab::Ui => Vec::new(),
    }
}

/// Ist `name` in der Tabelle des Reiters noch frei? Namen sind je Tabelle
/// eindeutig; derselbe Name in einer anderen Tabelle ist erlaubt.
pub fn name_free(m: &Model, tab: Tab, name: &str) -> bool {
    !names(m, tab).iter().any(|n| n == name)
}

/// Freier Name nach dem Muster „Basis“, „Basis 2“, „Basis 3“ …
fn unique(m: &Model, tab: Tab, base: &str) -> String {
    let taken = names(m, tab);
    (1..)
        .map(|i| match i {
            1 => base.to_string(),
            i => format!("{base} {i}"),
        })
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// Stift mit dieser Nummer, sonst der erste.
fn pen_by_number(m: &Model, nr: u16) -> Option<PenId> {
    let a = m.attr();
    a.pens()
        .iter()
        .find(|(_, p)| p.number == nr)
        .or_else(|| a.pens().iter().next())
        .map(|(id, _)| id)
}

/// Einheit eines Felds der Attributreiter.
pub(super) fn attr_unit(f: FieldId) -> &'static str {
    match f {
        FieldId::Dash(..) | FieldId::Hatch(_, 1..) => "mm",
        FieldId::Hatch(_, 0) => "°",
        _ => "",
    }
}

/// „Auf Standard zurücksetzen“ in einem Attributreiter: die Einträge des
/// Startsatzes (gleicher Name) bekommen ihre Startwerte, fehlende kommen
/// wieder dazu; eigene Einträge bleiben. Baustoffe: die Darstellungsverweise.
pub(super) fn reset_attr_tab(s: &mut Scene, tab: Tab) {
    let start = Model::with_seed(0);
    let sa = start.attr();
    s.edit_attr(|m| {
        match tab {
            Tab::LineTypes => {
                for (_, d) in sa.line_types().iter() {
                    let found = lt_list(m).into_iter().find(|(_, l)| l.name == d.name);
                    match found {
                        Some((id, l)) if l.pattern != d.pattern => {
                            let l = LineType {
                                pattern: d.pattern.clone(),
                                ..l
                            };
                            m.set_line_type(id, l);
                        }
                        Some(_) => {}
                        None => {
                            let guid = m.new_guid();
                            m.add_line_type(LineType { guid, ..d.clone() });
                        }
                    }
                }
            }
            Tab::Fills => {
                for (_, d) in sa.fills().iter() {
                    let found = fill_list(m).into_iter().find(|(_, f)| f.name == d.name);
                    match found {
                        Some((id, f)) if (&f.kind, f.space) != (&d.kind, d.space) => {
                            let f = Fill {
                                kind: d.kind.clone(),
                                space: d.space,
                                ..f
                            };
                            m.set_fill(id, f);
                        }
                        Some(_) => {}
                        None => {
                            let guid = m.new_guid();
                            m.add_fill(Fill { guid, ..d.clone() });
                        }
                    }
                }
            }
            Tab::Surfaces => {
                for (_, d) in sa.surfaces().iter() {
                    let found = surf_list(m).into_iter().find(|(_, o)| o.name == d.name);
                    match found {
                        Some((id, o)) if (o.color, o.cut_color) != (d.color, d.cut_color) => {
                            let o = Surface {
                                color: d.color,
                                cut_color: d.cut_color,
                                ..o
                            };
                            m.set_surface(id, o);
                        }
                        Some(_) => {}
                        None => {
                            let guid = m.new_guid();
                            m.add_surface(Surface { guid, ..d.clone() });
                        }
                    }
                }
            }
            Tab::Materials => {
                for (_, d) in start.materials().iter() {
                    let Some((id, mat)) = mat_list(m).into_iter().find(|(_, x)| x.name == d.name)
                    else {
                        continue;
                    };
                    // Verweise des Startsatzes über Name bzw. Stiftnummer
                    let fill = sa.fill(d.cut_fill).and_then(|f| {
                        fill_list(m)
                            .into_iter()
                            .find(|(_, x)| x.name == f.name)
                            .map(|(i, _)| i)
                    });
                    let pen = |p: PenId| {
                        let nr = sa.pen(p)?.number;
                        let a = m.attr();
                        a.pens()
                            .iter()
                            .find(|(_, x)| x.number == nr)
                            .map(|(i, _)| i)
                    };
                    let surface = sa.surface(d.surface).and_then(|o| {
                        surf_list(m)
                            .into_iter()
                            .find(|(_, x)| x.name == o.name)
                            .map(|(i, _)| i)
                    });
                    if let (Some(cut_fill), Some(cut_fg), Some(cut_bg), Some(surface)) =
                        (fill, pen(d.cut_fg), pen(d.cut_bg), surface)
                    {
                        let want = MaterialDisplay {
                            cut_fill,
                            cut_fg,
                            cut_bg,
                            surface,
                        };
                        if mat.display() != want {
                            m.set_material_display(id, want);
                        }
                    }
                }
            }
            Tab::Pens | Tab::Ui => {}
        }
        true
    });
}

// --- Lage -----------------------------------------------------------------------

/// Lage in einem Attributreiter (Fensterkoordinaten).
pub(super) struct AttrLayout {
    pub list: Rect,
    pub body: Rect,
    pub rows: Vec<Rect>,
    pub bar: Option<Rect>,
    pub scroll: f32,
    pub content_h: f32,
    /// Neu, Duplizieren, Löschen (nicht bei Baustoffen).
    pub buttons: Option<[Rect; 3]>,
    /// Grundlinie des Hinweises unter den Knöpfen.
    pub hint_y: f32,
    /// Linke Kante der Spalte mit dem Vorschaubild.
    pub thumb_x: f32,
    pub side: Rect,
    pub items: Vec<(Rect, Target)>,
    pub texts: Vec<UiText>,
    /// Nur zum Ablesen (BIM-Daten): Feld und Text.
    pub readonly: Vec<(Rect, String)>,
    pub preview: Option<Rect>,
}

impl Prefs {
    /// Gewählte Zeile im aktuellen Attributreiter (auf die Liste begrenzt).
    fn sel_index(&self, m: &Model) -> Option<usize> {
        let k = self.tab.attr_slot()?;
        let n = names(m, self.tab).len();
        (n > 0).then(|| self.attr_sel[k].min(n - 1))
    }

    fn sel_lt(&self, m: &Model) -> Option<(LineTypeId, LineType)> {
        (self.tab == Tab::LineTypes)
            .then(|| self.sel_index(m).and_then(|i| lt_list(m).get(i).cloned()))?
    }

    fn sel_fill(&self, m: &Model) -> Option<(FillId, Fill)> {
        (self.tab == Tab::Fills)
            .then(|| self.sel_index(m).and_then(|i| fill_list(m).get(i).cloned()))?
    }

    fn sel_surf(&self, m: &Model) -> Option<(SurfaceId, Surface)> {
        (self.tab == Tab::Surfaces)
            .then(|| self.sel_index(m).and_then(|i| surf_list(m).get(i).cloned()))?
    }

    fn sel_mat(&self, m: &Model) -> Option<(MaterialId, Material)> {
        (self.tab == Tab::Materials)
            .then(|| self.sel_index(m).and_then(|i| mat_list(m).get(i).cloned()))?
    }

    /// Verwender des gewählten Eintrags.
    fn sel_users(&self, m: &Model) -> Vec<String> {
        let r = match self.tab {
            Tab::LineTypes => self.sel_lt(m).map(|x| AttrRef::LineType(x.0)),
            Tab::Fills => self.sel_fill(m).map(|x| AttrRef::Fill(x.0)),
            Tab::Surfaces => self.sel_surf(m).map(|x| AttrRef::Surface(x.0)),
            _ => None,
        };
        r.map(|r| m.attr_users(r).iter().map(|u| u.label()).collect())
            .unwrap_or_default()
    }

    pub(super) fn attr_users_text(&self, s: &Scene) -> String {
        self.sel_users(s.model()).join(", ")
    }

    fn select_last(&mut self, m: &Model) {
        if let Some(k) = self.tab.attr_slot() {
            self.attr_sel[k] = names(m, self.tab).len().saturating_sub(1);
            self.attr_scroll[k] = f32::MAX;
        }
    }

    /// Pfeiltasten in der Liste.
    pub(super) fn step_row(&mut self, down: bool, s: &Scene) {
        let Some(k) = self.tab.attr_slot() else {
            return;
        };
        let n = names(s.model(), self.tab).len();
        let i = self.attr_sel[k].min(n.saturating_sub(1));
        self.attr_sel[k] = if down {
            (i + 1).min(n.saturating_sub(1))
        } else {
            i.saturating_sub(1)
        };
        // Sichtbar halten: Bildlauf beim nächsten Zeichnen begrenzen
        let row = 28.0;
        let top = self.attr_sel[k] as f32 * row;
        if top < self.attr_scroll[k] {
            self.attr_scroll[k] = top;
        }
    }

    pub(super) fn attr_layout(&self, t: &Theme, w: &Win, sc: &Scene) -> AttrLayout {
        let c = self.content(t, w);
        let s = w.scale;
        let m = sc.model();
        let list_w = (LIST_W * s).min(c.w * 0.42).round();
        let list = Rect::new(c.x, c.y, list_w, c.h);
        let row = t.size.table_row * s;
        let has_buttons = self.tab != Tab::Materials;
        // Knöpfe + zwei Hinweiszeilen (Schraffuren: vier)
        let foot = match self.tab {
            Tab::Materials => 8.0,
            Tab::Fills => 130.0,
            _ => 96.0,
        } * s;
        let body = Rect::new(list.x, c.y + row, list.w, (c.h - row - foot).max(row));
        let n = names(m, self.tab).len();
        let content_h = n as f32 * row;
        let bar = (content_h > body.h).then(|| {
            let bw = t.size.scrollbar * s;
            Rect::new(body.x + body.w - bw, body.y, bw, body.h)
        });
        let k = self.tab.attr_slot().unwrap_or(0);
        let scroll = self.attr_scroll[k].clamp(0.0, (content_h - body.h).max(0.0));
        let row_w = body.w - bar.map_or(0.0, |b| b.w + 4.0 * s);
        let rows = (0..n)
            .map(|i| Rect::new(body.x, body.y + i as f32 * row - scroll, row_w, row))
            .collect();
        let by = (body.y + body.h + 12.0 * s).round();
        let b = |x: f32, bw: f32| {
            Rect::new(
                (list.x + x * s).round(),
                by,
                (bw * s).round(),
                (30.0 * s).round(),
            )
        };
        let thumb_w = match self.tab {
            Tab::LineTypes => LT_THUMB_W,
            Tab::Surfaces => t.size.swatch_w,
            _ => t.size.list_thumb_w,
        } * s;
        let thumb_x = list.x + row_w - 10.0 * s - thumb_w;
        let side_x = list.x + list.w + 2.0 * PAD * s;
        let side = Rect::new(side_x, c.y, c.x + c.w - side_x, c.h);
        let mut l = AttrLayout {
            list,
            body,
            rows,
            bar,
            scroll,
            content_h,
            buttons: has_buttons.then(|| [b(0.0, 66.0), b(72.0, 110.0), b(188.0, 80.0)]),
            hint_y: by + 30.0 * s + 22.0 * s,
            thumb_x,
            side,
            items: Vec::new(),
            texts: Vec::new(),
            readonly: Vec::new(),
            preview: None,
        };
        match self.tab {
            Tab::LineTypes => self.lt_side(&mut l, t, s, m),
            Tab::Fills => self.fill_side(&mut l, t, s, m),
            Tab::Surfaces => self.surf_side(&mut l, t, s, m),
            Tab::Materials => self.mat_side(&mut l, t, s, m),
            _ => {}
        }
        l
    }

    fn lt_side(&self, l: &mut AttrLayout, t: &Theme, s: f32, m: &Model) {
        let Some((_, lt)) = self.sel_lt(m) else {
            return;
        };
        let (x, y0, sw) = (l.side.x, l.side.y, l.side.w);
        let fh = t.size.field_height * s;
        l.texts
            .push(UiText::heading(x, y0 + 18.0 * s, lt.name.clone()));
        l.texts.push(UiText::label(x, y0 + 54.0 * s, "Name"));
        let fx = x + 80.0 * s;
        l.items.push((
            Rect::new(fx, y0 + 36.0 * s, (sw - 80.0 * s).min(220.0 * s), fh),
            Target::Field(FieldId::Name),
        ));
        let mut y = y0 + 84.0 * s;
        l.texts.push(UiText::group_title(x, y + 14.0 * s, "Muster"));
        y += 26.0 * s;
        let fw = 100.0 * s;
        if lt.pattern.is_empty() {
            l.texts
                .push(UiText::dim(x, y + 18.0 * s, "Ohne Zeile: Volllinie"));
            y += 30.0 * s;
        } else {
            for (i, label) in ["Strich", "Lücke", "Punkt danach"].iter().enumerate() {
                l.texts.push(UiText::dim(
                    x + i as f32 * (fw + 12.0 * s),
                    y + 10.0 * s,
                    *label,
                ));
            }
            y += 16.0 * s;
            for i in 0..lt.pattern.len() {
                for k in 0..2 {
                    let r = Rect::new(x + k as f32 * (fw + 12.0 * s), y, fw, fh);
                    l.items.push((r, Target::Field(FieldId::Dash(i, k))));
                }
                let cb = t.size.checkbox * s;
                let r = Rect::new(
                    x + 2.0 * (fw + 12.0 * s) + 4.0 * s,
                    y + (fh - cb) * 0.5,
                    cb,
                    cb,
                );
                l.items.push((r, Target::Check(i)));
                y += fh + 8.0 * s;
            }
        }
        y += 4.0 * s;
        let bh = 30.0 * s;
        l.items
            .push((Rect::new(x, y, 140.0 * s, bh), Target::RowAdd));
        l.items
            .push((Rect::new(x + 148.0 * s, y, 130.0 * s, bh), Target::RowDel));
        y += bh + 26.0 * s;
        l.texts.push(UiText::group_title(x, y, "Vorschau"));
        y += 10.0 * s;
        let pw = (sw - 64.0 * s).min(t.size.preview_w * s);
        let pr = Rect::new(x, y, pw, PREVIEW_WIDTHS.len() as f32 * 30.0 * s);
        l.preview = Some(pr);
        y = pr.y + pr.h + 24.0 * s;
        let users = self.sel_users(m);
        if users.is_empty() {
            l.texts.push(UiText::dim(x, y, "Nicht verwendet"));
            return;
        }
        l.texts.push(UiText::dim(x, y, "Verwendet von:"));
        // je Zeile ein Verwender, so viele wie Platz ist
        let line = 18.0 * s;
        let room = ((l.side.y + l.side.h - y) / line).floor().max(2.0) as usize - 1;
        let shown = if users.len() > room {
            room - 1
        } else {
            users.len()
        };
        for (k, u) in users.iter().take(shown).enumerate() {
            l.texts
                .push(UiText::dim(x, y + (k + 1) as f32 * line, u.clone()));
        }
        if shown < users.len() {
            let rest = format!("… und {} weitere", users.len() - shown);
            l.texts
                .push(UiText::dim(x, y + (shown + 1) as f32 * line, rest));
        }
    }

    fn fill_side(&self, l: &mut AttrLayout, t: &Theme, s: f32, m: &Model) {
        let Some((_, fill)) = self.sel_fill(m) else {
            return;
        };
        let (x, y0, sw) = (l.side.x, l.side.y, l.side.w);
        let fh = t.size.field_height * s;
        let fx = x + 64.0 * s;
        let fw = (sw - 64.0 * s).min(200.0 * s);
        l.texts
            .push(UiText::heading(x, y0 + 18.0 * s, fill.name.clone()));
        let mut y = y0 + 34.0 * s;
        for (label, target) in [
            ("Name", Target::Field(FieldId::Name)),
            ("Art", Target::Combo(ComboId::FillKind)),
            ("Bezug", Target::Combo(ComboId::FillSpace)),
        ] {
            l.texts.push(UiText::label(x, y + 18.0 * s, label));
            l.items.push((Rect::new(fx, y, fw, fh), target));
            y += fh + 6.0 * s;
        }
        y += 6.0 * s;
        match &fill.kind {
            FillKind::Lines(lines) => {
                l.texts
                    .push(UiText::group_title(x, y + 14.0 * s, "Linienscharen"));
                y += 22.0 * s;
                let gap = 6.0 * s;
                let cw = ((sw - 4.0 * gap) / 5.0).min(78.0 * s);
                for (i, label) in ["Winkel", "Abstand", "Versatz", "Strich", "Lücke"]
                    .iter()
                    .enumerate()
                {
                    l.texts
                        .push(UiText::dim(x + i as f32 * (cw + gap), y + 10.0 * s, *label));
                }
                y += 16.0 * s;
                for i in 0..lines.len().min(MAX_ROWS) {
                    for k in 0..5 {
                        let r = Rect::new(x + k as f32 * (cw + gap), y, cw, fh);
                        l.items.push((r, Target::Field(FieldId::Hatch(i, k))));
                    }
                    y += fh + 6.0 * s;
                }
                let bh = 30.0 * s;
                l.items
                    .push((Rect::new(x, y, 140.0 * s, bh), Target::RowAdd));
                l.items
                    .push((Rect::new(x + 148.0 * s, y, 130.0 * s, bh), Target::RowDel));
                y += bh + 18.0 * s;
                l.texts.push(UiText::dim(
                    x,
                    y,
                    "Winkel gegen den Uhrzeigersinn: 45° = „/“, 135° = „\\“",
                ));
                y += 10.0 * s;
            }
            FillKind::Zigzag { .. } => {
                l.texts.push(UiText::label(x, y + 18.0 * s, "Periode"));
                l.items.push((
                    Rect::new(fx, y, 90.0 * s, fh),
                    Target::Field(FieldId::Zigzag),
                ));
                l.texts
                    .push(UiText::dim(fx + 98.0 * s, y + 18.0 * s, "Schichtdicken"));
                y += fh + 4.0 * s;
            }
            FillKind::Empty | FillKind::Solid => {}
        }
        y += 22.0 * s;
        l.texts.push(UiText::group_title(x, y, "Vorschau"));
        y += 10.0 * s;
        let bottom = l.side.y + l.side.h;
        let ph = (t.size.preview_h * s).min(bottom - y).max(40.0 * s);
        let pw = (t.size.preview_w * s).min(sw * 0.62);
        l.preview = Some(Rect::new(x, y, pw, ph));
        let tx = x + pw + 14.0 * s;
        let users = self.sel_users(m);
        l.texts
            .push(UiText::dim(tx, y + 14.0 * s, "Verwendet von:"));
        let ty = y + 32.0 * s;
        if users.is_empty() {
            l.texts.push(UiText::dim(tx, ty, "nichts"));
        } else {
            l.texts.push(UiText::wrapped(tx, ty, users.join("\n")));
        }
    }

    fn surf_side(&self, l: &mut AttrLayout, t: &Theme, s: f32, m: &Model) {
        let Some((id, o)) = self.sel_surf(m) else {
            return;
        };
        let (x, y0, sw) = (l.side.x, l.side.y, l.side.w);
        let fh = t.size.field_height * s;
        let vx = x + 168.0 * s;
        l.texts
            .push(UiText::heading(x, y0 + 18.0 * s, o.name.clone()));
        let mut y = y0 + 34.0 * s;
        l.texts.push(UiText::label(x, y + 18.0 * s, "Name"));
        l.items.push((
            Rect::new(vx, y, (sw - 168.0 * s).min(200.0 * s), fh),
            Target::Field(FieldId::Name),
        ));
        y += fh + 10.0 * s;
        let (ww, wh) = (t.size.swatch_w * s, t.size.swatch_h * s);
        for (label, ct) in [
            ("Farbe Ansichtsfläche", ColorTarget::SurfFace(id)),
            ("Farbe Schnittfläche 3D", ColorTarget::SurfCut(id)),
        ] {
            l.texts.push(UiText::label(x, y + 15.0 * s, label));
            l.items
                .push((Rect::new(vx, y + 2.0 * s, ww, wh), Target::Swatch(ct)));
            y += 30.0 * s;
        }
        y += 6.0 * s;
        l.texts.push(UiText::dim(
            x,
            y + 10.0 * s,
            "Glanz, Transparenz und Textur folgen später.",
        ));
        y += 42.0 * s;
        l.texts.push(UiText::group_title(x, y, "Vorschau"));
        y += 10.0 * s;
        let bottom = l.side.y + l.side.h;
        let ph = (t.size.preview_h * s).min(bottom - y).max(40.0 * s);
        let pw = (200.0 * s).min(sw * 0.56);
        l.preview = Some(Rect::new(x, y, pw, ph));
        let tx = x + pw + 14.0 * s;
        let mut ty = y + 14.0 * s;
        for line in ["Licht wie in der", "3D-Ansicht; Ecke", "in Schnittfarbe."] {
            l.texts.push(UiText::dim(tx, ty, line));
            ty += 18.0 * s;
        }
        ty += 10.0 * s;
        let users = self.sel_users(m);
        l.texts.push(UiText::dim(tx, ty, "Verwendet von:"));
        ty += 18.0 * s;
        if users.is_empty() {
            l.texts.push(UiText::dim(tx, ty, "nichts"));
        } else {
            l.texts.push(UiText::wrapped(tx, ty, users.join("\n")));
        }
    }

    fn mat_side(&self, l: &mut AttrLayout, t: &Theme, s: f32, m: &Model) {
        let Some((_, mat)) = self.sel_mat(m) else {
            return;
        };
        let (x, y0, sw) = (l.side.x, l.side.y, l.side.w);
        let fh = t.size.field_height * s;
        let vx = x + 124.0 * s;
        let vw = (sw - 124.0 * s).min(240.0 * s);
        l.texts
            .push(UiText::heading(x, y0 + 18.0 * s, mat.name.clone()));
        let mut y = y0 + 32.0 * s;
        l.texts
            .push(UiText::group_title(x, y + 14.0 * s, "Darstellung"));
        y += 22.0 * s;
        for (label, id) in [
            ("Schraffur", ComboId::MatFill),
            ("Stift Schraffur", ComboId::MatFg),
            ("Stift Grund", ComboId::MatBg),
            ("Oberfläche 3D", ComboId::MatSurface),
        ] {
            l.texts.push(UiText::label(x, y + 18.0 * s, label));
            l.items.push((Rect::new(vx, y, vw, fh), Target::Combo(id)));
            y += fh + 6.0 * s;
        }
        y += 10.0 * s;
        l.texts
            .push(UiText::group_title(x, y + 14.0 * s, "BIM-Daten"));
        y += 22.0 * s;
        let rw = (150.0 * s).min(vw);
        let lambda = mat.lambda.map_or("–".to_string(), |v| num(v as f32, 3));
        for (label, value) in [
            ("Kategorie", mat.category.name().to_string()),
            ("Priorität", mat.priority.to_string()),
            (
                "Rohdichte",
                format!("{} kg/m³", num_short(mat.density as f32)),
            ),
            ("λ", format!("{lambda} W/(mK)")),
        ] {
            l.texts.push(UiText::label(x, y + 17.0 * s, label));
            l.readonly.push((Rect::new(vx, y, rw, fh - 2.0 * s), value));
            y += fh + 2.0 * s;
        }
        l.texts.push(UiText::dim(
            x,
            y + 14.0 * s,
            "Diese Werte pflegt die Bauteilverwaltung (BIM).",
        ));
        y += 40.0 * s;
        l.texts.push(UiText::group_title(x, y, "Vorschau"));
        y += 10.0 * s;
        let bottom = l.side.y + l.side.h;
        let ph = (t.size.preview_h * s).min(bottom - y).max(30.0 * s);
        let pw = (t.size.preview_w * s).min(sw);
        l.preview = Some(Rect::new(x, y, pw, ph));
    }

    // --- Treffer --------------------------------------------------------------

    pub(super) fn attr_hit(&self, t: &Theme, w: &Win, s: &Scene, x: f64, y: f64) -> Option<Target> {
        self.tab.attr_slot()?;
        let l = self.attr_layout(t, w, s);
        if l.bar.is_some_and(|b| b.contains(x, y)) {
            return Some(Target::Bar(BarId::List));
        }
        if l.body.contains(x, y) {
            return l
                .rows
                .iter()
                .position(|r| r.contains(x, y))
                .map(Target::Row);
        }
        if let Some(b) = l.buttons {
            let targets = [Target::AttrNew, Target::AttrDup, Target::AttrDel];
            for (r, tg) in b.iter().zip(targets) {
                if r.contains(x, y) {
                    return Some(tg);
                }
            }
        }
        l.items
            .iter()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, tg)| *tg)
    }

    // --- Felder ---------------------------------------------------------------

    pub(super) fn attr_field_value(&self, f: FieldId, s: &Scene) -> String {
        let m = s.model();
        match f {
            FieldId::Name => {
                let i = self.sel_index(m).unwrap_or(0);
                names(m, self.tab).get(i).cloned().unwrap_or_default()
            }
            FieldId::Dash(i, k) => self
                .sel_lt(m)
                .and_then(|(_, l)| l.pattern.get(i).copied())
                .map_or(String::new(), |d| {
                    num(if k == 0 { d.len_mm } else { d.gap_mm }, 2)
                }),
            FieldId::Hatch(i, k) => self
                .sel_fill(m)
                .and_then(|(_, f)| match f.kind {
                    FillKind::Lines(l) => l.get(i).copied(),
                    _ => None,
                })
                .map_or(String::new(), |h| match k {
                    0 => num_short(h.angle_deg),
                    1 => num(h.spacing_mm, 2),
                    2 => num(h.offset_mm, 2),
                    3 => num(h.dash_mm, 2),
                    _ => num(h.gap_mm, 2),
                }),
            FieldId::Zigzag => match self.sel_fill(m).map(|(_, f)| f.kind) {
                Some(FillKind::Zigzag { period }) => num(period, 1),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    pub(super) fn apply_attr_value(
        &mut self,
        f: FieldId,
        text: &str,
        cx: &mut Ctx,
        out: &mut Out,
    ) -> Result<(), String> {
        let m = cx.scene.model();
        if f == FieldId::Name {
            let name = text.trim().to_string();
            if name.is_empty() {
                return Err("Der Eintrag braucht einen Namen".into());
            }
            let i = self.sel_index(m).unwrap_or(0);
            let own = names(m, self.tab).get(i).cloned().unwrap_or_default();
            if name != own && !name_free(m, self.tab, &name) {
                return Err(format!("„{name}“ gibt es schon"));
            }
            if name == own {
                return Ok(());
            }
            let (lt, fill, surf) = (self.sel_lt(m), self.sel_fill(m), self.sel_surf(m));
            out.model |= cx.scene.edit_attr(|m| match (lt, fill, surf) {
                (Some((id, l)), _, _) => m.set_line_type(id, LineType { name, ..l }),
                (_, Some((id, f)), _) => m.set_fill(id, Fill { name, ..f }),
                (_, _, Some((id, o))) => m.set_surface(id, Surface { name, ..o }),
                _ => false,
            });
            return Ok(());
        }
        let range = |label: &str, lo: f32, hi: f32, dec: usize, unit: &str| {
            let v = parse_num(text).filter(|v| *v >= lo - 1e-6 && *v <= hi + 1e-6);
            let scale = 10f32.powi(dec as i32);
            v.map(|v| (v * scale).round() / scale).ok_or_else(|| {
                let u = if unit.is_empty() {
                    String::new()
                } else {
                    format!(" {unit}")
                };
                format!("{label}: erlaubt {} bis {}{u}", num(lo, dec), num(hi, dec))
            })
        };
        match f {
            FieldId::Dash(i, k) => {
                let Some((id, mut l)) = self.sel_lt(m) else {
                    return Ok(());
                };
                let v = if k == 0 {
                    range("Strich", 0.0, 50.0, 2, "mm")?
                } else {
                    range("Lücke", 0.1, 50.0, 2, "mm")?
                };
                let Some(d) = l.pattern.get_mut(i) else {
                    return Ok(());
                };
                let slot = if k == 0 { &mut d.len_mm } else { &mut d.gap_mm };
                if *slot != v {
                    *slot = v;
                    out.model |= cx.scene.edit_attr(|m| m.set_line_type(id, l));
                }
            }
            FieldId::Hatch(i, k) => {
                let Some((id, mut fill)) = self.sel_fill(m) else {
                    return Ok(());
                };
                let FillKind::Lines(lines) = &mut fill.kind else {
                    return Ok(());
                };
                let Some(h) = lines.get_mut(i) else {
                    return Ok(());
                };
                let (slot, v) = match k {
                    0 => (&mut h.angle_deg, range("Winkel", 0.0, 180.0, 1, "°")?),
                    1 => (&mut h.spacing_mm, range("Abstand", 0.3, 20.0, 2, "mm")?),
                    2 => {
                        let hi = h.spacing_mm;
                        (&mut h.offset_mm, range("Versatz", 0.0, hi, 2, "mm")?)
                    }
                    3 => (&mut h.dash_mm, range("Strich", 0.0, 20.0, 2, "mm")?),
                    _ => (&mut h.gap_mm, range("Lücke", 0.0, 20.0, 2, "mm")?),
                };
                if *slot != v {
                    *slot = v;
                    // Versatz bleibt innerhalb des Abstands
                    h.offset_mm = h.offset_mm.min(h.spacing_mm);
                    out.model |= cx.scene.edit_attr(|m| m.set_fill(id, fill));
                }
            }
            FieldId::Zigzag => {
                let Some((id, fill)) = self.sel_fill(m) else {
                    return Ok(());
                };
                let v = range("Periode", 0.5, 4.0, 1, "")?;
                if fill.kind != (FillKind::Zigzag { period: v }) {
                    let fill = Fill {
                        kind: FillKind::Zigzag { period: v },
                        ..fill
                    };
                    out.model |= cx.scene.edit_attr(|m| m.set_fill(id, fill));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Kontrollkästchen „Punkt danach“.
    pub(super) fn toggle_dot(&mut self, i: usize, cx: &mut Ctx, out: &mut Out) {
        let Some((id, mut l)) = self.sel_lt(cx.scene.model()) else {
            return;
        };
        if let Some(d) = l.pattern.get_mut(i) {
            d.dot = !d.dot;
            out.model |= cx.scene.edit_attr(|m| m.set_line_type(id, l));
        }
    }

    // --- Knöpfe ---------------------------------------------------------------

    /// Neuer Linientyp (Knopf „Neu“): Strichlinie 3/1 mm, freier Name.
    pub fn new_line_type(&mut self, s: &mut Scene) -> LineTypeId {
        self.tab = Tab::LineTypes;
        let mut id = None;
        s.edit_attr(|m| {
            let guid = m.new_guid();
            let name = unique(m, Tab::LineTypes, "Neuer Linientyp");
            id = Some(m.add_line_type(LineType {
                guid,
                name,
                pattern: vec![NEW_DASH],
            }));
            true
        });
        self.select_last(s.model());
        id.expect("Linientyp angelegt")
    }

    /// Neue Schraffur (Knopf „Neu“): eine Schar 45°, 2 mm, papierbezogen.
    pub fn new_fill(&mut self, s: &mut Scene) -> FillId {
        self.tab = Tab::Fills;
        let mut id = None;
        s.edit_attr(|m| {
            let guid = m.new_guid();
            let name = unique(m, Tab::Fills, "Neue Schraffur");
            id = Some(m.add_fill(Fill {
                guid,
                name,
                kind: FillKind::Lines(vec![NEW_HATCH]),
                space: FillSpace::Paper,
            }));
            true
        });
        self.select_last(s.model());
        id.expect("Schraffur angelegt")
    }

    /// Neue Oberfläche in der Farbe „Flächen ohne Baustoff“.
    fn new_surface(&mut self, s: &mut Scene, t: &Theme) -> SurfaceId {
        let c = t.env.face;
        let mut id = None;
        s.edit_attr(|m| {
            let guid = m.new_guid();
            let name = unique(m, Tab::Surfaces, "Neue Oberfläche");
            id = Some(m.add_surface(Surface {
                guid,
                name,
                color: [c.0, c.1, c.2],
                cut_color: [c.0, c.1, c.2],
            }));
            true
        });
        self.select_last(s.model());
        id.expect("Oberfläche angelegt")
    }

    /// Kopie des gewählten Eintrags („Name Kopie“).
    fn duplicate(&mut self, s: &mut Scene) {
        let m = s.model();
        let (lt, fill, surf) = (self.sel_lt(m), self.sel_fill(m), self.sel_surf(m));
        let tab = self.tab;
        s.edit_attr(|m| {
            let guid = m.new_guid();
            let copy = |n: &str| unique(m, tab, &format!("{n} Kopie"));
            match (lt, fill, surf) {
                (Some((_, l)), _, _) => {
                    let name = copy(&l.name);
                    m.add_line_type(LineType { guid, name, ..l });
                }
                (_, Some((_, f)), _) => {
                    let name = copy(&f.name);
                    m.add_fill(Fill { guid, name, ..f });
                }
                (_, _, Some((_, o))) => {
                    let name = copy(&o.name);
                    m.add_surface(Surface { guid, name, ..o });
                }
                _ => return false,
            }
            true
        });
        self.select_last(s.model());
    }

    /// Löschen: nur ohne Verwender; die Auswahl bleibt an derselben Stelle.
    fn delete(&mut self, s: &mut Scene) -> bool {
        let m = s.model();
        let (lt, fill, surf) = (self.sel_lt(m), self.sel_fill(m), self.sel_surf(m));
        s.edit_attr(|m| match (lt, fill, surf) {
            (Some((id, _)), _, _) => m.remove_line_type(id),
            (_, Some((id, _)), _) => m.remove_fill(id),
            (_, _, Some((id, _))) => m.remove_surface(id),
            _ => false,
        })
    }

    pub(super) fn attr_click(&mut self, p: Target, cx: &mut Ctx, out: &mut Out) {
        let m = cx.scene.model();
        match p {
            Target::AttrNew => {
                match self.tab {
                    Tab::LineTypes => {
                        self.new_line_type(cx.scene);
                    }
                    Tab::Fills => {
                        self.new_fill(cx.scene);
                    }
                    Tab::Surfaces => {
                        self.new_surface(cx.scene, cx.theme);
                    }
                    _ => return,
                }
                out.model = true;
            }
            Target::AttrDup => {
                self.duplicate(cx.scene);
                out.model = true;
            }
            Target::AttrDel => out.model |= self.delete(cx.scene),
            Target::RowAdd | Target::RowDel => {
                let add = p == Target::RowAdd;
                if let Some((id, mut l)) = self.sel_lt(m) {
                    let changed = if add {
                        let ok = l.pattern.len() < MAX_ROWS;
                        if ok {
                            l.pattern.push(NEW_DASH);
                        }
                        ok
                    } else {
                        l.pattern.pop().is_some()
                    };
                    if changed {
                        out.model |= cx.scene.edit_attr(|m| m.set_line_type(id, l));
                    }
                } else if let Some((id, mut f)) = self.sel_fill(m) {
                    if let FillKind::Lines(lines) = &mut f.kind {
                        let changed = if add {
                            let ok = lines.len() < MAX_ROWS;
                            if ok {
                                lines.push(NEW_HATCH);
                            }
                            ok
                        } else {
                            lines.len() > 1 && lines.pop().is_some()
                        };
                        if changed {
                            out.model |= cx.scene.edit_attr(|m| m.set_fill(id, f));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Knopf gesperrt?
    fn attr_disabled(&self, tg: Target, m: &Model) -> bool {
        let n = names(m, self.tab).len();
        match tg {
            Target::AttrDup => n == 0,
            Target::AttrDel => n == 0 || !self.sel_users(m).is_empty(),
            Target::RowAdd | Target::RowDel => {
                let len = match (self.sel_lt(m), self.sel_fill(m)) {
                    (Some((_, l)), _) => Some((l.pattern.len(), 0)),
                    (_, Some((_, f))) => match f.kind {
                        FillKind::Lines(l) => Some((l.len(), 1)),
                        _ => None,
                    },
                    _ => None,
                };
                len.is_none_or(|(len, min)| {
                    if tg == Target::RowAdd {
                        len >= MAX_ROWS
                    } else {
                        len <= min
                    }
                })
            }
            _ => false,
        }
    }

    // --- Auswahllisten --------------------------------------------------------

    pub(super) fn attr_combo(
        &self,
        id: ComboId,
        cx: &Ctx,
    ) -> (Vec<String>, Vec<Option<Canvas>>, usize, Rect) {
        let (t, w) = (&*cx.theme, cx.win);
        let s = w.scale;
        let m = cx.scene.model();
        let l = self.attr_layout(t, &w, cx.scene);
        let anchor = l
            .items
            .iter()
            .find(|(_, tg)| *tg == Target::Combo(id))
            .map_or(l.side, |(r, _)| *r);
        let fill = self.sel_fill(m).map(|x| x.1);
        let mat = self.sel_mat(m).map(|x| x.1);
        let icon_sw = |c: [u8; 3]| Some(swatch_icon(Rgba::from_rgb8(c), s, t));
        let (items, icons, sel) = match id {
            ComboId::FillKind => {
                let sel = match fill.map(|f| f.kind) {
                    Some(FillKind::Empty) => 0,
                    Some(FillKind::Solid) => 1,
                    Some(FillKind::Lines(_)) => 2,
                    _ => 3,
                };
                let items = ["Leer", "Vollfläche", "Linien", "Zickzack"];
                (items.map(String::from).to_vec(), Vec::new(), sel)
            }
            ComboId::FillSpace => {
                let sel = fill.is_some_and(|f| f.space == FillSpace::Model) as usize;
                let items = vec!["Papier".into(), "Modell (folgt später)".into()];
                (items, Vec::new(), sel)
            }
            ComboId::MatFill => {
                let list = fill_list(m);
                let sel = mat
                    .and_then(|x| list.iter().position(|(i, _)| *i == x.cut_fill))
                    .unwrap_or(0);
                let icons = list
                    .iter()
                    .map(|(i, _)| Some(fill_tile(m, t, *i, s)))
                    .collect();
                (list.into_iter().map(|(_, f)| f.name).collect(), icons, sel)
            }
            ComboId::MatFg | ComboId::MatBg => {
                let list = pens_sorted(m);
                let cur = mat.map(|x| {
                    if id == ComboId::MatFg {
                        x.cut_fg
                    } else {
                        x.cut_bg
                    }
                });
                let sel = list.iter().position(|p| Some(p.0) == cur).unwrap_or(0);
                let icons = list.iter().map(|(_, p)| icon_sw(p.color)).collect();
                let items = list.iter().map(|(_, p)| pen_label(p)).collect();
                (items, icons, sel)
            }
            ComboId::MatSurface => {
                let list = surf_list(m);
                let sel = mat
                    .and_then(|x| list.iter().position(|(i, _)| *i == x.surface))
                    .unwrap_or(0);
                let icons = list.iter().map(|(_, o)| icon_sw(o.color)).collect();
                (list.into_iter().map(|(_, o)| o.name).collect(), icons, sel)
            }
            ComboId::PenWidth | ComboId::Scheme => (Vec::new(), Vec::new(), 0),
        };
        (items, icons, sel, anchor)
    }

    pub(super) fn attr_choose(&mut self, id: ComboId, i: usize, cx: &mut Ctx, out: &mut Out) {
        let m = cx.scene.model();
        match id {
            ComboId::FillKind => {
                let Some((fid, f)) = self.sel_fill(m) else {
                    return;
                };
                let kind = match i {
                    0 => FillKind::Empty,
                    1 => FillKind::Solid,
                    2 => match f.kind {
                        FillKind::Lines(l) => FillKind::Lines(l),
                        _ => FillKind::Lines(vec![NEW_HATCH]),
                    },
                    _ => match f.kind {
                        FillKind::Zigzag { period } => FillKind::Zigzag { period },
                        _ => FillKind::Zigzag { period: 1.0 },
                    },
                };
                let f = Fill { kind, ..f };
                out.model |= cx
                    .scene
                    .edit_attr(|m| m.attr().fill(fid) != Some(&f) && m.set_fill(fid, f));
            }
            ComboId::FillSpace => {
                // Modellbezogen zeichnet der Shader noch nicht: bleibt Papier
                if i == 1 {
                    self.error = Some("Modellbezogene Schraffuren folgen später".into());
                }
            }
            ComboId::MatFill | ComboId::MatFg | ComboId::MatBg | ComboId::MatSurface => {
                let Some((mid, mat)) = self.sel_mat(m) else {
                    return;
                };
                let mut d = mat.display();
                match id {
                    ComboId::MatFill => {
                        let Some((f, _)) = fill_list(m).get(i).cloned() else {
                            return;
                        };
                        d.cut_fill = f;
                    }
                    ComboId::MatSurface => {
                        let Some((o, _)) = surf_list(m).get(i).cloned() else {
                            return;
                        };
                        d.surface = o;
                    }
                    _ => {
                        let Some((p, _)) = pens_sorted(m).get(i).cloned() else {
                            return;
                        };
                        if id == ComboId::MatFg {
                            d.cut_fg = p;
                        } else {
                            d.cut_bg = p;
                        }
                    }
                }
                if d != mat.display() {
                    out.model |= cx.scene.edit_attr(|m| m.set_material_display(mid, d));
                }
            }
            ComboId::PenWidth | ComboId::Scheme => {}
        }
    }

    // --- Zeichnen -------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_attr(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sc: &Scene,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let m = sc.model();
        let l = self.attr_layout(t, w, sc);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (font, small) = (t.size.font * s, t.size.font_small * s);
        // Kopf der Liste
        let lr = at(l.list);
        let head_y = lr.y + 18.0 * s;
        label(c, bold, "Name", small, lr.x + 8.0 * s, head_y, u.text_dim);
        let right = match self.tab {
            Tab::Surfaces => "Farbe",
            Tab::Materials => "Schnitt",
            _ => "Muster",
        };
        let tx = at(Rect::new(l.thumb_x, 0.0, 0.0, 0.0)).x;
        label(c, bold, right, small, tx, head_y, u.text_dim);
        let line = s.round().max(1.0);
        c.fill_rect(lr.x, at(l.body).y - line, lr.w, line, u.border);
        // Zeilen in eigenem Bild (Bildlauf schneidet ab)
        let body = at(l.body);
        let mut bc = Canvas::new(body.w.ceil() as usize, body.h.ceil() as usize);
        let sel = self.sel_index(m);
        let list = names(m, self.tab);
        let thumbs = self.thumbs(m, t, s);
        for (i, (name, r)) in list.iter().zip(&l.rows).enumerate() {
            let r = Rect::new(r.x - l.body.x, r.y - l.body.y, r.w, r.h);
            if r.y + r.h < 0.0 || r.y > body.h {
                continue;
            }
            if sel == Some(i) {
                let b = line;
                bc.fill_rect(r.x, r.y, r.w, r.h, u.accent);
                bc.fill_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, u.pressed);
            } else if self.hover == Some(Target::Row(i)) {
                bc.fill_rect(r.x, r.y, r.w, r.h, u.hover);
            }
            let base = r.y + (r.h + regular.map_or(font * 0.7, |f| f.cap_height(font))) * 0.5;
            let thumb_x = l.thumb_x - l.body.x;
            let name = widgets::ellipsize(regular, name, font, thumb_x - 20.0 * s);
            label(&mut bc, regular, &name, font, 8.0 * s, base, u.text);
            if let Some(img) = thumbs.get(i) {
                let y = r.y + (r.h - img.height as f32) * 0.5;
                bc.blit(img, thumb_x as i32, y as i32);
            }
        }
        c.blit(&bc, body.x as i32, body.y as i32);
        if let Some(b) = l.bar {
            let total = l.content_h.max(1.0);
            let hover = self.hover == Some(Target::Bar(BarId::List))
                || matches!(self.drag, Some(Drag::Bar(BarId::List, _)));
            widgets::scrollbar(c, at(b), l.scroll / total, l.body.h / total, hover, s, t);
        }
        // Knöpfe und Hinweise unter der Liste
        if let Some(b) = l.buttons {
            for (r, (tg, text)) in b.iter().zip([
                (Target::AttrNew, "Neu"),
                (Target::AttrDup, "Duplizieren"),
                (Target::AttrDel, "Löschen"),
            ]) {
                let disabled = self.attr_disabled(tg, m);
                let st = ButtonState {
                    hover: self.hover == Some(tg) && !disabled,
                    pressed: self.pressed == Some(tg) && self.hover == Some(tg) && !disabled,
                    active: false,
                    disabled,
                };
                widgets::button(c, fonts, at(*r), text, st, s, t);
            }
        }
        let hy = at(Rect::new(0.0, l.hint_y, 0.0, 0.0)).y;
        let users = self.sel_users(m);
        let mut hints: Vec<String> = Vec::new();
        if !users.is_empty() {
            hints.push("Löschen gesperrt: wird verwendet von".into());
            hints.push(users.join(", "));
        }
        if self.tab == Tab::Fills {
            hints.push("Farben kommen vom Baustoff".into());
            hints.push("(Reiter „Baustoffe“).".into());
        }
        for (k, h) in hints.iter().enumerate() {
            let text = widgets::ellipsize(regular, h, small, lr.w);
            label(
                c,
                regular,
                &text,
                small,
                lr.x,
                hy + k as f32 * 17.0 * s,
                u.text_dim,
            );
        }
        // Bearbeiten
        for tx in &l.texts {
            let r = at(Rect::new(tx.x, tx.y, 0.0, 0.0));
            // nie über den rechten Rand des Bearbeitungsbereichs hinaus
            let max = (l.side.x + l.side.w - tx.x).max(0.0);
            let fit = |f, px| widgets::ellipsize(f, &tx.text, px, max);
            match tx.kind {
                TextKind::Heading => {
                    let px = t.size.font_title * s;
                    label(c, bold, &fit(bold, px), px, r.x, r.y, u.text)
                }
                TextKind::Label => {
                    label(c, regular, &fit(regular, font), font, r.x, r.y, u.text_dim)
                }
                TextKind::Dim => label(
                    c,
                    regular,
                    &fit(regular, small),
                    small,
                    r.x,
                    r.y,
                    u.field_unit,
                ),
                TextKind::Group(_) | TextKind::Title => {
                    label(c, bold, &fit(bold, font), font, r.x, r.y, u.text)
                }
                // je Eintrag eine oder mehrere Zeilen, bis zum unteren Rand
                TextKind::Wrap => {
                    let lines = tx
                        .text
                        .lines()
                        .flat_map(|e| widgets::wrap(regular, e, small, max));
                    let bottom = at(Rect::new(0.0, l.side.y + l.side.h, 0.0, 0.0)).y;
                    for (k, line) in lines.enumerate() {
                        let y = r.y + k as f32 * WRAP_LINE * s;
                        if y > bottom {
                            break;
                        }
                        label(c, regular, &line, small, r.x, y, u.field_unit);
                    }
                }
            }
        }
        for (r, text) in &l.readonly {
            widgets::field_readonly(c, fonts, at(*r), text, s, t);
        }
        for (r, tg) in &l.items {
            let rr = at(*r);
            let hover = self.hover == Some(*tg);
            match *tg {
                Target::Field(f) => {
                    let v = self.attr_field_value(f, sc);
                    let st = self.field_state(f, &v, attr_unit(f));
                    if f == FieldId::Name {
                        widgets::text_field(c, fonts, rr, &st, s, t);
                    } else {
                        widgets::field(c, fonts, rr, &st, s, t);
                    }
                }
                Target::Combo(id) => {
                    let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
                    let (text, icon) = self.combo_shown(id, m, t, s);
                    widgets::combo_icon(c, fonts, rr, &text, icon.as_ref(), hover, open, s, t);
                }
                Target::Swatch(ct) => {
                    let col = self.color_of(ct, sc, t);
                    widgets::swatch(c, rr, col, hover, s, t);
                }
                Target::Check(i) => {
                    let on = self
                        .sel_lt(m)
                        .and_then(|(_, l)| l.pattern.get(i).map(|d| d.dot))
                        .unwrap_or(false);
                    widgets::checkbox(c, rr, on, hover, s, t);
                }
                Target::RowAdd | Target::RowDel => {
                    let disabled = self.attr_disabled(*tg, m);
                    let what = if self.tab == Tab::Fills {
                        "Schar"
                    } else {
                        "Zeile"
                    };
                    let text = if *tg == Target::RowAdd {
                        format!("{what} hinzufügen")
                    } else {
                        format!("{what} entfernen")
                    };
                    let st = ButtonState {
                        hover: hover && !disabled,
                        pressed: self.pressed == Some(*tg) && hover && !disabled,
                        active: false,
                        disabled,
                    };
                    widgets::button(c, fonts, rr, &text, st, s, t);
                }
                _ => {}
            }
        }
        if let Some(pr) = l.preview {
            let pr = at(pr);
            match self.tab {
                Tab::LineTypes => self.paint_lt_preview(c, pr, m, t, fonts, s),
                Tab::Fills => {
                    if let Some((id, _)) = self.sel_fill(m) {
                        paint_fill_preview(c, pr, m, t, s, fill_display(m, id));
                    }
                }
                Tab::Surfaces => {
                    if let Some((_, o)) = self.sel_surf(m) {
                        paint_cube(c, pr, &o, t, s);
                    }
                }
                Tab::Materials => {
                    if let Some((_, mat)) = self.sel_mat(m) {
                        let cube_w = (pr.h * 0.9).min(pr.w * 0.3);
                        let wall = Rect::new(pr.x, pr.y, pr.w - cube_w - 12.0 * s, pr.h);
                        paint_fill_preview(c, wall, m, t, s, mat.display());
                        if let Some(o) = m.attr().surface(mat.surface) {
                            let cr = Rect::new(pr.x + pr.w - cube_w, pr.y, cube_w, pr.h);
                            paint_cube(c, cr, o, t, s);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Text und Bildchen einer geschlossenen Auswahlliste.
    fn combo_shown(&self, id: ComboId, m: &Model, t: &Theme, s: f32) -> (String, Option<Canvas>) {
        let fill = self.sel_fill(m).map(|x| x.1);
        let mat = self.sel_mat(m).map(|x| x.1);
        let a = m.attr();
        let sw = |c: [u8; 3]| Some(swatch_icon(Rgba::from_rgb8(c), s, t));
        match id {
            ComboId::FillKind => {
                let text = match fill.map(|f| f.kind) {
                    Some(FillKind::Empty) => "Leer",
                    Some(FillKind::Solid) => "Vollfläche",
                    Some(FillKind::Lines(_)) => "Linien",
                    _ => "Zickzack",
                };
                (text.into(), None)
            }
            ComboId::FillSpace => ("Papier".into(), None),
            ComboId::MatFill => mat.map_or((String::new(), None), |x| {
                let name = a.fill(x.cut_fill).map_or("–".into(), |f| f.name.clone());
                (name, Some(fill_tile(m, t, x.cut_fill, s)))
            }),
            ComboId::MatFg | ComboId::MatBg => mat.map_or((String::new(), None), |x| {
                let id = if id == ComboId::MatFg {
                    x.cut_fg
                } else {
                    x.cut_bg
                };
                a.pen(id)
                    .map_or(("–".into(), None), |p| (pen_label(p), sw(p.color)))
            }),
            ComboId::MatSurface => mat.map_or((String::new(), None), |x| {
                a.surface(x.surface)
                    .map_or(("–".into(), None), |o| (o.name.clone(), sw(o.color)))
            }),
            ComboId::PenWidth | ComboId::Scheme => (String::new(), None),
        }
    }

    /// Vorschaubilder der Listenzeilen.
    fn thumbs(&self, m: &Model, t: &Theme, s: f32) -> Vec<Canvas> {
        let (tw, th) = (t.size.list_thumb_w * s, t.size.list_thumb_h * s);
        match self.tab {
            Tab::LineTypes => lt_list(m)
                .iter()
                .map(|(_, l)| {
                    let w = 0.35 * t.px_per_mm * s;
                    line_strip(m, t, &l.pattern, LT_THUMB_W * s, th, w, s)
                })
                .collect(),
            Tab::Fills => fill_list(m)
                .iter()
                .map(|(id, _)| fill_tile(m, t, *id, s))
                .collect(),
            Tab::Surfaces => surf_list(m)
                .iter()
                .map(|(_, o)| swatch_icon(Rgba::from_rgb8(o.color), s, t))
                .collect(),
            Tab::Materials => mat_list(m)
                .iter()
                .map(|(_, x)| tile(m, t, &x.display(), tw, th, s))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn paint_lt_preview(
        &self,
        c: &mut Canvas,
        r: Rect,
        m: &Model,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
    ) {
        let Some((_, lt)) = self.sel_lt(m) else {
            return;
        };
        let regular = fonts.regular.as_ref();
        let small = t.size.font_small * s;
        let row = r.h / PREVIEW_WIDTHS.len() as f32;
        for (i, mm) in PREVIEW_WIDTHS.iter().enumerate() {
            let y = r.y + i as f32 * row;
            let w = (mm * t.px_per_mm * s).max(0.6);
            let img = line_strip(m, t, &lt.pattern, r.w, row - 6.0 * s, w, s);
            c.blit(&img, r.x as i32, y as i32);
            label(
                c,
                regular,
                &format!("{} mm", num(*mm, 2)),
                small,
                r.x + r.w + 10.0 * s,
                y + row * 0.5,
                t.ui.field_unit,
            );
        }
    }
}

// --- Bildchen und Vorschauen ------------------------------------------------------

/// „Nr. – Name“ eines Stifts in Auswahllisten.
fn pen_label(p: &Pen) -> String {
    format!("{} – {} {}", p.number, p.name, num(p.width_mm, 2))
}

/// Farbfeld als Bildchen in Auswahllisten und Listen.
fn swatch_icon(col: Rgba, s: f32, t: &Theme) -> Canvas {
    let (w, h) = ((t.size.swatch_w * s).round(), (t.size.swatch_h * s).round());
    let mut c = Canvas::new(w as usize, h as usize);
    widgets::swatch(&mut c, Rect::new(0.0, 0.0, w, h), col, false, s, t);
    c
}

/// Strichmuster in Bildpunkten bei Skalierung `s`.
fn pattern_px(pattern: &[Dash], px_per_mm: f32, s: f32) -> DashPattern {
    let mut p = SOLID;
    for (slot, d) in p.iter_mut().zip(pattern) {
        *slot = [
            d.len_mm * px_per_mm * s,
            d.gap_mm * px_per_mm * s,
            d.dot as u8 as f32,
            0.0,
        ];
    }
    p
}

/// Waagerechte Linie im Muster auf Papiergrund (Liste und Vorschau).
fn line_strip(m: &Model, t: &Theme, pattern: &[Dash], w: f32, h: f32, lw: f32, s: f32) -> Canvas {
    let paper = Rgba::from_rgb8(m.attr().display().paper);
    let ink = t.env.edge;
    let (cw, ch) = (w.round().max(1.0) as usize, h.round().max(1.0) as usize);
    let p = pattern_px(pattern, t.px_per_mm, s);
    let x0 = 6.0 * s;
    let len = w - 2.0 * x0;
    let half = lw * 0.5;
    let mid = ch as f32 * 0.5;
    Canvas::from_fn(cw, ch, |x, y| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let d = fx - x0;
        // Abdeckung quer zur Linie (weicher Rand)
        let cov = (half + 0.5 - (fy - mid).abs()).clamp(0.0, 1.0);
        if (0.0..=len).contains(&d) && cov > 0.0 && sk_render::dash_ink(d, len, &p, lw) {
            mix(paper, ink, cov)
        } else {
            paper
        }
    })
}

fn mix(a: Rgba, b: Rgba, f: f32) -> Rgba {
    let m = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
    Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), 255)
}

/// Darstellungsverweise für die Vorschau einer Schraffur ohne Baustoff:
/// Stift 4 (Schraffur) auf Stift 5 (Grund).
fn fill_display(m: &Model, fill: FillId) -> MaterialDisplay {
    let d = m.attr().display();
    let any = d.drawing[0].pen;
    let surface = m
        .attr()
        .surfaces()
        .iter()
        .next()
        .map(|(i, _)| i)
        .or_else(|| m.materials().iter().next().map(|(_, x)| x.surface));
    MaterialDisplay {
        cut_fill: fill,
        cut_fg: pen_by_number(m, 4).unwrap_or(any),
        cut_bg: pen_by_number(m, 5).unwrap_or(any),
        surface: surface.unwrap_or_else(|| {
            m.materials()
                .iter()
                .next()
                .map(|(_, x)| x.surface)
                .expect("Oberfläche")
        }),
    }
}

/// Kachel einer Schraffur in der Liste (Stift 4 auf Stift 5).
fn fill_tile(m: &Model, t: &Theme, fill: FillId, s: f32) -> Canvas {
    let (w, h) = (t.size.list_thumb_w * s, t.size.list_thumb_h * s);
    tile(m, t, &fill_display(m, fill), w, h, s)
}

/// Schnittfläche nach der Formel des Shaders, mit Rand in Stift „Schnitt“.
fn hatch_image(m: &Model, t: &Theme, d: &MaterialDisplay, w: usize, h: usize, s: f32) -> Canvas {
    let look = mat_look(m, t, d, &mut Vec::new());
    let rows = look_rows(&look, s);
    let hf = h as f32;
    // Zickzack: die Kachel ist eine Schicht, quer 0..1
    let th = hf.max(1.0);
    Canvas::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 + 0.5, hf - y as f32 - 0.5);
        let c = sk_render::fill_color(&rows, fx, fy, [fx / th, fy / th], [1.0 / th, 1.0 / th]);
        Rgba::from_f32([c[0], c[1], c[2], 1.0])
    })
}

/// Kleine Kachel mit dünnem Rand (Liste, Auswahlliste).
fn tile(m: &Model, t: &Theme, d: &MaterialDisplay, w: f32, h: f32, s: f32) -> Canvas {
    let (wi, hi) = (w.round().max(2.0) as usize, h.round().max(2.0) as usize);
    let mut c = Canvas::new(wi, hi);
    c.fill_rect(0.0, 0.0, wi as f32, hi as f32, t.ui.border);
    let b = (s.round().max(1.0) as usize).min(wi.min(hi) / 2 - 1);
    let inner = hatch_image(m, t, d, wi - 2 * b, hi - 2 * b, s);
    c.blit(&inner, b as i32, b as i32);
    c
}

/// Vorschau einer Schnittfläche: Rechteck mit Umriss in Stift „Schnitt“
/// (Kantenart `CUT`), innen die Schraffur in den Baustofffarben.
fn paint_fill_preview(c: &mut Canvas, r: Rect, m: &Model, t: &Theme, s: f32, d: MaterialDisplay) {
    let a = m.attr();
    let cut = a.pen(a.display().drawing[sk_model::edge_kind::CUT as usize].pen);
    let lw = cut
        .map_or(1.0, |p| (p.width_mm * t.px_per_mm * s).max(1.0))
        .round();
    let ink = cut.map_or(t.env.edge, |p| Rgba::from_rgb8(p.color));
    c.fill_rect(r.x, r.y, r.w, r.h, ink);
    let (w, h) = ((r.w - 2.0 * lw).max(1.0), (r.h - 2.0 * lw).max(1.0));
    let img = hatch_image(m, t, &d, w as usize, h as usize, s);
    c.blit(&img, (r.x + lw) as i32, (r.y + lw) as i32);
}

/// Würfel schräg von oben vor Himmel und Boden. Die Seiten werden wie in der
/// 3D-Ansicht beleuchtet (Lichtrichtung und Umgebungshelligkeit des
/// Renderers); die vordere obere Ecke ist in der Schnittfarbe aufgeschnitten.
fn paint_cube(c: &mut Canvas, r: Rect, o: &Surface, t: &Theme, s: f32) {
    let st = crate::style(&t.env);
    let shade = |col: [u8; 3], n: [f32; 3]| {
        let l = st.light;
        let d = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]).max(0.0);
        let k = st.ambient + (1.0 - st.ambient) * d;
        let f = |v: u8| ((v as f32 / 255.0 * k).clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba(f(col[0]), f(col[1]), f(col[2]), 255)
    };
    // Himmel (oben) und Boden
    let sky_top = t.env.sky.last().map_or(t.ui.bg, |x| x.1);
    let sky_low = t.env.sky.first().map_or(t.ui.bg, |x| x.1);
    let horizon = r.y + r.h * 0.62;
    let bg = Canvas::from_fn(r.w as usize, r.h as usize, |_, y| {
        let fy = r.y + y as f32;
        if fy >= horizon {
            t.env.ground
        } else {
            mix(
                sky_top,
                sky_low,
                ((fy - r.y) / (horizon - r.y)).clamp(0.0, 1.0),
            )
        }
    });
    c.blit(&bg, r.x as i32, r.y as i32);
    // Blick von Südwesten wie die Startansicht: links die Westseite
    // (−x), rechts die Südseite (−y), oben das Dach (+z)
    let a = (r.h * 0.34).min(r.w * 0.36);
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.52);
    let k = 0.866 * a;
    let top = (cx, cy - a);
    let left = (cx - k, cy - a * 0.5);
    let mid = (cx, cy);
    let right = (cx + k, cy - a * 0.5);
    let down = (cx, cy + a);
    let left_b = (cx - k, cy + a * 0.5);
    let right_b = (cx + k, cy + a * 0.5);
    let poly = |c: &mut Canvas, pts: &[(f32, f32)], col: Rgba| {
        let mut p = Path::new();
        p.move_to(pts[0].0, pts[0].1);
        for q in &pts[1..] {
            p.line_to(q.0, q.1);
        }
        p.close();
        c.fill(&p, col);
    };
    let (n_top, n_west, n_south) = ([0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]);
    poly(c, &[top, right, mid, left], shade(o.color, n_top));
    poly(c, &[left, mid, down, left_b], shade(o.color, n_west));
    poly(c, &[mid, right, right_b, down], shade(o.color, n_south));
    // Aufgeschnittene Ecke: obere Hälfte der Südseite nahe der Ecke
    let lerp =
        |p: (f32, f32), q: (f32, f32), f: f32| (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
    let h = 0.5;
    let p0 = lerp(mid, right, h);
    let p1 = right;
    let p2 = lerp(right, right_b, h);
    let p3 = lerp(p0, lerp(down, right_b, h), h);
    poly(c, &[p0, p1, p2, p3], shade(o.cut_color, n_south));
    let lerp_top = lerp(top, right, h);
    poly(
        c,
        &[lerp_top, right, p0, lerp(mid, top, 1.0 - h)],
        shade(o.cut_color, n_top),
    );
    // Kanten
    let ink = t.env.edge;
    let lw = (0.9 * s).max(0.8);
    let mut p = Path::new();
    for (a, b) in [
        (top, right),
        (right, right_b),
        (right_b, down),
        (down, left_b),
        (left_b, left),
        (left, top),
        (left, mid),
        (mid, right),
        (mid, down),
    ] {
        p.segment(a, b, lw);
    }
    c.fill(&p, ink);
}
