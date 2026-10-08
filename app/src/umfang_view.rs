//! Umfang über den Blättern (KA-1, architektur/paket-ka1.md): Geschoss-Chips
//! und ab zwei Gebäuden das Gebäudefeld. Hier wird nichts gerechnet: das
//! Blatt liest seine Liste mit [`crate::scene::Scene::schedule_in`] im
//! [`Umfang`], den diese Leiste liefert. KA-2 und KA-4 zeigen dieselbe
//! Leiste über ihren Blättern.
//!
//! Der Umfang gilt für die Sitzung. Er steht nicht in der Datei, nicht in
//! `einstellungen.txt` und ist kein Rückgängig-Schritt.

use crate::scene::FOUNDATION_NAME;
use sk_model::qto::Umfang;
use sk_model::{BuildingId, LevelKind, Model, StoreyId};
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::time::Instant;

/// Zeile der Chips über der Liste: Pille 24 dip und Luft darunter.
pub const ROW: f32 = 34.0;
const CHIP_H: f32 = 24.0;
const CHIP_PAD: f32 = 12.0;
const CHIP_GAP: f32 = 6.0;
const CHIP_PX: f32 = 11.0;
/// Gebäudefeld vor den Chips ab zwei Gebäuden (dip) und Zeilen seiner Liste.
const FIELD_W: f32 = 150.0;
const FIELD_ROW: f32 = 26.0;
/// Weite des Wackelns, wenn der letzte Chip abgewählt werden soll (dip).
const WOBBLE_DIP: f32 = 3.0;

/// Ein Geschoss-Chip: Name und die Geschosse dahinter (bei „Projekt“ alle
/// gleichnamigen Geschosse der Gebäude und die losen).
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub name: String,
    pub geschosse: Vec<StoreyId>,
}

/// Kurzname eines Geschosses wie im Bogen: „Fundament“, „EG“, „OG“.
fn kurzname(m: &Model, id: StoreyId) -> String {
    match m.storey(id) {
        Some(s) if s.kind == LevelKind::Foundation => FOUNDATION_NAME.into(),
        Some(s) => s.short.clone(),
        None => String::new(),
    }
}

/// Gebäude des Modells in ihrer Reihenfolge.
pub fn gebaeude(m: &Model) -> Vec<BuildingId> {
    m.buildings().iter().map(|(id, _)| id).collect()
}

/// Chips in der Reihenfolge des Geschossbogens, von unten nach oben: die
/// Geschosse des Gebäudes `g`, bei `None` (Projekt) ein Chip je Name über
/// alle Gebäude und die losen Geschosse.
pub fn umfang_chips(m: &Model, g: Option<BuildingId>) -> Vec<Chip> {
    let mut st: Vec<(StoreyId, f64)> = m
        .storeys()
        .iter()
        .filter(|(_, s)| g.is_none() || s.building == g)
        .map(|(id, s)| (id, s.elevation))
        .collect();
    st.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out: Vec<Chip> = Vec::new();
    for (id, _) in st {
        let name = kurzname(m, id);
        match out.iter_mut().find(|c| c.name == name) {
            Some(c) => c.geschosse.push(id),
            None => out.push(Chip {
                name,
                geschosse: vec![id],
            }),
        }
    }
    out
}

/// Einträge des Gebäudefelds ab zwei Gebäuden: „Projekt“, dann die Gebäude
/// mit ihren Geschossen (leise daneben). Leer bei höchstens einem Gebäude.
pub fn feld(m: &Model) -> Vec<(Option<BuildingId>, String, String)> {
    let gb = gebaeude(m);
    if gb.len() < 2 {
        return Vec::new();
    }
    let mut out = vec![(None, "Projekt".to_string(), String::new())];
    for g in gb {
        let name = m.building(g).map_or(String::new(), |b| b.name.clone());
        let geschosse = umfang_chips(m, Some(g))
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        out.push((Some(g), name, geschosse));
    }
    out
}

