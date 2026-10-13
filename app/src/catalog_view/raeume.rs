//! Reiter „Räume“ im Bauteilkatalog (Raumbuch RB2, plan.md §4.3 und §7):
//! links die Raumvorlagen, rechts Name, Nutzungsgruppe nach DIN 277,
//! Anrechnung zur Wohnfläche und je Fläche der Ausbautyp (Boden, Estrich,
//! Wand, Wandfliesen mit Höhe, Decke, Sockel). Vorlagen des Werks und der
//! Firma stehen mit in der Liste und kommen mit der ersten Änderung ins
//! Projekt, samt ihren Ausbautypen; OK übernimmt sie mit den Typen.

use super::{label, rounded, Catalog, ComboId, FieldId, Item, ListLayout, Popup, Target};
use super::{CONTENT_X, PAD};
use crate::prefs::Win;
use crate::window_kit::parse_num;
use sk_model::model::{
    space_template_rank, FINISH_TYPES, SPACE_TEMPLATES, TILES_DEFAULT, TILES_HEIGHT, WOFLV,
};
use sk_model::{import_type, Guid, SpaceGroup, SpaceType, TypeCategory, WallTiles};
use sk_paint::Canvas;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts, Rect};
use std::borrow::Cow;

/// Woher eine Vorlage der Liste kommt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Herkunft {
    Projekt,
    Firma,
    Werk,
}

/// Zeilen mit Ausbautyp: Beschriftung und was „leer“ heißt (Reihenfolge
/// wie [`SpaceType::slots`]).
const ZEILEN: [(&str, &str); 6] = [
    ("Bodenbelag", "ohne Belag"),
    ("Estrich", "nach Geschoss"),
    ("Wand", "ohne Bekleidung"),
    ("Wandfliesen", "keine"),
    ("Decke", "ohne Bekleidung"),
    ("Sockel", "keine Sockelleiste"),
];

/// Erste Zeile und Zeilenabstand (dip).
const ZEILE_Y: f32 = 214.0;
const ZEILE_H: f32 = 46.0;
/// Auswahl der Ausbautypen: links, Breite.
const TYP_X: f32 = CONTENT_X + 120.0;
const TYP_W: f32 = 360.0;

/// Fliesenhöhe aus dem Feld (m, 0,10 bis 4,00, auf cm) in mm.
pub(super) fn parse_tiles(t: &str) -> Result<f64, String> {
    let (lo, hi) = TILES_HEIGHT;
    let msg = || format!("Fliesenhöhe {} bis {} m", m_text(lo), m_text(hi));
    let m = parse_num(t).ok_or_else(msg)?;
    let mm = (m * 100.0).round() * 10.0;
    if !(lo..=hi).contains(&mm) {
        return Err(msg());
    }
    Ok(mm)
}

/// „1,20“ aus mm.
pub(super) fn m_text(mm: f64) -> String {
    format!("{:.2}", mm / 1000.0).replace('.', ",")
}

fn woflv_text(p: u8) -> String {
    if p == 0 {
        "0 % (Zubehör)".into()
    } else {
        format!("{p} %")
    }
}

impl Catalog {
    /// Vorlagen der Liste: die des Projekts, dazu die der Firma und des
    /// Werks, die es noch nicht führt (nicht doppelt nach Guid oder Name).
    /// Werksvorlagen in ihrer Reihenfolge, die übrigen nach Name.
    pub(super) fn raum_eintraege(&self) -> Vec<(Guid, String, Herkunft)> {
        let mut v: Vec<(Guid, String, Herkunft)> = self
            .work
            .space_types()
            .iter()
            .map(|s| (s.guid, s.name.clone(), Herkunft::Projekt))
            .collect();
        let frei = |v: &[(Guid, String, Herkunft)], g: Guid, n: &str| {
            !v.iter().any(|x| x.0 == g || x.1 == n)
        };
        if let Some(lib) = self.company_lib() {
            for s in &lib.space_types {
                if frei(&v, s.guid, &s.name) {
                    v.push((s.guid, s.name.clone(), Herkunft::Firma));
                }
            }
        }
        for t in &SPACE_TEMPLATES {
            if frei(&v, t.guid, t.name) {
                v.push((t.guid, t.name.into(), Herkunft::Werk));
            }
        }
        v.sort_by(|a, b| (space_template_rank(a.0), &a.1).cmp(&(space_template_rank(b.0), &b.1)));
        v
    }

