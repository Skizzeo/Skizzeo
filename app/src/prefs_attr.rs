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
use crate::attr_pick::{
    fill_display, has_joints, line_strip, paint_cube, paint_elevation_tile, paint_fill_preview,
    TileKey, Tiles,
};
use sk_model::proctex::{self, Bond, Pattern, BRICK_FORMATS};
use sk_model::{Dash, Fill, FillId, FillKind, FillSpace, HatchLine, LineType, LineTypeId, Surface};

/// Breite der Liste links (dip).
const LIST_W: f32 = 250.0;
/// Zeilenabstand umbrochener Einträge (dip).
const WRAP_LINE: f32 = 18.0;
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

/// Steinformat zu Länge und Höhe ([`BRICK_FORMATS`]).
fn format_index(len: f32, h: f32) -> Option<usize> {
    BRICK_FORMATS.iter().position(|f| f.1 == len && f.2 == h)
}

/// Musterarten in der Auswahl „Art“ (Paket 6, 7a).
const KIND_NAMES: [&str; 7] = [
    "ohne",
    "Mauerwerk",
    "Putz",
    "Sichtbeton",
    "Holzschalung",
    "Platten",
    "Naturstein",
];

/// Vorlage, mit der eine neue Art aus Paket 7 beginnt.
const KIND_PRESETS: [&str; 4] = [
    "Sichtbeton mittelgrau",
    "Holzschalung Lärche",
    "Betonplatten 40 × 40",
    "Naturstein",
];

/// Verbände in der Auswahl „Verband“.
const BONDS: [Bond; 5] = [
    Bond::Half,
    Bond::Third,
    Bond::Wild,
    Bond::Block,
    Bond::Cross,
];

/// Einträge und Wahl der Musterlisten (Art, Steinformat, Verband).
fn pattern_combo(id: ComboId, p: Option<&Pattern>, free: bool) -> (Vec<String>, usize) {
    match id {
        ComboId::PatKind => {
            let mut items: Vec<String> = KIND_NAMES.iter().map(|k| k.to_string()).collect();
            let sel = match p {
                None => 0,
                Some(Pattern::Masonry { .. }) => 1,
                Some(Pattern::Plaster { .. }) => 2,
                Some(Pattern::Concrete { .. }) => 3,
                Some(Pattern::Timber { .. }) => 4,
                Some(Pattern::Tiles { .. }) => 5,
                Some(Pattern::Stone { .. }) => 6,
                Some(Pattern::Foreign(_)) => {
                    items.push("unbekannt (neuere Version)".into());
                    KIND_NAMES.len()
                }
            };
            (items, sel)
        }
        ComboId::PatFormat => {
            let mut items: Vec<String> = BRICK_FORMATS
                .iter()
                .map(|f| format!("{} {} × {}", f.0, f.1, f.2))
                .collect();
            items.push("frei".into());
            let sel = match p {
                Some(Pattern::Masonry { len, h, .. }) if !free => {
                    format_index(*len, *h).unwrap_or(BRICK_FORMATS.len())
                }
                _ => BRICK_FORMATS.len(),
            };
            (items, sel)
        }
        ComboId::PatAnchors => {
            let on = matches!(p, Some(Pattern::Concrete { anchors: true, .. }));
            (vec!["ohne".into(), "mit".into()], on as usize)
        }
        ComboId::PatWoodDir => {
            let lying = matches!(
                p,
                Some(Pattern::Timber {
                    vertical: false,
                    ..
                })
            );
            (
                vec!["senkrecht".into(), "waagerecht".into()],
                lying as usize,
            )
        }
        ComboId::PatGrid => {
            let half = matches!(p, Some(Pattern::Tiles { half: true, .. }));
            (
                vec!["Kreuzfuge".into(), "Halbversatz".into()],
                half as usize,
            )
        }
        _ => {
            let items = [
                "Läufer halbsteinig",
                "Läufer drittelsteinig",
                "wild",
                "Blockverband",
                "Kreuzverband",
            ];
            let sel = match p {
                Some(Pattern::Masonry { bond, .. }) => {
                    BONDS.iter().position(|b| b == bond).unwrap_or(2)
                }
                _ => 2,
            };
            (items.map(String::from).to_vec(), sel)
        }
    }
}

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