/// Bringt den Umfang auf den Modellstand (paket-ka1.md §3): gelöschte
/// Geschosse fallen aus `ohne`, neue sind gewählt, außer ihr Chip ist
/// abgewählt; ein gelöschtes Gebäude gilt als Projekt; bei höchstens einem
/// Gebäude gibt es kein Gebäudefeld und es gilt Projekt. Ist kein Chip mehr
/// gewählt, gelten alle. `true`, wenn sich etwas ändert.
pub fn bereinigen(m: &Model, u: &mut Umfang) -> bool {
    let vorher = u.clone();
    u.ohne.retain(|s| m.storey(*s).is_some());
    let gb = gebaeude(m);
    if gb.len() < 2 || u.gebaeude.is_some_and(|g| !gb.contains(&g)) {
        u.gebaeude = None;
    }
    let chips = umfang_chips(m, u.gebaeude);
    if !chips.is_empty() && chips.iter().all(|c| !an(u, c)) {
        u.ohne.clear();
    }
    // Ein abgewählter Chip lässt alle seine Geschosse weg, auch ein neues
    // Geschoss gleichen Namens im Projekt (Liste wie Chip)
    let aus: Vec<StoreyId> = chips
        .iter()
        .filter(|c| !an(u, c))
        .flat_map(|c| c.geschosse.iter().copied())
        .filter(|s| !u.ohne.contains(s))
        .collect();
    u.ohne.extend(aus);
    *u != vorher
}

/// Ist der Chip gewählt? Ein Chip über mehrere Geschosse ist gewählt, wenn
/// keines davon abgewählt ist.
pub fn an(u: &Umfang, c: &Chip) -> bool {
    c.geschosse.iter().all(|s| !u.ohne.contains(s))
}

/// Sind alle Chips gewählt?
pub fn alle_an(u: &Umfang, chips: &[Chip]) -> bool {
    chips.iter().all(|c| an(u, c))
}

/// „Alle“: jeden Chip wählen.
pub fn alle(u: &mut Umfang) {
    u.ohne.clear();
}

/// Klick auf Chip `i` (Bedienbarkeit 3.1): Sind alle gewählt, zeigt er nur
/// diesen; danach nimmt jeder Klick einen dazu oder weg. Strg+Klick wählt
/// nur diesen. `false`: abgelehnt, weil es der letzte gewählte Chip ist
/// (er wackelt), oder `i` gibt es nicht.
pub fn klick(u: &mut Umfang, chips: &[Chip], i: usize, strg: bool) -> bool {
    let Some(c) = chips.get(i) else {
        return false;
    };
    let nur = |u: &mut Umfang| {
        u.ohne = chips
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(_, c)| c.geschosse.iter().copied())
            .collect();
    };
    if strg || alle_an(u, chips) {
        nur(u);
        return true;
    }
    if an(u, c) {
        if chips.iter().filter(|c| an(u, c)).count() <= 1 {
            return false;
        }
        u.ohne.extend(c.geschosse.iter().copied());
    } else {
        u.ohne.retain(|s| !c.geschosse.contains(s));
    }
    true
}

/// Tooltip am Chip (paket-ka1.md §1), zweite Zeile mit Strg+Klick.
pub fn tooltip(u: &Umfang, chips: &[Chip], i: usize) -> String {
    let Some(c) = chips.get(i) else {
        return String::new();
    };
    let erste = if alle_an(u, chips) {
        format!("Nur {} zeigen", c.name)
    } else if an(u, c) {
        format!("{} weglassen", c.name)
    } else {
        format!("{} dazunehmen", c.name)
    };
    format!("{erste}\nStrg+Klick: nur dieses Geschoss")
}

/// Name des Umfangs im Gebäudefeld und in der Kopfzeile: das Gebäude, bei
/// einem einzigen Gebäude dieses, sonst „Projekt“.
pub fn umfang_name(m: &Model, u: &Umfang) -> String {
    let gb = gebaeude(m);
    let g = u.gebaeude.or(match gb.as_slice() {
        [g] => Some(*g),
        _ => None,
    });
    match g.and_then(|g| m.building(g)) {
        Some(b) => b.name.clone(),
        None => "Projekt".into(),
    }
}