    /// Die gewählte Vorlage und ihre Herkunft.
    pub(super) fn raum(&self) -> Option<(SpaceType, Herkunft)> {
        self.raum_zu(self.room_sel?)
            .map(|(s, h)| (s.into_owned(), h))
    }

    /// Vorlage `g` und ihre Herkunft wie in [`Catalog::raum_eintraege`]:
    /// aus dem Projekt, sonst der Firma, sonst dem Werk. Ohne die Liste zu
    /// bauen und ohne Kopie, außer für eine Werksvorlage (je Kachel und
    /// Mausbewegung).
    fn raum_zu(&self, g: Guid) -> Option<(Cow<'_, SpaceType>, Herkunft)> {
        if let Some(s) = self.work.space_type(g) {
            return Some((Cow::Borrowed(s), Herkunft::Projekt));
        }
        if let Some(s) = self
            .company_lib()
            .and_then(|l| l.space_types.iter().find(|s| s.guid == g))
        {
            return Some((Cow::Borrowed(s), Herkunft::Firma));
        }
        SPACE_TEMPLATES
            .iter()
            .find(|t| t.guid == g)
            .map(|t| (Cow::Owned(t.space_type()), Herkunft::Werk))
    }

    /// Name, Kurzzeichen und Dicke (mm) eines Ausbautyps: aus dem Projekt,
    /// sonst aus der Firma, sonst aus dem Werk.
    pub(super) fn ausbau_info(&self, g: Guid) -> Option<(String, String, f64)> {
        if let Some(t) = self
            .work
            .type_by_guid(g)
            .and_then(|id| self.work.layer_set(id))
        {
            return Some((t.name.clone(), t.code.clone(), t.thickness()));
        }
        if let Some(t) = self
            .company_lib()
            .and_then(|l| l.types.get(l.type_by_guid(g)?))
        {
            return Some((t.name.clone(), t.code.clone(), t.thickness()));
        }
        FINISH_TYPES.iter().find(|t| t.guid == g).map(|t| {
            let d = t.layers.iter().map(|l| l.1).sum();
            (t.name.to_string(), t.code.to_string(), d)
        })
    }

    /// Ausbautypen einer Art zur Wahl: die des Projekts nach Name, dann die
    /// der Firma und des Werks, die es noch nicht führt.
    pub(super) fn ausbau_wahl(&self, cat: TypeCategory) -> Vec<Guid> {
        let mut v: Vec<(String, Guid)> = self
            .work
            .layer_sets()
            .iter()
            .filter(|(_, t)| t.category == cat)
            .map(|(_, t)| (t.name.clone(), t.guid))
            .collect();
        v.sort();
        let mut v: Vec<Guid> = v.into_iter().map(|x| x.1).collect();
        if let Some(lib) = self.company_lib() {
            for (_, t) in lib.types.iter() {
                if t.category == cat && !v.contains(&t.guid) {
                    v.push(t.guid);
                }
            }
        }
        for t in &FINISH_TYPES {
            if t.category == cat && !v.contains(&t.guid) {
                v.push(t.guid);
            }
        }
        v
    }

    /// Text einer Wahl: „Parkett Eiche 20 · 2 cm“.
    fn ausbau_text(&self, g: Option<Guid>, leer: &str) -> String {
        match g.and_then(|g| self.ausbau_info(g)) {
            Some((name, _, d)) if d > 0.0 => format!("{name} · {}", super::cm_text(d)),
            Some((name, ..)) => name,
            None if g.is_some() => "fehlt".into(),
            None => leer.into(),
        }
    }

    // --- Lage -----------------------------------------------------------------

    /// Liste links: ein Kopf, darunter die Vorlagen.
    pub(super) fn rooms_layout(&self, t: &Theme, w: &Win) -> ListLayout {
        let s = w.scale;
        let body = self.list_body(t, w);
        let tile_h = t.size.catalog_tile_h * s;
        let mut y = body.y - self.scroll;
        let heads = vec![("Raumvorlagen".to_string(), y)];
        y += 24.0 * s;
        let mut tiles = Vec::new();
        for (g, ..) in self.raum_eintraege() {
            let r = Rect::new(
                body.x + 10.0 * s,
                y.round(),
                (t.size.catalog_list_w - 8.0) * s,
                tile_h,
            );
            tiles.push((Item::Room(g), r));
            y += tile_h + 4.0 * s;
        }
        let content = y + self.scroll - body.y;
        (tiles, heads, content)
    }