/// Namen der Tabelle eines Reiters.
fn names(m: &Model, tab: Tab) -> Vec<String> {
    let a = m.attr();
    match tab {
        Tab::Pens => a.pens().iter().map(|(_, p)| p.name.clone()).collect(),
        Tab::LineTypes => a.line_types().iter().map(|(_, l)| l.name.clone()).collect(),
        Tab::Fills => a.fills().iter().map(|(_, f)| f.name.clone()).collect(),
        Tab::Surfaces => a.surfaces().iter().map(|(_, o)| o.name.clone()).collect(),
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

/// Einheit eines Felds der Attributreiter.
pub(super) fn attr_unit(f: FieldId) -> &'static str {
    match f {
        FieldId::Dash(..) | FieldId::Hatch(_, 1..) => "mm",
        FieldId::Hatch(_, 0) => "°",
        FieldId::PatLen | FieldId::PatH | FieldId::PatJoint | FieldId::PatGrain => "mm",
        FieldId::PatShare(_) | FieldId::PatSpread | FieldId::PatFlame | FieldId::PatRelief => "%",
        FieldId::PatNum(key) => num_spec(key).1,
        _ => "",
    }
}

/// Beschriftung, Einheit und Nachkommastellen der Regler aus Paket 7 nach
/// ihrem Schlüssel ([`proctex::limits`]); gleiche Schlüssel verschiedener
/// Arten heißen gleich.
fn num_spec(key: &str) -> (&'static str, &'static str, usize) {
    match key {
        "w" => ("Tafelbreite", "mm", 0),
        "h" => ("Tafelhöhe", "mm", 0),
        "joint" => ("Fuge", "mm", 1),
        "cloud" => ("Wolkigkeit", "%", 0),
        "pores" => ("Lunker", "%", 1),
        "board" => ("Brettbreite", "mm", 0),
        "grain" => ("Maserung", "%", 0),
        "len" => ("Länge", "mm", 0),
        "wid" => ("Breite", "mm", 0),
        "spread" => ("Streuung", "%", 0),
        "size" => ("Steingröße", "mm", 0),
        "irr" => ("Unregelmäßigkeit", "%", 0),
        _ => ("", "", 0),
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
    /// Neu, Duplizieren, Löschen.
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
    /// Zweite Vorschau (Oberflächen mit Muster: Ansichtskachel).
    pub preview2: Option<Rect>,
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

    pub(super) fn sel_surf(&self, m: &Model) -> Option<(SurfaceId, Surface)> {
        (self.tab == Tab::Surfaces)
            .then(|| self.sel_index(m).and_then(|i| surf_list(m).get(i).cloned()))?
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
        // Fenster „Muster“: seine Regler rechts (Paket 7b)
        if self.pw.is_some() {
            return self.pw_controls(t, w, sc);
        }
        let c = self.content(t, w);
        let s = w.scale;
        let m = sc.model();
        let list_w = (LIST_W * s).min(c.w * 0.42).round();
        let list = Rect::new(c.x, c.y, list_w, c.h);
        let row = t.size.table_row * s;
        // Knöpfe + zwei Hinweiszeilen (Schraffuren: vier)
        let foot = match self.tab {
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
            Tab::LineTypes => crate::attr_pick::LT_THUMB_W,
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
            buttons: Some([b(0.0, 66.0), b(72.0, 110.0), b(188.0, 80.0)]),
            hint_y: by + 30.0 * s + 22.0 * s,
            thumb_x,
            side,
            items: Vec::new(),
            texts: Vec::new(),
            readonly: Vec::new(),
            preview: None,
            preview2: None,
        };
        match self.tab {
            Tab::LineTypes => self.lt_side(&mut l, t, s, m),
            Tab::Fills => self.fill_side(&mut l, t, s, m),
            Tab::Surfaces => self.surf_side(&mut l, t, s, m),
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
        y += 4.0 * s;
        self.pattern_side(l, t, s, m, (id, &o), y, false);
    }

    /// Abschnitt „Muster“ einer Oberfläche (Paket 6, soll-p6-5): links die
    /// Regler ab `y`, rechts die Vorschau (Würfel, Ansichtskachel); passt
    /// die Spalte nicht daneben, steht die Vorschau darunter.
    /// `window`: rechte Spalte des Fensters „Muster“ (nur die Regler, die
    /// Art als Liste über die ganze Breite, ohne Vorschau und Hinweise).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_side(
        &self,
        l: &mut AttrLayout,
        t: &Theme,
        s: f32,
        m: &Model,
        (id, o): (SurfaceId, &Surface),
        mut y: f32,
        window: bool,
    ) {
        let (x, sw) = (l.side.x, l.side.w);
        let fh = t.size.field_height * s;
        let bottom = l.side.y + l.side.h;
        let gap = 12.0 * s;
        // Regler ab vx (kurze Beschriftungen), Listen cw breit
        let vx = x + if window { 90.0 } else { 104.0 } * s;
        let cw = (sw - (vx - x)).min(150.0 * s);
        let free = sw - (vx - x) - cw - gap;
        let right = free >= 90.0 * s;
        let pw = if right {
            free.min(120.0 * s)
        } else {
            110.0 * s
        };
        let short = 66.0 * s;
        let step = fh + 5.0 * s;
        let top = y;
        let row = |l: &mut AttrLayout, y: &mut f32, label: &str, w: f32, tg: Target| {
            l.texts.push(UiText::label(x, *y + 18.0 * s, label));
            l.items.push((Rect::new(vx, *y, w, fh), tg));
            *y += step;
        };
        if window {
            let r = Rect::new(x, y, sw.min(200.0 * s), fh);
            l.items.push((r, Target::Combo(ComboId::PatKind)));
            y += step + 8.0 * s;
        } else {
            l.texts.push(UiText::group_title(x, y + 14.0 * s, "Muster"));
            y += 24.0 * s;
            row(l, &mut y, "Art", cw, Target::Combo(ComboId::PatKind));
        }
        // „Zufall“ in eigener Zeile unter den Werten (soll-p6-5)
        let random = |l: &mut AttrLayout, y: &mut f32| {
            let r = Rect::new(vx, *y, (cw * 0.6).max(60.0 * s), fh);
            l.items.push((r, Target::PatRandom));
            *y += step;
        };
        match &o.pattern {
            Some(Pattern::Masonry { len, h, .. }) => {
                row(
                    l,
                    &mut y,
                    "Steinformat",
                    cw,
                    Target::Combo(ComboId::PatFormat),
                );
                if self.pat_free || format_index(*len, *h).is_none() {
                    l.texts.push(UiText::label(x, y + 18.0 * s, "Länge, Höhe"));
                    l.items
                        .push((Rect::new(vx, y, short, fh), Target::Field(FieldId::PatLen)));
                    l.items.push((
                        Rect::new(vx + short + 8.0 * s, y, short, fh),
                        Target::Field(FieldId::PatH),
                    ));
                    y += step;
                }
                row(l, &mut y, "Fuge", short, Target::Field(FieldId::PatJoint));
                row(l, &mut y, "Verband", cw, Target::Combo(ComboId::PatBond));
                let (ww, wh) = (t.size.swatch_w * s, t.size.swatch_h * s);
                let fw = ((cw - 2.0 * 6.0 * s) / 3.0).floor();
                // Farbe mit Anteil daneben (soll-p6-5, Einstellungen (aj))
                let share_w = (fw - ww.min(fw) - 4.0 * s).max(30.0 * s);
                l.texts.push(UiText::label(x, y + 18.0 * s, "Läuferfarben"));
                for k in 0..3 {
                    let cx = vx + k as f32 * (fw + 6.0 * s);
                    l.items.push((
                        Rect::new(cx, y + (fh - wh) * 0.5, ww.min(fw), wh),
                        Target::Swatch(ColorTarget::PatStone(id, k)),
                    ));
                    let r = Rect::new(cx + fw - share_w, y, share_w, fh);
                    if k < 2 {
                        l.items.push((r, Target::Field(FieldId::PatShare(k))));
                    } else {
                        let rest = self.pat_value(FieldId::PatShare(2), m);
                        l.readonly.push((r, format!("{rest} %")));
                    }
                }
                y += step;
                if let Some(Pattern::Masonry { hpal: Some(hp), .. }) = &o.pattern {
                    l.texts.push(UiText::label(x, y + 18.0 * s, "Kopffarben"));
                    for (k, (_, a)) in hp.iter().enumerate().filter(|(_, c)| c.1 > 0.0) {
                        let cx = vx + k as f32 * (fw + 6.0 * s);
                        l.items.push((
                            Rect::new(cx, y + (fh - wh) * 0.5, ww.min(fw), wh),
                            Target::Swatch(ColorTarget::PatHead(id, k)),
                        ));
                        l.readonly.push((
                            Rect::new(cx + fw - share_w, y, share_w, fh),
                            format!("{} %", num_short(*a)),
                        ));
                    }
                    y += step;
                }
                l.texts.push(UiText::label(x, y + 18.0 * s, "Fugenfarbe"));
                l.items.push((
                    Rect::new(vx, y + (fh - wh) * 0.5, ww, wh),
                    Target::Swatch(ColorTarget::PatJoint(id)),
                ));
                y += step;
                row(
                    l,
                    &mut y,
                    "Flammung",
                    short,
                    Target::Field(FieldId::PatFlame),
                );
                row(
                    l,
                    &mut y,
                    "Relief",
                    short,
                    Target::Field(FieldId::PatRelief),
                );
                row(
                    l,
                    &mut y,
                    "Streuung",
                    short,
                    Target::Field(FieldId::PatSpread),
                );
                random(l, &mut y);
            }
            Some(Pattern::Plaster { .. }) => {
                row(
                    l,
                    &mut y,
                    "Körnung",
                    short,
                    Target::Field(FieldId::PatGrain),
                );
                row(
                    l,
                    &mut y,
                    "Streuung",
                    short,
                    Target::Field(FieldId::PatSpread),
                );
                random(l, &mut y);
            }
            // Paket 7b: Regler der neuen Arten (paket-7 §2.1)
            Some(
                p @ (Pattern::Concrete { .. }
                | Pattern::Timber { .. }
                | Pattern::Tiles { .. }
                | Pattern::Stone { .. }),
            ) => {
                let num = |l: &mut AttrLayout, y: &mut f32, key: &'static str| {
                    row(
                        l,
                        y,
                        num_spec(key).0,
                        short,
                        Target::Field(FieldId::PatNum(key)),
                    );
                };
                let (ww, wh) = (t.size.swatch_w * s, t.size.swatch_h * s);
                let fw = ((cw - 2.0 * 6.0 * s) / 3.0).floor();
                let share_w = (fw - ww.min(fw) - 4.0 * s).max(30.0 * s);
                // Farben mit Anteil daneben wie die Läuferfarben, dann Fuge
                let colors = |l: &mut AttrLayout, y: &mut f32| {
                    l.texts.push(UiText::label(x, *y + 18.0 * s, "Farben"));
                    for k in 0..3 {
                        let cx = vx + k as f32 * (fw + 6.0 * s);
                        l.items.push((
                            Rect::new(cx, *y + (fh - wh) * 0.5, ww.min(fw), wh),
                            Target::Swatch(ColorTarget::PatStone(id, k)),
                        ));
                        let r = Rect::new(cx + fw - share_w, *y, share_w, fh);
                        if k < 2 {
                            l.items.push((r, Target::Field(FieldId::PatShare(k))));
                        } else {
                            let rest = self.pat_value(FieldId::PatShare(2), m);
                            l.readonly.push((r, format!("{rest} %")));
                        }
                    }
                    *y += step;
                    l.texts.push(UiText::label(x, *y + 18.0 * s, "Fugenfarbe"));
                    l.items.push((
                        Rect::new(vx, *y + (fh - wh) * 0.5, ww, wh),
                        Target::Swatch(ColorTarget::PatJoint(id)),
                    ));
                    *y += step;
                };
                match p {
                    Pattern::Concrete { .. } => {
                        num(l, &mut y, "w");
                        num(l, &mut y, "h");
                        l.texts.push(UiText::label(x, y + 18.0 * s, "Stoß"));
                        l.items.push((
                            Rect::new(vx, y, short, fh),
                            Target::Field(FieldId::PatNum("joint")),
                        ));
                        y += step;
                        row(
                            l,
                            &mut y,
                            "Ankerlöcher",
                            cw,
                            Target::Combo(ComboId::PatAnchors),
                        );
                        num(l, &mut y, "cloud");
                        num(l, &mut y, "pores");
                    }
                    Pattern::Timber { .. } => {
                        row(
                            l,
                            &mut y,
                            "Richtung",
                            cw,
                            Target::Combo(ComboId::PatWoodDir),
                        );
                        num(l, &mut y, "board");
                        num(l, &mut y, "joint");
                        num(l, &mut y, "grain");
                        l.texts.push(UiText::label(x, y + 18.0 * s, "Holzfarben"));
                        for k in 0..2 {
                            let cx = vx + k as f32 * (fw + 6.0 * s);
                            l.items.push((
                                Rect::new(cx, y + (fh - wh) * 0.5, ww.min(fw), wh),
                                Target::Swatch(ColorTarget::PatWood(id, k)),
                            ));
                        }
                        y += step;
                    }
                    Pattern::Tiles { .. } => {
                        l.texts
                            .push(UiText::label(x, y + 18.0 * s, "Länge, Breite"));
                        l.items.push((
                            Rect::new(vx, y, short, fh),
                            Target::Field(FieldId::PatNum("len")),
                        ));
                        l.items.push((
                            Rect::new(vx + short + 8.0 * s, y, short, fh),
                            Target::Field(FieldId::PatNum("wid")),
                        ));
                        y += step;
                        num(l, &mut y, "joint");
                        row(l, &mut y, "Raster", cw, Target::Combo(ComboId::PatGrid));
                        colors(l, &mut y);
                        num(l, &mut y, "spread");
                    }
                    _ => {
                        num(l, &mut y, "size");
                        num(l, &mut y, "joint");
                        num(l, &mut y, "irr");
                        colors(l, &mut y);
                    }
                }
                random(l, &mut y);
            }
            _ => {}
        }
        if window {
            return;
        }
        // Verweis ins Fenster „Muster“ (paket-7 §1.1), in Akzentfarbe
        l.items.push((
            Rect::new(vx, y, cw.max(110.0 * s), fh),
            Target::Pw(Pw::Open),
        ));
        y += step;
        // Hinweise je Zeile (soll-p6-5)
        let mut hint = if o.pattern.is_none() {
            String::from(
                "Arten: ohne (Vorgabe), Mauerwerk, Putz, Sichtbeton, Holzschalung, Platten, \
                 Naturstein.",
            )
        } else {
            String::new()
        };
        if o.pattern.is_some() && o.pattern == proctex::factory_for(o.guid) {
            hint.push_str("Werkswerte nach Jörns Vorlage.\n");
        }
        if matches!(o.pattern, Some(Pattern::Masonry { .. })) {
            hint.push_str(
                "Flammung: rote Läufer laufen zu den Enden grau aus.\n\
                 Relief: Brandrillen und Sinterpunkte; 0 % = glatt.",
            );
        }
        if matches!(o.pattern, Some(Pattern::Foreign(_))) {
            hint =
                "Muster, das diese Programmversion nicht lesen kann; bleibt unverändert erhalten."
                    .into();
        }
        // Vorschau: rechte Spalte ab der Überschrift „Muster“, sonst darunter
        let (px, mut py) = if right {
            (x + sw - pw, top)
        } else {
            (x, y + 4.0 * s)
        };
        l.texts
            .push(UiText::group_title(px, py + 14.0 * s, "Vorschau"));
        py += 24.0 * s;
        let ph = pw.min((bottom - py - 24.0 * s).max(40.0 * s));
        l.preview = Some(Rect::new(px, py, pw, ph));
        l.texts
            .push(UiText::dim(px, py + ph + 16.0 * s, "3D (Würfel)"));
        let (tx, ty) = if right {
            (px, py + ph + 26.0 * s)
        } else {
            (px + pw + gap, py)
        };
        let mut low = py + ph + 26.0 * s;
        if o.pattern.as_ref().is_some_and(has_joints) && ty + ph + 20.0 * s <= bottom {
            l.preview2 = Some(Rect::new(tx, ty, pw, ph));
            l.texts
                .push(UiText::dim(tx, ty + ph + 16.0 * s, "Ansicht (Fugen)"));
            low = low.max(ty + ph + 26.0 * s);
        }
        let y = if right { y } else { low };
        let wrap_w = if right {
            (sw - pw - gap).max(120.0 * s)
        } else {
            sw
        };
        // Die Verwender stehen unter der Liste („Löschen gesperrt …“)
        l.texts
            .push(UiText::wrapped_in(x, y + 10.0 * s, hint, wrap_w));
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
            _ => self.pat_value(f, m),
        }
    }

    /// Muster der gewählten Oberfläche.
    fn sel_pattern(&self, m: &Model) -> Option<(SurfaceId, Pattern)> {
        self.sel_surf(m)
            .and_then(|(id, o)| o.pattern.map(|p| (id, p)))
    }

    /// Text eines Musterfelds.
    fn pat_value(&self, f: FieldId, m: &Model) -> String {
        let Some((_, p)) = self.sel_pattern(m) else {
            return String::new();
        };
        match (f, p) {
            (FieldId::PatLen, Pattern::Masonry { len, .. }) => num_short(len),
            (FieldId::PatH, Pattern::Masonry { h, .. }) => num_short(h),
            (FieldId::PatJoint, Pattern::Masonry { joint, .. }) => num_short(joint),
            (FieldId::PatShare(k), mut p) => {
                proctex::palette_mut(&mut p).map_or(String::new(), |pal| num_short(pal[k.min(2)].1))
            }
            (FieldId::PatNum(key), p) => proctex::value(&p, key).map_or(String::new(), num_short),
            (FieldId::PatSpread, Pattern::Masonry { spread, .. })
            | (FieldId::PatSpread, Pattern::Plaster { spread, .. }) => num_short(spread),
            (FieldId::PatGrain, Pattern::Plaster { grain, .. }) => num_short(grain),
            (FieldId::PatFlame, Pattern::Masonry { flame, .. }) => num_short(flame),
            (FieldId::PatRelief, Pattern::Masonry { relief, .. }) => num_short(relief),
            _ => String::new(),
        }
    }

    /// Setzt das Muster der gewählten Oberfläche (geprüft nach Regel 57).
    fn put_pattern(
        &mut self,
        id: SurfaceId,
        p: Option<Pattern>,
        cx: &mut Ctx,
        out: &mut Out,
    ) -> Result<(), String> {
        if let Some(p) = &p {
            proctex::validate(p)?;
        }
        out.model |= cx.scene.edit_attr(|m| {
            m.attr().surface(id).is_some_and(|o| o.pattern != p) && m.set_surface_pattern(id, p)
        });
        Ok(())
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
            field_range(text, label, (lo, hi), dec, unit)
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
            FieldId::PatLen
            | FieldId::PatH
            | FieldId::PatJoint
            | FieldId::PatShare(_)
            | FieldId::PatSpread
            | FieldId::PatGrain
            | FieldId::PatFlame
            | FieldId::PatRelief
            | FieldId::PatNum(_) => {
                let Some((id, p)) = self.sel_pattern(m) else {
                    return Ok(());
                };
                let q = pattern_field_id(&p, f, text)?;
                if q != p {
                    self.put_pattern(id, Some(q), cx, out)?;
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
                pattern: None,
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
            Target::PatRandom => {
                // Neuer Startwert unter 2^24 (Looks-Zeilen, RGBA32F exakt)
                let Some((sid, mut p)) = self.sel_pattern(m) else {
                    return;
                };
                if matches!(p, Pattern::Foreign(_)) {
                    return;
                }
                let old = proctex::seed_of(&p);
                let (mut seed, mut k) = (old, 1u32);
                while seed == old {
                    seed =
                        proctex::lowbias32(old ^ k.wrapping_mul(0x9e37_79b9)) & proctex::SEED_MAX;
                    k += 1;
                }
                p = proctex::with_seed(&p, seed);
                let _ = self.put_pattern(sid, Some(p), cx, out);
            }
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
        let m = cx.scene.model();
        let l = self.attr_layout(t, &w, cx.scene);
        let anchor = l
            .items
            .iter()
            .find(|(_, tg)| *tg == Target::Combo(id))
            .map_or(l.side, |(r, _)| *r);
        let fill = self.sel_fill(m).map(|x| x.1);
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
            ComboId::PatKind
            | ComboId::PatFormat
            | ComboId::PatBond
            | ComboId::PatAnchors
            | ComboId::PatWoodDir
            | ComboId::PatGrid => {
                let p = self.sel_pattern(m).map(|x| x.1);
                let (items, sel) = pattern_combo(id, p.as_ref(), self.pat_free);
                (items, Vec::new(), sel)
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
            ComboId::PatKind => {
                let Some((sid, o)) = self.sel_surf(m) else {
                    return;
                };
                self.pat_free = false;
                let kind = |p: &Pattern| std::mem::discriminant(p);
                let p = match (i, o.pattern) {
                    (0, _) => None,
                    (1, Some(p @ Pattern::Masonry { .. }))
                    | (2, Some(p @ Pattern::Plaster { .. })) => Some(p),
                    (1, _) => Some(proctex::masonry_default()),
                    (2, _) => Some(proctex::plaster_default()),
                    (3..=6, old) => {
                        let start =
                            proctex::preset_named(KIND_PRESETS[i - 3]).map(|v| v.pattern.clone());
                        match old {
                            Some(p) if start.as_ref().is_some_and(|s| kind(s) == kind(&p)) => {
                                Some(p)
                            }
                            _ => start,
                        }
                    }
                    (_, Some(p @ Pattern::Foreign(_))) => Some(p),
                    _ => None,
                };
                if let Err(e) = self.put_pattern(sid, p, cx, out) {
                    self.error = Some(e);
                }
            }
            ComboId::PatFormat => {
                let Some((sid, mut p)) = self.sel_pattern(m) else {
                    return;
                };
                let Pattern::Masonry { len, h, .. } = &mut p else {
                    return;
                };
                if let Some(f) = BRICK_FORMATS.get(i) {
                    self.pat_free = false;
                    (*len, *h) = (f.1, f.2);
                    if let Err(e) = self.put_pattern(sid, Some(p), cx, out) {
                        self.error = Some(e);
                    }
                } else {
                    self.pat_free = true;
                    out.repaint = true;
                }
            }
            ComboId::PatBond => {
                let Some((sid, mut p)) = self.sel_pattern(m) else {
                    return;
                };
                let Pattern::Masonry { bond, .. } = &mut p else {
                    return;
                };
                *bond = BONDS[i.min(BONDS.len() - 1)];
                if let Err(e) = self.put_pattern(sid, Some(p), cx, out) {
                    self.error = Some(e);
                }
            }
            ComboId::PatAnchors | ComboId::PatWoodDir | ComboId::PatGrid => {
                let Some((sid, mut p)) = self.sel_pattern(m) else {
                    return;
                };
                match (&mut p, id) {
                    (Pattern::Concrete { anchors, .. }, ComboId::PatAnchors) => *anchors = i == 1,
                    (Pattern::Timber { vertical, .. }, ComboId::PatWoodDir) => *vertical = i == 0,
                    (Pattern::Tiles { half, .. }, ComboId::PatGrid) => *half = i == 1,
                    _ => return,
                }
                if let Err(e) = self.put_pattern(sid, Some(p), cx, out) {
                    self.error = Some(e);
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
            _ => "Muster",
        };
        let tx = at(Rect::new(l.thumb_x, 0.0, 0.0, 0.0)).x;
        label(c, bold, right, small, tx, head_y, u.text_dim);
        let line = s.round().max(1.0);
        c.fill_rect(lr.x, at(l.body).y - line, lr.w, line, u.border);
        // Zeilen in eigenem Bild (Bildlauf schneidet ab)
        let body = at(l.body);
        let mut tiles = self.tiles.borrow_mut();
        tiles.sync(m, t, s);
        let mut bc = tiles.take_rows(body.w.ceil() as usize, body.h.ceil() as usize);
        let sel = self.sel_index(m);
        let list = names(m, self.tab);
        // Kacheln nur der sichtbaren Zeilen, aus dem Speicher (M1)
        let keys = self.thumb_keys(m);
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
            if let Some(img) = keys.get(i).and_then(|&k| tiles.get(m, t, s, k)) {
                let y = r.y + (r.h - img.height as f32) * 0.5;
                bc.blit(img, thumb_x as i32, y as i32);
            }
        }
        c.blit(&bc, body.x as i32, body.y as i32);
        tiles.give_rows(bc);
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
            // umbrochen, gekürzt erst am Ende der letzten Zeile
            // (Einstellungen (al))
            let all = format!("Löschen gesperrt: wird verwendet von {}", users.join(", "));
            let mut lines = widgets::wrap(regular, &all, small, lr.w);
            if lines.len() > 3 {
                lines.truncate(3);
                // „ …“ passt noch in die dritte Zeile (Einstellungen (al))
                let fits =
                    |l: &str| regular.is_none_or(|f| f.width(&format!("{l} …"), small) <= lr.w);
                while !lines[2].is_empty() && !fits(&lines[2]) {
                    lines[2].pop();
                }
                let end = lines[2].trim_end().len();
                lines[2].truncate(end);
                lines[2].push_str(" …");
            }
            hints.extend(lines);
        }
        if self.tab == Tab::Fills {
            hints.push("Farben kommen vom Baustoff".into());
            hints.push("(Dateimenü „Baustoffe …“).".into());
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
        self.paint_attr_items(c, &l, t, fonts, s, sc, at);
        if let Some(pr) = l.preview {
            let pr = at(pr);
            // Große Vorschauen nur bei Auswahl- oder Darstellungswechsel
            // neu (M2)
            match self.tab {
                Tab::LineTypes => self.paint_lt_preview(c, &mut tiles, pr, m, t, fonts, s),
                Tab::Fills => {
                    if let Some((id, _)) = self.sel_fill(m) {
                        tiles.preview(c, 0, TileKey::Fill(id), pr, true, |c| {
                            paint_fill_preview(c, pr, m, t, s, fill_display(m, id))
                        });
                    }
                }
                Tab::Surfaces => {
                    if let Some((id, o)) = self.sel_surf(m) {
                        // wilder Verband: Tabelle über den Hintergrundweg,
                        // bis dahin Mischfarbe, dann Einblendung
                        let ready = o.pattern.as_ref().is_none_or(proctex::pattern_ready);
                        tiles.preview(c, 0, TileKey::Surface(id), pr, ready, |c| {
                            paint_cube(c, pr, &o, t, s)
                        });
                        if let Some(r2) = l.preview2.map(at) {
                            tiles.preview(c, 1, TileKey::Surface(id), r2, ready, |c| {
                                paint_elevation_tile(c, r2, m, &o, t, s)
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Beschriftungen, Felder, Listen und Knöpfe einer Lage (Attributreiter
    /// und Regler des Fensters „Muster“).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_attr_items(
        &self,
        c: &mut Canvas,
        l: &AttrLayout,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
        sc: &Scene,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let u = &t.ui;
        let m = sc.model();
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (font, small) = (t.size.font * s, t.size.font_small * s);
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
                TextKind::Wrap(w) => {
                    let max = max.min(w);
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
                    let text = self.combo_shown(id, m);
                    widgets::combo_icon(c, fonts, rr, &text, None, hover, open, s, t);
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
                Target::Pw(Pw::Open) => {
                    let col = if hover { u.accent_hover } else { u.accent };
                    let cap = regular.map_or(font * 0.7, |f| f.cap_height(font));
                    let y = rr.y + (rr.h + cap) * 0.5;
                    label(c, regular, "Mehr Muster …", font, rr.x, y, col);
                }
                Target::PatRandom => {
                    let st = ButtonState {
                        hover,
                        pressed: self.pressed == Some(*tg) && hover,
                        active: false,
                        disabled: false,
                    };
                    widgets::button(c, fonts, rr, "⚄ Zufall", st, s, t);
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
    }

    /// Text einer geschlossenen Auswahlliste.
    fn combo_shown(&self, id: ComboId, m: &Model) -> String {
        let fill = self.sel_fill(m).map(|x| x.1);
        match id {
            ComboId::FillKind => {
                let text = match fill.map(|f| f.kind) {
                    Some(FillKind::Empty) => "Leer",
                    Some(FillKind::Solid) => "Vollfläche",
                    Some(FillKind::Lines(_)) => "Linien",
                    _ => "Zickzack",
                };
                text.into()
            }
            ComboId::FillSpace => "Papier".into(),
            ComboId::PatKind
            | ComboId::PatFormat
            | ComboId::PatBond
            | ComboId::PatAnchors
            | ComboId::PatWoodDir
            | ComboId::PatGrid => {
                let p = self.sel_pattern(m).map(|x| x.1);
                let (items, sel) = pattern_combo(id, p.as_ref(), self.pat_free);
                items.get(sel).cloned().unwrap_or_default()
            }
            ComboId::PenWidth | ComboId::Scheme => String::new(),
        }
    }

    /// Kacheln der Listenzeilen ([`attr_pick::Tiles`] malt sie).
    fn thumb_keys(&self, m: &Model) -> Vec<TileKey> {
        match self.tab {
            Tab::LineTypes => lt_list(m).iter().map(|x| TileKey::LineType(x.0)).collect(),
            Tab::Fills => fill_list(m).iter().map(|x| TileKey::Fill(x.0)).collect(),
            Tab::Surfaces => surf_list(m).iter().map(|x| TileKey::Surface(x.0)).collect(),
            _ => Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_lt_preview(
        &self,
        c: &mut Canvas,
        tiles: &mut Tiles,
        r: Rect,
        m: &Model,
        t: &Theme,
        fonts: &Fonts,
        s: f32,
    ) {
        let Some((id, lt)) = self.sel_lt(m) else {
            return;
        };
        let regular = fonts.regular.as_ref();
        let small = t.size.font_small * s;
        let row = r.h / PREVIEW_WIDTHS.len() as f32;
        tiles.preview(c, 0, TileKey::LineType(id), r, true, |c| {
            for (i, mm) in PREVIEW_WIDTHS.iter().enumerate() {
                let y = r.y + i as f32 * row;
                let w = (mm * t.px_per_mm * s).max(0.6);
                let img = line_strip(m, t, &lt.pattern, r.w, row - 6.0 * s, w, s);
                c.blit(&img, r.x as i32, y as i32);
            }
        });
        for (i, mm) in PREVIEW_WIDTHS.iter().enumerate() {
            let y = r.y + i as f32 * row;
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

/// Zahl aus `text` in `lo..=hi`, gerundet auf `dec` Stellen; sonst die
/// Meldung „Bezeichnung: erlaubt … bis …“.
fn field_range(
    text: &str,
    label: &str,
    (lo, hi): (f32, f32),
    dec: usize,
    unit: &str,
) -> Result<f32, String> {
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
}

/// Muster nach Eingabe von `text` in das Musterfeld `f` (Grenzen, Rundung
/// und Meldungen der Einstellungen). Passt das Feld nicht zur Art, kommt
/// das Muster unverändert zurück.
fn pattern_field_id(p: &Pattern, f: FieldId, text: &str) -> Result<Pattern, String> {
    let range = |label: &str, lo: f32, hi: f32, dec: usize, unit: &str| {
        field_range(text, label, (lo, hi), dec, unit)
    };
    // Anteil 1 oder 2 einer Palette; Anteil 3 ist der Rest auf 100 %
    let share = |palette: &mut proctex::Palette, k: usize| -> Result<(), String> {
        let k = k.min(1);
        let other = palette[1 - k].1;
        let v = range("Anteil", 0.0, 100.0 - other, 0, "%")?;
        palette[k].1 = v;
        palette[2].1 = 100.0 - v - other;
        Ok(())
    };
    let mut p = p.clone();
    if let FieldId::PatNum(key) = f {
        let Some(&(_, lo, hi)) = proctex::limits(proctex::gen_word(&p))
            .iter()
            .find(|l| l.0 == key)
        else {
            return Ok(p);
        };
        let (label, unit, dec) = num_spec(key);
        let v = range(label, lo, hi, dec, unit)?;
        if proctex::value(&p, key) != Some(v) {
            proctex::set_value(&mut p, key, v);
        }
        return Ok(p);
    }
    match &mut p {
        Pattern::Masonry {
            len,
            h,
            joint,
            palette,
            flame,
            relief,
            spread,
            ..
        } => match f {
            FieldId::PatLen => *len = range("Steinlänge", 50.0, 600.0, 0, "mm")?,
            FieldId::PatH => *h = range("Steinhöhe", 20.0, 300.0, 0, "mm")?,
            FieldId::PatJoint => *joint = range("Fuge", 6.0, 15.0, 1, "mm")?,
            FieldId::PatShare(k) => share(palette, k)?,
            FieldId::PatSpread => *spread = range("Streuung", 0.0, 20.0, 0, "%")?,
            FieldId::PatFlame => *flame = range("Flammung", 0.0, 100.0, 0, "%")?,
            FieldId::PatRelief => *relief = range("Relief", 0.0, 100.0, 0, "%")?,
            _ => {}
        },
        Pattern::Plaster { grain, spread, .. } => match f {
            FieldId::PatGrain => *grain = range("Körnung", 0.5, 5.0, 1, "mm")?,
            FieldId::PatSpread => *spread = range("Streuung", 0.0, 10.0, 0, "%")?,
            _ => {}
        },
        Pattern::Tiles { palette, .. } | Pattern::Stone { palette, .. } => {
            if let FieldId::PatShare(k) = f {
                share(palette, k)?
            }
        }
        _ => {}
    }
    Ok(p)
}

/// Wie ein Musterfeld der Einstellungen, mit dem Schlüssel des Felds:
/// "len", "h", "joint", "share1", "share2", "spread", "grain", "flame",
/// "relief" oder ein Schlüssel aus [`proctex::limits`] (Abnahme A298).
#[cfg(test)]
pub fn pattern_field(p: &Pattern, key: &str, text: &str) -> Result<Pattern, String> {
    let f = match key {
        "len" => FieldId::PatLen,
        "h" => FieldId::PatH,
        "joint" => FieldId::PatJoint,
        "share1" => FieldId::PatShare(0),
        "share2" => FieldId::PatShare(1),
        "spread" => FieldId::PatSpread,
        "grain" => FieldId::PatGrain,
        "flame" => FieldId::PatFlame,
        "relief" => FieldId::PatRelief,
        _ => match proctex::limits(proctex::gen_word(p))
            .iter()
            .find(|l| l.0 == key)
        {
            Some(l) => FieldId::PatNum(l.0),
            None => return Ok(p.clone()),
        },
    };
    pattern_field_id(p, f, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Ctx;
    use sk_ui::widgets::Fonts;

    fn win() -> Win {
        Win {
            w: 1600,
            h: 1000,
            top: 32,
            scale: 1.0,
        }
    }

    /// Paket 7b: Die Regler der neuen Arten stehen im Abschnitt „Muster“
    /// und setzen das Muster; Flammung und Relief gelten als Musterfelder
    /// (vorher liefen sie ins Leere).
    #[test]
    fn regler_aller_arten() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let mut th = Theme::dark();
        let mut st = crate::settings::Settings::new(
            ["skizzeo", "--ohne-einstellungen"]
                .map(String::from)
                .into_iter(),
            None,
        );
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut p = Prefs::open(&mut s, &th);
        p.tab = Tab::Surfaces;
        let (id, _) = p.sel_surf(s.model()).expect("Oberfläche");
        let mut out = Out::default();
        for (name, keys) in [
            (
                "Sichtbeton mittelgrau",
                &["w", "h", "joint", "cloud", "pores"][..],
            ),
            ("Holzschalung Lärche", &["board", "joint", "grain"][..]),
            (
                "Betonplatten 40 × 40",
                &["len", "wid", "joint", "spread"][..],
            ),
            ("Naturstein", &["size", "joint", "irr"][..]),
        ] {
            let start = proctex::preset_named(name).unwrap().pattern.clone();
            s.edit_attr(|m| m.set_surface_pattern(id, Some(start.clone())));
            let l = p.attr_layout(&th, &win(), &s);
            for key in keys {
                let f = FieldId::PatNum(key);
                assert!(
                    l.items.iter().any(|(_, t)| *t == Target::Field(f)),
                    "{name}: Feld {key}"
                );
                let &(_, lo, hi) = proctex::limits(proctex::gen_word(&start))
                    .iter()
                    .find(|x| x.0 == *key)
                    .unwrap();
                let v = ((lo + hi) / 2.0).round();
                let mut cx = Ctx {
                    scene: &mut s,
                    theme: &mut th,
                    settings: &mut st,
                    fonts: &fonts,
                    win: win(),
                };
                p.apply_value(f, &v.to_string(), &mut cx, &mut out)
                    .unwrap_or_else(|e| panic!("{name} {key}: {e}"));
                let now = s
                    .model()
                    .attr()
                    .surface(id)
                    .unwrap()
                    .pattern
                    .clone()
                    .unwrap();
                assert_eq!(proctex::value(&now, key), Some(v), "{name} {key}");
            }
        }
        let start = proctex::masonry_default();
        s.edit_attr(|m| m.set_surface_pattern(id, Some(start)));
        let mut cx = Ctx {
            scene: &mut s,
            theme: &mut th,
            settings: &mut st,
            fonts: &fonts,
            win: win(),
        };
        p.apply_value(FieldId::PatFlame, "40", &mut cx, &mut out)
            .unwrap();
        p.apply_value(FieldId::PatRelief, "30", &mut cx, &mut out)
            .unwrap();
        let now = s
            .model()
            .attr()
            .surface(id)
            .unwrap()
            .pattern
            .clone()
            .unwrap();
        assert_eq!(proctex::value(&now, "flame"), Some(40.0));
        assert_eq!(proctex::value(&now, "relief"), Some(30.0));
    }
}