/// Name im Gebäudefeld: der gewählte Eintrag, sonst „Projekt“.
pub fn umfang_name_von(feld: &[(Option<BuildingId>, String, String)], u: &Umfang) -> String {
    feld.iter()
        .find(|(g, _, _)| *g == u.gebaeude)
        .map_or("Projekt".into(), |(_, n, _)| n.clone())
}

/// Kopfzeile unter dem Titel und erste Zeile der Tabelle (paket-ka1.md §3):
/// „Gebäude 1 · alle Geschosse · Stand 08.10.2026, 11:34“ bzw. mit den
/// gewählten Geschossen „EG + OG“. `uhr`: Jahr, Monat, Tag, Stunde, Minute.
pub fn umfang_text(m: &Model, u: &Umfang, uhr: (u16, u8, u8, u8, u8)) -> String {
    let chips = umfang_chips(m, u.gebaeude);
    let geschosse = if alle_an(u, &chips) {
        "alle Geschosse".to_string()
    } else {
        chips
            .iter()
            .filter(|c| an(u, c))
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let (y, mo, d, h, mi) = uhr;
    format!(
        "{} · {geschosse} · Stand {d:02}.{mo:02}.{y}, {h:02}:{mi:02}",
        umfang_name(m, u)
    )
}

/// Rechteck in Fensterpixeln: x, y, Breite, Höhe.
pub type Rect = (f32, f32, f32, f32);

/// Teil der Leiste unter der Maus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hot {
    Chip(usize),
    /// „Alle“ hinter den Chips.
    All,
    /// Gebäudefeld und, wenn offen, ein Eintrag seiner Liste (0 = Projekt).
    Field,
    FieldItem(usize),
}

/// Wo die Leiste im Blatt steht: linker Rand und Oberkante der Pillen (px),
/// Maßstab.
#[derive(Clone, Copy, Debug)]
pub struct Lage {
    pub x0: f32,
    pub y: f32,
    pub s: f32,
}

/// Lage der Teile (px): Gebäudefeld (ab zwei Gebäuden), Chips und „Alle“
/// (nur wenn nicht alle gewählt sind).
pub struct ChipLayout {
    pub field: Option<Rect>,
    pub chips: Vec<Rect>,
    pub all: Option<Rect>,
}

/// Die Umfangsleiste eines Blatts (Einstellungen §3 KA-1 Punkte 5–6): Zustand,
/// Lage, Klicks und Zeichnen. Mengen- und Kostenblatt haben je eine; das
/// Fenster gibt den Umfang beim Kartenwechsel weiter.
#[derive(Default)]
pub struct Leiste {
    pub umfang: Umfang,
    chips: Vec<Chip>,
    field: Vec<(Option<BuildingId>, String, String)>,
    field_open: bool,
    wobble: Option<(usize, Instant)>,
    pub hot: Option<Hot>,
    /// Summe hinter dem Namen je Chip (Reiter Kosten), sonst leer.
    pub summen: Vec<String>,
}

impl Leiste {
    /// An das Modell angleichen. `true`, wenn sich Umfang, Chips oder Feld
    /// geändert haben.
    pub fn sync(&mut self, m: &Model) -> bool {
        let mut changed = bereinigen(m, &mut self.umfang);
        let chips = umfang_chips(m, self.umfang.gebaeude);
        let field = feld(m);
        if chips != self.chips || field != self.field {
            self.chips = chips;
            self.field = field;
            self.field_open &= !self.field.is_empty();
            changed = true;
        }
        changed
    }

    pub fn chips(&self) -> &[Chip] {
        &self.chips
    }

    /// Name des Umfangs im Gebäudefeld.
    pub fn feld_name(&self) -> String {
        umfang_name_von(&self.field, &self.umfang)
    }