    pub(super) fn room_field_rect(&self, t: &Theme, w: &Win, f: FieldId) -> Option<Rect> {
        match f {
            FieldId::RoomName => Some(self.r(t, w, CONTENT_X, 72.0, 330.0, 32.0)),
            FieldId::TilesHeight => {
                self.raum_zu(self.room_sel?)?.0.tiles?;
                let y = ZEILE_Y + 3.0 * ZEILE_H;
                Some(self.r(t, w, TYP_X + TYP_W + 46.0, y, 76.0, 30.0))
            }
            _ => None,
        }
    }

    pub(super) fn room_combo_rect(&self, t: &Theme, w: &Win, id: ComboId) -> Rect {
        match id {
            ComboId::RoomGroup => self.r(t, w, CONTENT_X, 150.0, 330.0, 30.0),
            ComboId::RoomWoflv => self.r(t, w, CONTENT_X + 350.0, 150.0, 140.0, 30.0),
            ComboId::RoomSlot(i) => self.r(t, w, TYP_X, ZEILE_Y + i as f32 * ZEILE_H, TYP_W, 30.0),
            _ => Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Ziel rechts im Reiter „Räume“.
    pub(super) fn rooms_hit(&self, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        self.room_sel?;
        for f in [FieldId::RoomName, FieldId::TilesHeight] {
            if self
                .room_field_rect(t, w, f)
                .is_some_and(|r| r.contains(x, y))
            {
                return Some(Target::Field(f));
            }
        }
        let mut ids = vec![ComboId::RoomGroup, ComboId::RoomWoflv];
        ids.extend((0..ZEILEN.len()).map(ComboId::RoomSlot));
        ids.into_iter()
            .find(|id| self.room_combo_rect(t, w, *id).contains(x, y))
            .map(Target::Combo)
    }

    // --- Ändern ---------------------------------------------------------------

    /// Ändert die gewählte Vorlage in der Arbeitskopie; eine Vorlage der
    /// Firma oder des Werks kommt dabei ins Projekt, mit den Ausbautypen,
    /// auf die sie nach der Änderung verweist. Verstößt das Ergebnis gegen
    /// eine Regel, bleibt alles, wie es war, und der Fuß nennt den Grund.
    pub(super) fn room_change(&mut self, f: impl FnOnce(&mut SpaceType)) -> bool {
        let Some((mut s, herkunft)) = self.raum() else {
            return false;
        };
        if herkunft != Herkunft::Projekt {
            s.name = self.work.free_space_name(&s.name);
        }
        f(&mut s);
        // Erst auf der Kopie prüfen, dann anlegen: Eine abgelehnte Änderung
        // legt nichts an, und das Modell wird nicht geklont (Review #85).
        // Ins Projekt kommen nur die Ausbautypen, auf die die Vorlage jetzt
        // verweist.
        let lib = self.company.as_ref().map(|c| &c.0);
        let why = self.work.space_type_problem_before(&s, lib);
        let ok = why.is_none()
            && s.slots().into_iter().filter_map(|x| x.2).all(|g| {
                self.work.type_by_guid(g).is_some()
                    || self.work.ensure_finish_type(g).is_some()
                    || lib.is_some_and(|l| import_type(&mut self.work, l, g).is_some())
            })
            && self.work.put_space_type(s);
        if ok {
            self.message = None;
            return true;
        }
        self.message = Some(why.unwrap_or_else(|| "Vorlage nicht übernommen".into()));
        self.blink = Some(self.clock());
        false
    }

    /// Wert eines Felds der Vorlage.
    pub(super) fn room_field_value(&self, f: FieldId) -> String {
        let Some((s, _)) = self.raum() else {
            return String::new();
        };
        match f {
            FieldId::RoomName => s.name,
            FieldId::TilesHeight => s.tiles.map_or(String::new(), |t| m_text(t.height)),
            _ => String::new(),
        }
    }

    pub(super) fn room_apply_value(&mut self, f: FieldId, text: &str) -> Result<(), String> {
        let Some((s, _)) = self.raum() else {
            return Ok(());
        };
        match f {
            FieldId::RoomName => {
                let name = text.trim();
                if name.is_empty() {
                    return Err("Name fehlt".into());
                }
                if name == s.name {
                    return Ok(());
                }
                let taken = self
                    .raum_eintraege()
                    .iter()
                    .any(|e| e.0 != s.guid && e.1 == name);
                if taken {
                    return Err(format!("Eine Vorlage „{name}“ gibt es schon"));
                }
                let name = name.to_string();
                self.room_change(|s| s.name = name);
            }
            FieldId::TilesHeight => {
                let mm = parse_tiles(text)?;
                if s.tiles.is_some_and(|t| t.height != mm) {
                    self.room_change(|s| {
                        if let Some(t) = s.tiles.as_mut() {
                            t.height = mm;
                        }
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Auswahlliste einer Zeile: Einträge und gewählter.
    pub(super) fn room_combo_items(&self, id: ComboId) -> Option<(Vec<String>, Option<usize>)> {
        let (s, _) = self.raum()?;
        Some(match id {
            ComboId::RoomGroup => (
                SpaceGroup::ALL
                    .iter()
                    .map(|g| format!("{} {}", g.short(), g.name()))
                    .collect(),
                SpaceGroup::ALL.iter().position(|g| *g == s.group),
            ),
            ComboId::RoomWoflv => (
                WOFLV.iter().map(|p| woflv_text(*p)).collect(),
                WOFLV.iter().position(|p| *p == s.woflv),
            ),
            ComboId::RoomSlot(i) => {
                let (_, cat, cur) = s.slots()[i];
                let wahl = self.ausbau_wahl(cat);
                let mut items = vec![ZEILEN[i].1.to_string()];
                items.extend(wahl.iter().map(|g| self.ausbau_text(Some(*g), "")));
                let sel = match cur {
                    None => Some(0),
                    Some(g) => wahl.iter().position(|x| *x == g).map(|k| k + 1),
                };
                (items, sel)
            }
            _ => return None,
        })
    }

    /// Eintrag `k` einer Auswahlliste gewählt.
    pub(super) fn room_choose(&mut self, id: ComboId, k: usize) {
        let Some((s, _)) = self.raum() else {
            return;
        };
        match id {
            ComboId::RoomGroup => {
                if let Some(&g) = SpaceGroup::ALL.get(k).filter(|g| **g != s.group) {
                    self.room_change(|s| s.group = g);
                }
            }
            ComboId::RoomWoflv => {
                if let Some(&p) = WOFLV.get(k).filter(|p| **p != s.woflv) {
                    self.room_change(|s| s.woflv = p);
                }
            }
            ComboId::RoomSlot(i) => {
                let (_, cat, cur) = s.slots()[i];
                let neu = match k {
                    0 => None,
                    k => match self.ausbau_wahl(cat).get(k - 1) {
                        Some(g) => Some(*g),
                        None => return,
                    },
                };
                if neu == cur {
                    return;
                }
                self.room_change(|s| match i {
                    0 => s.floor = neu,
                    1 => s.screed = neu,
                    2 => s.wall = neu,
                    3 => {
                        let height = s.tiles.map_or(TILES_DEFAULT, |t| t.height);
                        s.tiles = neu.map(|set| WallTiles { set, height });
                    }
                    4 => s.ceiling = neu,
                    _ => s.skirting = neu,
                });
            }
            _ => {}
        }
    }

    /// „Neu“: eigene Vorlage „Raum“ mit dem Ausbau eines Wohnraums.
    pub(super) fn room_new(&mut self) {
        let mut s = SPACE_TEMPLATES[1].space_type();
        for g in s.slots().into_iter().filter_map(|x| x.2) {
            if self.work.ensure_finish_type(g).is_none() {
                return;
            }
        }
        s.guid = self.work.new_guid();
        s.name = self.free_room_name("Raum");
        if self.work.put_space_type(s.clone()) {
            self.room_sel = Some(s.guid);
        }
    }

    /// „Duplizieren“: Kopie der gewählten Vorlage, „… (Kopie)“.
    pub(super) fn room_duplicate(&mut self) {
        let Some((mut s, _)) = self.raum() else {
            return;
        };
        for g in s.slots().into_iter().filter_map(|x| x.2) {
            if self.work.type_by_guid(g).is_none() && self.work.ensure_finish_type(g).is_none() {
                let lib = self.company_lib().cloned();
                if !lib.is_some_and(|l| import_type(&mut self.work, &l, g).is_some()) {
                    return;
                }
            }
        }
        s.guid = self.work.new_guid();
        s.name = self.free_room_name(&format!("{} (Kopie)", s.name));
        if self.work.put_space_type(s.clone()) {
            self.room_sel = Some(s.guid);
        }
    }

    /// „Löschen“: nimmt die Vorlage aus dem Projekt. Eine des Werks oder
    /// der Firma steht danach wieder in ihrer ursprünglichen Fassung da.
    pub(super) fn room_delete(&mut self) {
        let Some(g) = self.room_sel.filter(|g| self.work.space_type(*g).is_some()) else {
            return;
        };
        self.work.remove_space_type(g);
        if !self.raum_eintraege().iter().any(|e| e.0 == g) {
            self.room_sel = self.raum_eintraege().first().map(|e| e.0);
        }
    }

    pub(super) fn room_in_project(&self) -> bool {
        self.room_sel
            .is_some_and(|g| self.work.space_type(g).is_some())
    }

    /// Räume, die die gewählte Vorlage nutzen; mit ihnen bleibt sie.
    pub(super) fn room_users(&self) -> usize {
        self.room_sel.map_or(0, |g| self.work.space_type_users(g))
    }

    fn free_room_name(&self, name: &str) -> String {
        let taken = |n: &str| self.raum_eintraege().iter().any(|e| e.1 == n);
        if !taken(name) {
            return name.into();
        }
        (2..)
            .map(|i| format!("{name} {i}"))
            .find(|n| !taken(n))
            .unwrap_or_default()
    }

    /// „In Firmenkatalog speichern“ im Reiter „Räume“: nur eine Vorlage des
    /// Projekts.
    pub(super) fn room_export(&mut self) {
        if let Some(g) = self.room_sel.filter(|_| self.room_in_project()) {
            self.popup = Some(Popup::Export(g, false));
        }
    }

    // --- Malen ----------------------------------------------------------------

    /// Kachel: Name, darunter Nutzungsgruppe und Bodenbelag, rechts die
    /// Herkunft.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_room_tile(
        &self,
        c: &mut Canvas,
        g: Guid,
        r: Rect,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sel: bool,
        hover: bool,
    ) {
        let Some((raum, herkunft)) = self.raum_zu(g) else {
            return;
        };
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
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
        let tx = r.x + 16.0 * s;
        let right = r.x + r.w - 10.0 * s;
        let px = t.size.font * s;
        let pd = t.size.font_detail * s;
        let mark = match herkunft {
            Herkunft::Projekt => "",
            Herkunft::Firma => "Firma",
            Herkunft::Werk => "Werk",
        };
        let mw = regular.map_or(0.0, |f| f.width(mark, pd));
        let font = if sel { bold } else { regular };
        let name = widgets::ellipsize(font, &raum.name, px, right - tx - mw - 8.0 * s);
        label(c, font, &name, px, tx, r.y + 20.0 * s, u.text);
        if !mark.is_empty() {
            label(c, regular, mark, pd, right - mw, r.y + 18.0 * s, u.text_dim);
        }
        let detail = self.room_detail(&raum);
        let detail = widgets::ellipsize(regular, &detail, pd, right - tx);
        label(c, regular, &detail, pd, tx, r.y + 38.0 * s, u.text_dim);
    }

    /// „NUF 1 · Parkett Eiche 20“.
    fn room_detail(&self, s: &SpaceType) -> String {
        let boden = s
            .floor
            .and_then(|f| self.ausbau_info(f))
            .map_or_else(|| "ohne Belag".to_string(), |i| i.0);
        format!("{} · {boden}", s.group.short())
    }

    /// Rechte Seite: Name, Nutzung, Wohnfläche, Ausbau je Fläche.
    pub(super) fn paint_rooms(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let small = t.size.font_small * s;
        let x0 = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x;
        let f = self.frame(t, w);
        let max = f.x + f.w - PAD * s - x0;
        let Some((raum, herkunft)) = self.raum() else {
            let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
            label(
                c,
                regular,
                "Keine Raumvorlage gewählt",
                small,
                x0,
                y,
                u.text_dim,
            );
            return;
        };
        // Name
        if let Some(r) = self.room_field_rect(t, w, FieldId::RoomName) {
            let v = self.room_field_value(FieldId::RoomName);
            let st = self.field_state(FieldId::RoomName, &v);
            widgets::text_field(c, fonts, r, &st, s, t);
        }
        let herkunft_text = match herkunft {
            Herkunft::Projekt => "Vorlage des Projekts",
            Herkunft::Firma => "Vorlage der Firma; kommt mit der ersten Änderung ins Projekt",
            Herkunft::Werk => "Werksvorlage; kommt mit der ersten Änderung ins Projekt",
        };
        let hy = self.r(t, w, 0.0, 120.0, 0.0, 0.0).y;
        let text = widgets::ellipsize(regular, herkunft_text, small, max);
        label(c, regular, &text, small, x0, hy, u.text_dim);
        // Nutzungsgruppe und Wohnfläche
        let ly = self.r(t, w, 0.0, 144.0, 0.0, 0.0).y;
        label(
            c,
            regular,
            "Nutzungsgruppe (DIN 277)",
            small,
            x0,
            ly,
            u.text_dim,
        );
        let wx = self.room_combo_rect(t, w, ComboId::RoomWoflv).x;
        label(c, regular, "Wohnfläche (WoFlV)", small, wx, ly, u.text_dim);
        let gruppe = format!("{} {}", raum.group.short(), raum.group.name());
        for (id, text) in [
            (ComboId::RoomGroup, gruppe),
            (ComboId::RoomWoflv, woflv_text(raum.woflv)),
        ] {
            let r = self.room_combo_rect(t, w, id);
            let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
            let text = widgets::ellipsize(regular, &text, t.size.font * s, r.w - 36.0 * s);
            let hov = self.hover == Some(Target::Combo(id));
            widgets::combo(c, fonts, r, &text, hov, open, s, t);
        }
        // Ausbau je Fläche
        let ay = self.r(t, w, 0.0, ZEILE_Y - 8.0, 0.0, 0.0).y;
        label(c, regular, "Ausbau je Fläche", small, x0, ay, u.text_dim);
        let slots = raum.slots();
        let mut boden = 0.0;
        for (i, (name, leer)) in ZEILEN.iter().enumerate() {
            let id = ComboId::RoomSlot(i);
            let r = self.room_combo_rect(t, w, id);
            label(
                c,
                regular,
                name,
                t.size.font * s,
                x0,
                r.y + 20.0 * s,
                u.text,
            );
            let cur = slots[i].2;
            let text = self.ausbau_text(cur, leer);
            let text = widgets::ellipsize(regular, &text, t.size.font * s, r.w - 36.0 * s);
            let open = matches!(&self.popup, Some(Popup::Combo(cb)) if cb.id == id);
            let hov = self.hover == Some(Target::Combo(id));
            widgets::combo(c, fonts, r, &text, hov, open, s, t);
            if i < 2 {
                boden += cur.and_then(|g| self.ausbau_info(g)).map_or(0.0, |x| x.2);
            }
            if i == 3 {
                if let Some(fr) = self.room_field_rect(t, w, FieldId::TilesHeight) {
                    label(
                        c,
                        regular,
                        "bis",
                        small,
                        fr.x - 30.0 * s,
                        r.y + 20.0 * s,
                        u.text_dim,
                    );
                    let v = self.room_field_value(FieldId::TilesHeight);
                    let st = self.field_state(FieldId::TilesHeight, &v);
                    widgets::field(c, fonts, fr, &st, s, t);
                    let ux = fr.x + fr.w + 10.0 * s;
                    label(
                        c,
                        regular,
                        "über Fußboden",
                        small,
                        ux,
                        r.y + 20.0 * s,
                        u.text_dim,
                    );
                }
            }
        }
        // Fußbodenaufbau: mit Estrich nach Geschoss gleicht dessen Dämmung
        // den Belag aus (Jörn 17:39: Fertigfußboden im Geschoss gleich hoch)
        let y = self
            .r(t, w, 0.0, ZEILE_Y + 6.0 * ZEILE_H + 14.0, 0.0, 0.0)
            .y;
        let summe = if raum.screed.is_some() {
            format!(
                "Fußbodenaufbau {} (Belag und Estrich)",
                super::cm_text(boden)
            )
        } else {
            "Estrich nach Geschoss: die Dämmung gleicht den Belag aus, der Fertigfußboden liegt im Geschoss gleich hoch".into()
        };
        let summe = widgets::ellipsize(regular, &summe, small, max);
        label(c, regular, &summe, small, x0, y, u.text_dim);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Tab};
    use super::*;
    use crate::scene::Scene;
    use sk_model::model::{
        BATH_SPACE_GUID, KITCHEN_SPACE_GUID, LIVING_SPACE_GUID, PARQUET_TYPE_GUID,
        PORCELAIN_TYPE_GUID, WALL_TILES_TYPE_GUID, WET_FLOOR_TYPE_GUID,
    };
    use sk_model::Model;
    use sk_platform::{Event, Key, Modifiers, MouseButton};

    const WIN: Win = Win {
        w: 1280,
        h: 800,
        top: 32,
        scale: 1.0,
    };

    fn klick(c: &mut Catalog, s: &mut Scene, r: Rect) -> super::super::Out {
        let (theme, f) = (Theme::dark(), Fonts::system());
        let mut cx = Ctx {
            scene: s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        let (x, y) = ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
        let mods = Modifiers::default();
        let button = MouseButton::Left;
        c.handle(&Event::MouseDown { button, x, y, mods }, &mut cx);
        c.handle(&Event::MouseUp { button, x, y, mods }, &mut cx)
    }

    fn tippen(c: &mut Catalog, s: &mut Scene, text: &str) {
        let (theme, f) = (Theme::dark(), Fonts::system());
        let mut cx = Ctx {
            scene: s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        for ch in text.chars() {
            c.handle(&Event::Text(ch), &mut cx);
        }
        let key = Event::Key {
            key: Key::Enter,
            down: true,
            mods: Modifiers::default(),
            repeat: false,
        };
        c.handle(&key, &mut cx);
    }

    /// Wählt im offenen Auswahlfeld den Eintrag mit dem Text.
    fn waehlen(c: &mut Catalog, s: &mut Scene, id: ComboId, text: &str) {
        let t = Theme::dark();
        let r = c.room_combo_rect(&t, &WIN, id);
        klick(c, s, r);
        let Some(Popup::Combo(cb)) = &c.popup else {
            panic!("keine Auswahl");
        };
        let k = cb
            .items
            .iter()
            .position(|x| x.0.starts_with(text))
            .unwrap_or_else(|| panic!("{text} fehlt"));
        let r = c.combo_rows(cb, &WIN).1[k];
        klick(c, s, r);
    }

    /// Reiter „Räume“: die 13 Werksvorlagen stehen ohne Projektvorlage in
    /// der Liste; eine Änderung holt die Vorlage samt Ausbautypen ins
    /// Projekt, OK übernimmt sie als ein Schritt.
    #[test]
    fn werksvorlage_aendern_und_uebernehmen() {
        let (t, f) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(7));
        let mut c = Catalog::open(&s, None);
        let tab = c.tab_rect(&t, &WIN, Tab::Rooms);
        klick(&mut c, &mut s, tab);
        assert_eq!(c.tab, Tab::Rooms);
        let (tiles, heads, _) = c.list_layout(&t, &WIN);
        assert_eq!(heads[0].0, "Raumvorlagen");
        assert_eq!(tiles.len(), 13);
        assert_eq!(c.raum_eintraege()[0].1, "Diele, Flur");
        // Küche wählen, Belag auf Feinsteinzeug lassen, Wohnfläche 50 %
        let k = c
            .raum_eintraege()
            .iter()
            .position(|e| e.0 == KITCHEN_SPACE_GUID)
            .unwrap();
        klick(&mut c, &mut s, tiles[k].1);
        assert_eq!(c.room_sel, Some(KITCHEN_SPACE_GUID));
        assert!(c.is_selected(Item::Room(KITCHEN_SPACE_GUID)));
        assert!(!c.room_in_project());
        let (img, ..) = c.paint(&t, &f, &WIN);
        assert!(img.width > 0);
        waehlen(&mut c, &mut s, ComboId::RoomWoflv, "50 %");
        assert!(c.room_in_project());
        let k = c.work.space_type(KITCHEN_SPACE_GUID).unwrap();
        assert_eq!((k.woflv, k.floor), (50, Some(PORCELAIN_TYPE_GUID)));
        // Wandfliesen dazu, Höhe 0,60 m
        waehlen(&mut c, &mut s, ComboId::RoomSlot(3), "Wandfliesen 30 × 60");
        let r = c.room_field_rect(&t, &WIN, FieldId::TilesHeight).unwrap();
        klick(&mut c, &mut s, r);
        tippen(&mut c, &mut s, "0,6");
        let k = c.work.space_type(KITCHEN_SPACE_GUID).unwrap().clone();
        assert_eq!(
            k.tiles,
            Some(WallTiles {
                set: WALL_TILES_TYPE_GUID,
                height: 600.0
            })
        );
        assert!(s.model().space_types().is_empty(), "erst mit OK");
        let ok = c.foot_buttons(&t, &WIN)[2].1;
        let out = klick(&mut c, &mut s, ok);
        assert!(out.closed && out.applied);
        let m = s.model();
        assert_eq!(m.space_type(KITCHEN_SPACE_GUID), Some(&k));
        assert!(m.type_by_guid(WALL_TILES_TYPE_GUID).is_some());
        assert!(m.check().is_empty(), "{:?}", m.check());
        // ein Schritt zurück: wieder ohne Vorlage und ohne Ausbautypen
        s.undo();
        assert!(s.model().space_types().is_empty());
        assert!(s.model().type_by_guid(WALL_TILES_TYPE_GUID).is_none());
    }

    /// Name, Neu, Duplizieren, Löschen; ungültige Fliesenhöhe bleibt
    /// stehen und meldet sich.
    #[test]
    fn eigene_vorlagen() {
        let t = Theme::dark();
        let mut s = Scene::with_model(Model::with_seed(7));
        let mut c = Catalog::open(&s, None);
        let tab = c.tab_rect(&t, &WIN, Tab::Rooms);
        klick(&mut c, &mut s, tab);
        let [neu, dup, del] = c.list_buttons(&t, &WIN);
        klick(&mut c, &mut s, neu.1);
        let g = c.room_sel.unwrap();
        assert_eq!(c.work.space_type(g).unwrap().name, "Raum");
        let r = c.room_field_rect(&t, &WIN, FieldId::RoomName).unwrap();
        klick(&mut c, &mut s, r);
        tippen(&mut c, &mut s, "Ankleide");
        assert_eq!(c.work.space_type(g).unwrap().name, "Ankleide");
        // ein Name, den es schon gibt
        klick(&mut c, &mut s, r);
        tippen(&mut c, &mut s, "Bad");
        assert_eq!(c.work.space_type(g).unwrap().name, "Ankleide");
        klick(&mut c, &mut s, dup.1);
        let kopie = c.room_sel.unwrap();
        assert_eq!(c.work.space_type(kopie).unwrap().name, "Ankleide (Kopie)");
        klick(&mut c, &mut s, del.1);
        assert!(c.work.space_type(kopie).is_none());
        // Bad: Fliesenhöhe außerhalb
        c.room_sel = Some(BATH_SPACE_GUID);
        let r = c.room_field_rect(&t, &WIN, FieldId::TilesHeight).unwrap();
        klick(&mut c, &mut s, r);
        tippen(&mut c, &mut s, "9");
        assert!(c.work.space_type(BATH_SPACE_GUID).is_none());
        assert_eq!(parse_tiles("9").unwrap_err(), "Fliesenhöhe 0,10 bis 4,00 m");
        assert_eq!(parse_tiles("1,2"), Ok(1200.0));
        // Werksvorlage wieder löschen: steht in alter Fassung da
        c.room_sel = Some(LIVING_SPACE_GUID);
        waehlen(&mut c, &mut s, ComboId::RoomGroup, "NUF 2");
        assert_eq!(
            c.work.space_type(LIVING_SPACE_GUID).unwrap().group,
            SpaceGroup::Nuf2
        );
        klick(&mut c, &mut s, del.1);
        assert_eq!(c.raum().unwrap().0.group, SpaceGroup::Nuf1);
        assert!(c.work.check().is_empty(), "{:?}", c.work.check());
    }

    /// Eine Änderung wird auf der Kopie geprüft, bevor etwas ins Projekt
    /// kommt: abgelehnt legt sie nichts an; angenommen kommen nur die
    /// Ausbautypen, auf die die Vorlage danach verweist (Review #85).
    #[test]
    fn aenderung_erst_pruefen_dann_anlegen() {
        let s = Scene::with_model(Model::with_seed(7));
        let mut c = Catalog::open(&s, None);
        let vorher = (c.work.layer_sets().len(), c.work.materials().len());
        c.room_sel = Some(BATH_SPACE_GUID);
        // Bodenbelag mit einem Typ anderer Art: abgelehnt, nichts angelegt
        assert!(!c.room_change(|r| r.floor = Some(WALL_TILES_TYPE_GUID)));
        assert!(c.message.as_deref().unwrap().contains("kein Typ"));
        assert!(c.work.space_types().is_empty());
        assert_eq!(
            (c.work.layer_sets().len(), c.work.materials().len()),
            vorher
        );
        // Bad mit Parkett: der Fliesenboden des Werksbads kommt nicht mit
        assert!(c.room_change(|r| r.floor = Some(PARQUET_TYPE_GUID)));
        assert!(c.message.is_none());
        let bad = c.work.space_type(BATH_SPACE_GUID).unwrap();
        assert_eq!(bad.floor, Some(PARQUET_TYPE_GUID));
        assert!(c.work.type_by_guid(PARQUET_TYPE_GUID).is_some());
        assert!(c.work.type_by_guid(WET_FLOOR_TYPE_GUID).is_none());
        assert!(c.work.check().is_empty(), "{:?}", c.work.check());
    }
}