    /// Wackelt gerade der Chip `i`?
    #[cfg(test)]
    pub fn wobbling(&self) -> Option<usize> {
        self.wobble.map(|w| w.0)
    }

    fn text_w(fonts: &Fonts, text: &str, px: f32, bold: bool) -> f32 {
        let f = if bold { fonts.bold.as_ref() } else { None }.or(fonts.regular.as_ref());
        f.map_or(text.chars().count() as f32 * px * 0.6, |f| {
            f.width(text, px)
        })
    }

    /// Breite des Chip-Inhalts: Name fett, dahinter leise die Summe.
    fn chip_text_w(&self, fonts: &Fonts, i: usize, s: f32) -> f32 {
        let px = CHIP_PX * s;
        let name = Self::text_w(fonts, &self.chips[i].name, px, true);
        match self.summen.get(i).filter(|x| !x.is_empty()) {
            Some(sum) => name + 6.0 * s + Self::text_w(fonts, sum, px, false),
            None => name,
        }
    }

    pub fn layout(&self, fonts: &Fonts, at: Lage) -> ChipLayout {
        let s = at.s;
        let h = CHIP_H * s;
        let mut x = at.x0;
        let field = (!self.field.is_empty()).then(|| {
            let r = (x, at.y, FIELD_W * s, h);
            x += (FIELD_W + 2.0 * CHIP_GAP) * s;
            r
        });
        let chips = (0..self.chips.len())
            .map(|i| {
                let w = self.chip_text_w(fonts, i, s) + 2.0 * CHIP_PAD * s;
                let r = (x, at.y, w, h);
                x += w + CHIP_GAP * s;
                r
            })
            .collect();
        let all = (!alle_an(&self.umfang, &self.chips)).then(|| {
            let w = Self::text_w(fonts, "Alle", CHIP_PX * s, true) + 8.0 * s;
            (x + 4.0 * s, at.y, w, h)
        });
        ChipLayout { field, chips, all }
    }

    /// Einträge der offenen Liste des Gebäudefelds (px), unter dem Feld.
    fn field_items(&self, fonts: &Fonts, at: Lage) -> Vec<Rect> {
        if !self.field_open {
            return Vec::new();
        }
        let Some((fx, fy, _, fh)) = self.layout(fonts, at).field else {
            return Vec::new();
        };
        let s = at.s;
        let w = self
            .field
            .iter()
            .map(|(_, n, g)| {
                Self::text_w(fonts, n, CHIP_PX * s, true)
                    + Self::text_w(fonts, g, 10.0 * s, false)
                    + 40.0 * s
            })
            .fold(FIELD_W * s, f32::max);
        (0..self.field.len())
            .map(|i| {
                (
                    fx,
                    fy + fh + 4.0 * s + i as f32 * FIELD_ROW * s,
                    w,
                    FIELD_ROW * s,
                )
            })
            .collect()
    }

    /// Liegt etwas über der Liste (offenes Gebäudefeld) oder wackelt ein
    /// Chip? Dann zeichnet das Fenster ganz statt nur einzelner Zeilen.
    pub fn overlay_open(&self) -> bool {
        self.field_open || self.wobble.is_some()
    }

    pub fn field_open(&self) -> bool {
        self.field_open
    }

    /// Schließt die Liste des Gebäudefelds; `true`, wenn sie offen war.
    pub fn close_field(&mut self) -> bool {
        std::mem::take(&mut self.field_open)
    }

    pub fn hit(&self, fonts: &Fonts, at: Lage, x: f32, y: f32) -> Option<Hot> {
        let inside = |(rx, ry, rw, rh): Rect| x >= rx && x < rx + rw && y >= ry && y < ry + rh;
        if let Some(i) = self.field_items(fonts, at).into_iter().position(inside) {
            return Some(Hot::FieldItem(i));
        }
        let l = self.layout(fonts, at);
        if l.field.is_some_and(inside) {
            return Some(Hot::Field);
        }
        if let Some(i) = l.chips.iter().position(|r| inside(*r)) {
            return Some(Hot::Chip(i));
        }
        l.all.filter(|r| inside(*r)).map(|_| Hot::All)
    }

    /// Klick auf einen Teil der Leiste (Bedienbarkeit 3.1).
    pub fn click(&mut self, hot: Hot, ctrl: bool) {
        match hot {
            Hot::Chip(i) => {
                self.field_open = false;
                if !klick(&mut self.umfang, &self.chips, i, ctrl) {
                    self.wobble = Some((i, Instant::now()));
                }
            }
            Hot::All => {
                self.field_open = false;
                alle(&mut self.umfang);
            }
            Hot::Field => self.field_open = !self.field_open,
            Hot::FieldItem(i) => {
                self.field_open = false;
                if let Some(&(g, _, _)) = self.field.get(i) {
                    if g != self.umfang.gebaeude {
                        self.umfang.gebaeude = g;
                        // Chips des Gebäudes bringt das nächste `sync`
                        self.chips.clear();
                    }
                }
            }
        }
    }

    /// Tooltip an einem Chip.
    pub fn tip(&self, hot: Hot) -> Option<String> {
        match hot {
            Hot::Chip(i) => Some(tooltip(&self.umfang, &self.chips, i)),
            _ => None,
        }
    }

    /// Wackeln weiterführen; `true`, solange es läuft.
    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        let Some((_, at)) = self.wobble else {
            return false;
        };
        let d = std::time::Duration::from_secs_f32(t.size.anim_ms.max(0.0) / 1000.0);
        if t.size.anim_ms > 0.0 && now.duration_since(at) < d {
            true
        } else {
            self.wobble = None;
            false
        }
    }

    /// Versatz des wackelnden Chips (px).
    fn wobble_dx(&self, i: usize, t: &Theme, s: f32, now: Instant) -> f32 {
        match self.wobble {
            Some((j, at)) if j == i && t.size.anim_ms > 0.0 => {
                let ms = now.duration_since(at).as_secs_f32() * 1000.0;
                let d = t.size.anim_ms.max(1.0);
                if ms >= d {
                    return 0.0;
                }
                let k = 1.0 - ms / d;
                (ms / d * std::f32::consts::TAU * 3.0).sin() * k * WOBBLE_DIP * s
            }
            _ => 0.0,
        }
    }

    /// Gebäudefeld, Chips und „Alle“: an Akzent hell mit Rand Akzent und
    /// fettem Text, aus `sheet_bg` mit Rand `sheet_rule` und gedämpftem Text,
    /// Überfahren weiß mit dunklerem Rand.
    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, at: Lage, now: Instant) {
        let s = at.s;
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let px = CHIP_PX * s;
        let l = self.layout(fonts, at);
        let line = s.max(1.0);
        let framed = |c: &mut Canvas, (x, y, w, h): Rect, r: f32, fill: Rgba, edge: Rgba| {
            let mut p = Path::new();
            p.rounded_rect(x, y, w, h, r);
            c.fill(&p, edge);
            let mut p = Path::new();
            p.rounded_rect(x + line, y + line, w - 2.0 * line, h - 2.0 * line, r - line);
            c.fill(&p, fill);
        };
        if let Some((x, y, w, h)) = l.field {
            let hot = self.hot == Some(Hot::Field) || self.field_open;
            let edge = if hot { u.sheet_hint } else { u.sheet_rule };
            framed(
                c,
                (x, y, w, h),
                t.size.corner_radius * s,
                u.sheet_card,
                edge,
            );
            if let Some(f) = regular {
                let base = y + (h + f.cap_height(px)) * 0.5;
                f.draw(c, &self.feld_name(), px, x + 10.0 * s, base, u.sheet_text);
                f.draw(c, "▾", px, x + w - 18.0 * s, base, u.sheet_text_dim);
            }
        }
        for (i, (ch, r)) in self.chips.iter().zip(&l.chips).enumerate() {
            let on = an(&self.umfang, ch);
            let hover = self.hot == Some(Hot::Chip(i));
            let r = (r.0 + self.wobble_dx(i, t, s, now), r.1, r.2, r.3);
            let (fill, edge, font, col) = if on {
                (u.sheet_select, u.accent, bold, u.sheet_text)
            } else if hover {
                (u.sheet_card, u.sheet_hint, regular, u.sheet_text)
            } else {
                (u.sheet_bg, u.sheet_rule, regular, u.sheet_text_dim)
            };
            let fill = if on && hover { u.sheet_hover } else { fill };
            framed(c, r, r.3 * 0.5, fill, edge);
            let Some(f) = font else { continue };
            let base = r.1 + (r.3 + f.cap_height(px)) * 0.5;
            let sum = self.summen.get(i).filter(|x| !x.is_empty());
            let name_w = f.width(&ch.name, px);
            let sum_w = match (sum, regular) {
                (Some(x), Some(rf)) => 6.0 * s + rf.width(x, px),
                _ => 0.0,
            };
            let x = r.0 + (r.2 - name_w - sum_w) * 0.5;
            f.draw(c, &ch.name, px, x, base, col);
            if let (Some(sum), Some(rf)) = (sum, regular) {
                rf.draw(c, sum, px, x + name_w + 6.0 * s, base, u.sheet_text_dim);
            }
        }
        if let (Some((x, y, _, h)), Some(f)) = (l.all, bold) {
            let col = crate::cards::verweis(u, self.hot == Some(Hot::All));
            f.draw(
                c,
                "Alle",
                px,
                x + 4.0 * s,
                y + (h + f.cap_height(px)) * 0.5,
                col,
            );
        }
    }

    /// Offene Liste des Gebäudefelds über dem Blatt.
    pub fn paint_field_list(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, at: Lage) {
        let items = self.field_items(fonts, at);
        let (Some(&(x, y, w, _)), Some(&(_, ly, _, lh))) = (items.first(), items.last()) else {
            return;
        };
        let s = at.s;
        let u = &t.ui;
        let h = ly + lh - y;
        let r = t.size.corner_radius * s;
        let mut p = Path::new();
        p.rounded_rect(x - s, y - s + 2.0 * s, w + 2.0 * s, h + 2.0 * s, r);
        c.fill(&p, u.shadow);
        let mut p = Path::new();
        p.rounded_rect(x - s, y - s, w + 2.0 * s, h + 2.0 * s, r);
        c.fill(&p, u.sheet_rule);
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, r);
        c.fill(&p, u.sheet_card);
        let px = CHIP_PX * s;
        for (i, ((g, name, geschosse), (ix, iy, iw, ih))) in
            self.field.iter().zip(items).enumerate()
        {
            if self.hot == Some(Hot::FieldItem(i)) {
                c.fill_rect(ix, iy, iw, ih, u.sheet_hover);
            }
            let chosen = *g == self.umfang.gebaeude;
            let font = if chosen { fonts.bold.as_ref() } else { None }.or(fonts.regular.as_ref());
            let Some(f) = font else { continue };
            let base = iy + (ih + f.cap_height(px)) * 0.5;
            f.draw(c, name, px, ix + 10.0 * s, base, u.sheet_text);
            if let Some(r) = fonts.regular.as_ref() {
                let nx = ix + 10.0 * s + f.width(name, px) + 10.0 * s;
                r.draw(c, geschosse, 10.0 * s, nx, base, u.sheet_text_dim);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UHR: (u16, u8, u8, u8, u8) = (2026, 10, 8, 11, 34);

    /// Standardhaus RH-1 (Fundament, EG, OG, ein Gebäude).
    fn haus() -> Model {
        sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model
    }

    fn namen(c: &[Chip]) -> Vec<&str> {
        c.iter().map(|c| c.name.as_str()).collect()
    }

    /// Abnahme 1–3 (paket-ka1.md §5): Chips von unten nach oben, Klickfolge,
    /// letzter Chip bleibt, Kopfzeile.
    #[test]
    fn chips_und_klickfolge() {
        let m = haus();
        let mut u = Umfang::projekt();
        bereinigen(&m, &mut u);
        let c = umfang_chips(&m, u.gebaeude);
        assert_eq!(namen(&c), ["Fundament", "EG", "OG"]);
        assert!(alle_an(&u, &c));
        assert!(umfang_text(&m, &u, UHR).ends_with(" · alle Geschosse · Stand 08.10.2026, 11:34"));
        assert_eq!(
            tooltip(&u, &c, 1),
            "Nur EG zeigen\nStrg+Klick: nur dieses Geschoss"
        );
        // EG: nur EG
        assert!(klick(&mut u, &c, 1, false));
        assert_eq!(c.iter().filter(|c| an(&u, c)).count(), 1);
        assert!(an(&u, &c[1]));
        assert_eq!(
            tooltip(&u, &c, 2),
            "OG dazunehmen\nStrg+Klick: nur dieses Geschoss"
        );
        assert!(umfang_text(&m, &u, UHR).contains(" · EG · Stand"));
        // letzter gewählter Chip bleibt
        assert!(!klick(&mut u, &c, 1, false));
        assert!(an(&u, &c[1]));
        // OG dazu: EG + OG
        assert!(klick(&mut u, &c, 2, false));
        assert!(umfang_text(&m, &u, UHR).contains(" · EG + OG · Stand"));
        assert_eq!(
            tooltip(&u, &c, 2),
            "OG weglassen\nStrg+Klick: nur dieses Geschoss"
        );
        // EG weg: nur OG
        assert!(klick(&mut u, &c, 1, false));
        assert!(!an(&u, &c[1]) && an(&u, &c[2]) && !an(&u, &c[0]));
        // Strg+Klick auf Fundament: nur Fundament
        assert!(klick(&mut u, &c, 0, true));
        assert!(an(&u, &c[0]) && !an(&u, &c[1]) && !an(&u, &c[2]));
        // Alle
        alle(&mut u);
        assert!(alle_an(&u, &c));
    }

    /// Abnahme 3 (Teil): Ist kein Chip mehr gewählt, gelten alle.
    #[test]
    fn bereinigen_ohne_gewaehlte() {
        let m = haus();
        let mut u = Umfang::projekt();
        assert!(!bereinigen(&m, &mut u));
        // alle abgewählt geht nicht
        let c = umfang_chips(&m, None);
        u.ohne = c.iter().flat_map(|c| c.geschosse.clone()).collect();
        assert!(bereinigen(&m, &mut u));
        assert!(alle_an(&u, &c));
    }

    /// Abnahme 4 (Summenprobe, Regel 96): die Geschosse einzeln ergeben
    /// zusammen die Mengen und Summen aller Geschosse, je Baustoff, Gewerk
    /// und Kostengruppe auf die mm-Einheit.
    #[test]
    fn summenprobe_je_geschoss() {
        use std::collections::HashMap;
        let m = haus();
        let ganz = sk_model::qto::schedule(&m);
        let c = umfang_chips(&m, None);
        type Summen = HashMap<String, f64>;
        fn summen(s: &sk_model::qto::Schedule, out: &mut Summen) {
            let mut add = |k: String, v: f64| *out.entry(k).or_default() += v;
            for b in &s.buildings {
                for x in &b.by_material {
                    add(format!("B{:?}v", x.material), x.volume);
                    add(format!("B{:?}a", x.material), x.area.unwrap_or(0.0));
                    add(format!("B{:?}l", x.material), x.length.unwrap_or(0.0));
                }
                for x in &b.by_trade {
                    add(format!("G{:?}v", x.trade), x.volume);
                    add(format!("G{:?}a", x.trade), x.area.unwrap_or(0.0));
                    add(format!("G{:?}l", x.trade), x.length.unwrap_or(0.0));
                    add(format!("G{:?}n", x.trade), x.rows.len() as f64);
                }
                for x in &b.by_kg {
                    add(format!("K{}v", x.kg), x.volume);
                    add(format!("K{}n", x.kg), x.rows.len() as f64);
                }
            }
        }
        let mut soll = Summen::new();
        summen(&ganz, &mut soll);
        let mut ist = Summen::new();
        for i in 0..c.len() {
            let mut u = Umfang::projekt();
            assert!(klick(&mut u, &c, i, true));
            let t = ganz.restrict(&m, &u);
            assert!(t
                .buildings
                .iter()
                .flat_map(|b| &b.storeys)
                .all(|s| c[i].geschosse.contains(&s.id)));
            summen(&t, &mut ist);
        }
        assert!(soll.len() > 10, "{soll:?}");
        assert_eq!(soll.len(), ist.len());
        for (k, v) in &soll {
            assert!((ist[k] - v).abs() < 1.0, "{k}: {} statt {v}", ist[k]);
        }
    }

    /// Abnahme 5 und 6: Zwei Gebäude geben das Gebäudefeld; bei „Projekt“
    /// ein Chip je Name für beide; ein neues Gebäude kommt gewählt dazu, ein
    /// gelöschtes nimmt seine Geschosse und die Wahl im Feld mit.
    #[test]
    fn zwei_gebaeude() {
        let mut m = haus();
        let mut u = Umfang::projekt();
        assert!(feld(&m).is_empty());
        // nur EG, dann kommt ein Nebengebäude mit Gründung und EG dazu
        let c = umfang_chips(&m, None);
        assert!(klick(&mut u, &c, 1, false));
        m.begin("Gebäude");
        let b2 = m.add_building(1);
        m.commit();
        assert!(
            bereinigen(&m, &mut u),
            "Gründung des Nebengebäudes bleibt weg"
        );
        let f = feld(&m);
        let namen_feld: Vec<&str> = f.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(namen_feld.len(), 3);
        assert_eq!(namen_feld[0], "Projekt");
        assert_eq!(
            f[2],
            (
                Some(b2),
                namen_feld[2].to_string(),
                "Fundament · EG".to_string()
            )
        );
        let c = umfang_chips(&m, None);
        assert_eq!(namen(&c), ["Fundament", "EG", "OG"]);
        assert_eq!(c[1].geschosse.len(), 2, "ein EG-Chip für beide");
        assert!(an(&u, &c[1]) && !an(&u, &c[0]) && !an(&u, &c[2]));
        let t = sk_model::qto::schedule(&m).restrict(&m, &u);
        assert_eq!(t.buildings.len(), 2);
        // das leere Nebengebäude hat keine Mengenzeilen
        assert!(t
            .buildings
            .iter()
            .flat_map(|b| &b.storeys)
            .all(|s| c[1].geschosse.contains(&s.id)));
        assert_eq!(t.buildings[0].storeys.len(), 1);
        assert!(umfang_text(&m, &u, UHR).starts_with("Projekt · EG · Stand"));
        // Abwahl von EG nimmt beide heraus (OG dazu, dann EG weg)
        assert!(klick(&mut u, &c, 2, false));
        assert!(klick(&mut u, &c, 1, false));
        let t = sk_model::qto::schedule(&m).restrict(&m, &u);
        assert!(t
            .buildings
            .iter()
            .all(|b| b.storeys.iter().all(|s| c[2].geschosse.contains(&s.id))));
        // Nebengebäude gewählt: seine Chips; gelöscht: wieder Projekt
        let mut u = Umfang::gebaeude(b2);
        assert!(!bereinigen(&m, &mut u));
        assert_eq!(namen(&umfang_chips(&m, u.gebaeude)), ["Fundament", "EG"]);
        assert_eq!(umfang_name_von(&f, &u), f[2].1);
        let c2 = umfang_chips(&m, u.gebaeude);
        assert!(klick(&mut u, &c2, 1, false));
        m.begin("Löschen");
        assert!(m.remove_building(b2));
        m.commit();
        assert!(bereinigen(&m, &mut u));
        assert_eq!(u, Umfang::projekt());
        assert!(feld(&m).is_empty());
    }
}
