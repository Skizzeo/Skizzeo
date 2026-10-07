//! Auswahlfelder und Vorschauen der Darstellung (Paket 5-0), gemeinsam
//! für das Einstellungsfenster und das Materialfenster: Kacheln der Listen
//! (Linientyp, Schraffur, Oberfläche, Baustoff), Bildchen und Texte der
//! Auswahllisten eines Baustoffs (Schraffur, Stifte, Oberfläche) und die
//! großen Vorschauen (Schnittfläche, Würfel).
//!
//! Die Bilder rechnen mit denselben Formeln wie die Grafikkarte
//! ([`sk_render::fill_color`], [`sk_render::dash_ink`]). [`Tiles`] hält sie
//! je Stand von Modell, Schema und Skalierung vor (Review 3b M1–M3):
//! gezeichnet wird nur, was sich geändert hat und zu sehen ist.

use crate::draw_table::{look_rows, mat_look};
use crate::prefs::num;
use sk_model::proctex::{self, Pattern};
use sk_model::{
    Dash, FillId, LineTypeId, MaterialDisplay, MaterialId, Model, Pen, PenId, Surface, SurfaceId,
};
use sk_paint::{Canvas, Path, Rgba};
use sk_render::{DashPattern, SOLID};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Rect};

// --- Zwischenspeicher -------------------------------------------------------

/// Wofür eine Kachel steht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileKey {
    /// Linientyp als Strich in der Liste.
    LineType(LineTypeId),
    /// Schraffur in Stift 4 auf Stift 5 (Liste, Auswahlliste).
    Fill(FillId),
    /// Farbfeld einer Oberfläche.
    Surface(SurfaceId),
    /// Farbfeld eines Stifts.
    Pen(PenId),
    /// Schnittfläche eines Baustoffs in seinen Farben.
    Material(MaterialId),
}

/// Kacheln und große Vorschauen eines Fensters, gültig für einen Stand von
/// Modell (Revision), Schema und Skalierung; ändert sich einer, beginnt der
/// Speicher leer.
#[derive(Default)]
pub struct Tiles {
    /// Revision von Modell und Attributen, Skalierung (Bits).
    stamp: Option<(u64, u64, u32)>,
    theme: Option<Theme>,
    tiles: Vec<(TileKey, Canvas)>,
    /// Ausschnitte des Fensterbildes mit fertiger Vorschau.
    previews: Vec<Preview>,
    /// Leinwand der Listenzeilen vom letzten Bild ([`Canvas::reuse`]).
    rows: Option<Canvas>,
}

/// Gemerkte große Vorschau: Schlüssel, Lage (ganze Bildpunkte) und Bild.
struct Preview {
    key: (u8, TileKey),
    at: [i32; 4],
    img: Canvas,
    /// `None`: fertig gemalt. Sonst in der Mischfarbe gemalt, weil die
    /// Verbandstabelle noch im Hintergrund lief; der Zähler
    /// ([`proctex::bond_generation`]) von damals.
    waiting: Option<u64>,
    /// Bild in der Mischfarbe und Beginn, solange es über dem fertigen
    /// ausblendet (`anim_ms`).
    fade: Option<(Canvas, std::time::Instant)>,
}

impl std::fmt::Debug for Tiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tiles")
            .field("stamp", &self.stamp)
            .field("tiles", &self.tiles.len())
            .field("previews", &self.previews.len())
            .finish()
    }
}

impl Tiles {
    /// Gleicht den Speicher an Modell, Schema und Skalierung an.
    pub fn sync(&mut self, m: &Model, t: &Theme, s: f32) {
        let stamp = Some((m.revision(), m.attr().rev(), s.to_bits()));
        if self.stamp != stamp || self.theme.as_ref() != Some(t) {
            self.stamp = stamp;
            self.theme = Some(t.clone());
            self.tiles.clear();
            self.previews.clear();
        }
    }

    /// Kachel zu `key`, beim ersten Mal gemalt ([`make_tile`]); ohne
    /// Eintrag im Modell `None`. Vorher [`Tiles::sync`].
    pub fn get(&mut self, m: &Model, t: &Theme, s: f32, key: TileKey) -> Option<&Canvas> {
        let i = match self.tiles.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                self.tiles.push((key, make_tile(m, t, s, key)?));
                self.tiles.len() - 1
            }
        };
        Some(&self.tiles[i].1)
    }

    /// Große Vorschau im Bereich `r` von `c`: beim ersten Mal mit `paint`
    /// gemalt und der Ausschnitt gemerkt, danach nur kopiert (M2). `slot`
    /// unterscheidet Vorschauen desselben Eintrags. `ready` = `false`: `paint`
    /// malt vorläufig (Mischfarbe, Tabelle im Hintergrund); sobald `ready`
    /// kommt, wird neu gemalt und das vorläufige Bild blendet aus.
    pub fn preview(
        &mut self,
        c: &mut Canvas,
        slot: u8,
        key: TileKey,
        r: Rect,
        ready: bool,
        paint: impl FnOnce(&mut Canvas),
    ) {
        let (ox, oy) = c.origin();
        let at = [
            (r.x - ox).floor() as i32,
            (r.y - oy).floor() as i32,
            (r.x + r.w - ox).ceil() as i32,
            (r.y + r.h - oy).ceil() as i32,
        ];
        let (x0, y0, x1, y1) = (
            at[0] as f32 + ox,
            at[1] as f32 + oy,
            at[2] as f32 + ox,
            at[3] as f32 + oy,
        );
        let anim = self.theme.as_ref().map_or(0.0, |t| t.size.anim_ms);
        let found = self
            .previews
            .iter()
            .position(|p| p.key == (slot, key) && p.at == at);
        let mut old = None;
        if let Some(i) = found {
            let p = &mut self.previews[i];
            if p.waiting.is_none() || !ready {
                if p.waiting.is_some() {
                    // eine andere Tabelle wurde fertig: weiter warten
                    p.waiting = Some(proctex::bond_generation());
                }
                c.copy_rect_from(&p.img, x0, y0, x1, y1);
                if let Some((img, t0)) = &p.fade {
                    let f = t0.elapsed().as_secs_f32() * 1000.0 / anim.max(1.0);
                    if f < 1.0 {
                        c.blit_scaled(img, x0, y0, 1.0, 1.0 - crate::scene::ease_out(f));
                    } else {
                        p.fade = None;
                    }
                }
                return;
            }
            // Tabelle fertig: neu malen, das vorläufige Bild blendet aus
            if anim > 0.0 {
                old = Some((self.previews.remove(i).img, std::time::Instant::now()));
            }
        }
        paint(c);
        let (w, h) = ((at[2] - at[0]).max(0), (at[3] - at[1]).max(0));
        let mut img = Canvas::new(w as usize, h as usize);
        img.set_origin(x0, y0);
        img.copy_rect_from(c, x0, y0, x1, y1);
        if let Some((o, _)) = &old {
            c.blit_scaled(o, x0, y0, 1.0, 1.0);
        }
        self.previews.retain(|p| p.key != (slot, key));
        self.previews.push(Preview {
            key: (slot, key),
            at,
            img,
            waiting: (!ready).then(proctex::bond_generation),
            fade: old,
        });
    }

    /// Wartet eine Vorschau auf ihre Verbandstabelle oder blendet gerade
    /// ein? Dann fragt das Fenster in kurzen Abständen nach
    /// ([`Tiles::tick`]).
    pub fn busy(&self) -> bool {
        self.previews
            .iter()
            .any(|p| p.waiting.is_some() || p.fade.is_some())
    }

    /// Neu zeichnen, weil eine Tabelle fertig wurde oder eingeblendet wird.
    pub fn tick(&self) -> bool {
        let g = proctex::bond_generation();
        self.previews
            .iter()
            .any(|p| p.fade.is_some() || p.waiting.is_some_and(|w| w != g))
    }

    /// Leinwand für die Listenzeilen, durchsichtig in der Größe `w` × `h`;
    /// zurück mit [`Tiles::give_rows`] (M3).
    pub fn take_rows(&mut self, w: usize, h: usize) -> Canvas {
        let mut c = self.rows.take().unwrap_or_else(|| Canvas::new(0, 0));
        c.reuse(w, h);
        c
    }

    pub fn give_rows(&mut self, c: Canvas) {
        self.rows = Some(c);
    }
}

// --- Auswahllisten eines Baustoffs --------------------------------------------

/// Darstellungsverweis eines Baustoffs, den eine Auswahlliste setzt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Schraffur der Schnittfläche.
    Fill,
    /// Stift der Schraffur.
    Fg,
    /// Stift des Grunds.
    Bg,
    /// Oberfläche (3D).
    Surface,
}

/// Stifte nach Nummer (Auswahllisten).
fn pens_by_number(m: &Model) -> Vec<(PenId, Pen)> {
    let mut v: Vec<(PenId, Pen)> = m
        .attr()
        .pens()
        .iter()
        .map(|(id, p)| (id, p.clone()))
        .collect();
    v.sort_by_key(|p| p.1.number);
    v
}

/// Einträge einer Auswahlliste: Schlüssel und Text, in Listenreihenfolge.
fn pick_entries(m: &Model, pick: Pick) -> Vec<(TileKey, String)> {
    let a = m.attr();
    match pick {
        Pick::Fill => a
            .fills()
            .iter()
            .map(|(i, f)| (TileKey::Fill(i), f.name.clone()))
            .collect(),
        Pick::Fg | Pick::Bg => pens_by_number(m)
            .iter()
            .map(|(i, p)| (TileKey::Pen(*i), pen_label(p)))
            .collect(),
        Pick::Surface => a
            .surfaces()
            .iter()
            .map(|(i, o)| (TileKey::Surface(i), o.name.clone()))
            .collect(),
    }
}

/// Gewählter Eintrag von `d` für `pick`.
fn picked(d: &MaterialDisplay, pick: Pick) -> TileKey {
    match pick {
        Pick::Fill => TileKey::Fill(d.cut_fill),
        Pick::Fg => TileKey::Pen(d.cut_fg),
        Pick::Bg => TileKey::Pen(d.cut_bg),
        Pick::Surface => TileKey::Surface(d.surface),
    }
}

/// Offene Auswahlliste: Texte, Bildchen und der gewählte Eintrag.
pub fn pick_items(
    m: &Model,
    tiles: &mut Tiles,
    t: &Theme,
    s: f32,
    pick: Pick,
    d: &MaterialDisplay,
) -> (Vec<String>, Vec<Option<Canvas>>, usize) {
    tiles.sync(m, t, s);
    let list = pick_entries(m, pick);
    let cur = picked(d, pick);
    let sel = list.iter().position(|(k, _)| *k == cur).unwrap_or(0);
    let icons = list
        .iter()
        .map(|(k, _)| tiles.get(m, t, s, *k).cloned())
        .collect();
    (list.into_iter().map(|(_, n)| n).collect(), icons, sel)
}

/// Geschlossene Auswahlliste: Text und Bildchen des gewählten Eintrags,
/// „–“ ohne Eintrag.
pub fn pick_shown(
    m: &Model,
    tiles: &mut Tiles,
    t: &Theme,
    s: f32,
    pick: Pick,
    d: &MaterialDisplay,
) -> (String, Option<Canvas>) {
    tiles.sync(m, t, s);
    let a = m.attr();
    let key = picked(d, pick);
    let name = match key {
        TileKey::Fill(i) => a.fill(i).map(|f| f.name.clone()),
        TileKey::Pen(i) => a.pen(i).map(pen_label),
        TileKey::Surface(i) => a.surface(i).map(|o| o.name.clone()),
        _ => None,
    };
    match name {
        // Schraffur ohne Eintrag: nur der Strich; Stift und Oberfläche ohne
        // Bildchen
        None if pick == Pick::Fill => ("–".into(), tiles.get(m, t, s, key).cloned()),
        None => ("–".into(), None),
        Some(n) => (n, tiles.get(m, t, s, key).cloned()),
    }
}

/// Setzt in `d` den Eintrag `i` der Auswahlliste `pick`; `false`, wenn es
/// ihn nicht gibt.
pub fn pick_apply(m: &Model, pick: Pick, i: usize, d: &mut MaterialDisplay) -> bool {
    let Some((key, _)) = pick_entries(m, pick).into_iter().nth(i) else {
        return false;
    };
    match (pick, key) {
        (Pick::Fill, TileKey::Fill(f)) => d.cut_fill = f,
        (Pick::Fg, TileKey::Pen(p)) => d.cut_fg = p,
        (Pick::Bg, TileKey::Pen(p)) => d.cut_bg = p,
        (Pick::Surface, TileKey::Surface(o)) => d.surface = o,
        _ => return false,
    }
    true
}

// --- Bildchen und Vorschauen ------------------------------------------------

/// Breite der Spalte „Muster“ im Reiter „Linientypen“ (dip).
pub(crate) const LT_THUMB_W: f32 = 80.0;

/// Malt die Kachel zu `key` (Listen und Auswahllisten); `None`, wenn es den
/// Eintrag nicht gibt.
pub(crate) fn make_tile(m: &Model, t: &Theme, s: f32, key: TileKey) -> Option<Canvas> {
    let a = m.attr();
    let (tw, th) = (t.size.list_thumb_w * s, t.size.list_thumb_h * s);
    Some(match key {
        TileKey::LineType(id) => {
            let l = a.line_type(id)?;
            let w = 0.35 * t.px_per_mm * s;
            line_strip(m, t, &l.pattern, LT_THUMB_W * s, th, w, s)
        }
        TileKey::Fill(id) => {
            a.fill(id)?;
            fill_tile(m, t, id, s)
        }
        // mit Muster die Mischfarbe, wie die Fläche in 3D aus der Ferne
        TileKey::Surface(id) => {
            let o = a.surface(id)?;
            let rgb = o
                .pattern
                .as_ref()
                .map_or(o.color, |p| proctex::mix(p, o.color));
            swatch_icon(Rgba::from_rgb8(rgb), s, t)
        }
        TileKey::Pen(id) => swatch_icon(Rgba::from_rgb8(a.pen(id)?.color), s, t),
        TileKey::Material(id) => tile(m, t, &m.material(id)?.display(), tw, th, s),
    })
}

/// Stift mit dieser Nummer, sonst der erste.
pub(crate) fn pen_by_number(m: &Model, nr: u16) -> Option<PenId> {
    let a = m.attr();
    a.pens()
        .iter()
        .find(|(_, p)| p.number == nr)
        .or_else(|| a.pens().iter().next())
        .map(|(id, _)| id)
}

/// „Nr. – Name“ eines Stifts in Auswahllisten.
pub(crate) fn pen_label(p: &Pen) -> String {
    format!("{} – {} {}", p.number, p.name, num(p.width_mm, 2))
}

/// Farbfeld als Bildchen in Auswahllisten und Listen.
pub(crate) fn swatch_icon(col: Rgba, s: f32, t: &Theme) -> Canvas {
    let (w, h) = ((t.size.swatch_w * s).round(), (t.size.swatch_h * s).round());
    let mut c = Canvas::new(w as usize, h as usize);
    widgets::swatch(&mut c, Rect::new(0.0, 0.0, w, h), col, false, s, t);
    c
}

/// Strichmuster in Bildpunkten bei Skalierung `s`.
pub(crate) fn pattern_px(pattern: &[Dash], px_per_mm: f32, s: f32) -> DashPattern {
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
pub(crate) fn line_strip(
    m: &Model,
    t: &Theme,
    pattern: &[Dash],
    w: f32,
    h: f32,
    lw: f32,
    s: f32,
) -> Canvas {
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

pub(crate) fn mix(a: Rgba, b: Rgba, f: f32) -> Rgba {
    let m = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
    Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), 255)
}

/// Darstellungsverweise für die Vorschau einer Schraffur ohne Baustoff:
/// Stift 4 (Schraffur) auf Stift 5 (Grund).
pub(crate) fn fill_display(m: &Model, fill: FillId) -> MaterialDisplay {
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
pub(crate) fn fill_tile(m: &Model, t: &Theme, fill: FillId, s: f32) -> Canvas {
    let (w, h) = (t.size.list_thumb_w * s, t.size.list_thumb_h * s);
    tile(m, t, &fill_display(m, fill), w, h, s)
}

/// Schnittfläche nach der Formel des Shaders, mit Rand in Stift „Schnitt“.
pub(crate) fn hatch_image(
    m: &Model,
    t: &Theme,
    d: &MaterialDisplay,
    w: usize,
    h: usize,
    s: f32,
) -> Canvas {
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
pub(crate) fn tile(m: &Model, t: &Theme, d: &MaterialDisplay, w: f32, h: f32, s: f32) -> Canvas {
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
pub(crate) fn paint_fill_preview(
    c: &mut Canvas,
    r: Rect,
    m: &Model,
    t: &Theme,
    s: f32,
    d: MaterialDisplay,
) {
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
pub(crate) fn paint_cube(c: &mut Canvas, r: Rect, o: &Surface, t: &Theme, s: f32) {
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
    let patterned = o
        .pattern
        .as_ref()
        .filter(|p| !matches!(p, Pattern::Foreign(_)));
    // Tabelle des wilden Verbands noch im Hintergrund: Mischfarbe
    let drawn = patterned.filter(|p| proctex::pattern_ready(p));
    match drawn {
        // Paket 6: Muster auf den Seiten um die Ecke herum, der Deckel in
        // der Mischfarbe (waagerechte Flächen ohne Fugen)
        Some(p) => {
            let edge = cube_edge_mm(p);
            poly(
                c,
                &[top, right, mid, left],
                shade(proctex::mix(p, o.color), n_top),
            );
            let west = |rgb| shade(rgb, n_west);
            let south = |rgb| shade(rgb, n_south);
            pattern_face(c, [left_b, down, left], (0.0, edge), p, o.color, &west);
            pattern_face(c, [down, right_b, mid], (edge, edge), p, o.color, &south);
        }
        None => {
            let col = patterned.map_or(o.color, |p| proctex::mix(p, o.color));
            poly(c, &[top, right, mid, left], shade(col, n_top));
            poly(c, &[left, mid, down, left_b], shade(col, n_west));
            poly(c, &[mid, right, right_b, down], shade(col, n_south));
        }
    }
    // Aufgeschnittene Ecke: obere Hälfte der Südseite nahe der Ecke, der
    // Deckel bleibt gleichmäßig (soll-e6, Einstellungen (ak)); mit Muster
    // ohne Schnittecke wie soll-p6-5
    if patterned.is_none() {
        let lerp =
            |p: (f32, f32), q: (f32, f32), f: f32| (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
        let h = 0.5;
        let p0 = lerp(mid, right, h);
        let p1 = right;
        let p2 = lerp(right, right_b, h);
        let p3 = lerp(p0, lerp(down, right_b, h), h);
        poly(c, &[p0, p1, p2, p3], shade(o.cut_color, n_south));
    }
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

/// Kantenlänge des Vorschauwürfels mit Muster (mm): 11 Schichten
/// Mauerwerk, 50 Körner Putz, 40 cm Sichtbeton, 6 Bretter, 3 Platten bzw.
/// 3 Natursteine.
fn cube_edge_mm(p: &Pattern) -> f64 {
    match p {
        Pattern::Masonry { h, joint, .. } => 11.0 * (*h as f64 + *joint as f64),
        Pattern::Plaster { grain, .. } => 50.0 * *grain as f64,
        Pattern::Concrete { .. } => 400.0,
        Pattern::Timber { board, joint, .. } => 6.0 * (*board as f64 + *joint as f64),
        Pattern::Tiles {
            len, wid, joint, ..
        } => 3.0 * (len.max(*wid) as f64 + *joint as f64),
        Pattern::Stone { size, .. } => 3.0 * *size as f64,
        Pattern::Foreign(_) => 1000.0,
    }
}

/// Malt eine Würfelseite mit Muster: Parallelogramm aus `q` = (unten
/// links, unten rechts, oben links) in Bildpunkten, `uv` = (u am linken
/// Rand, Kantenlänge) in mm, v von unten. Je Bildpunkt 2 × 2 Proben
/// ([`proctex::sample`], dieselbe Formel wie der Shader).
fn pattern_face(
    c: &mut Canvas,
    q: [(f32, f32); 3],
    uv: (f64, f64),
    p: &Pattern,
    base: [u8; 3],
    shade: &dyn Fn([u8; 3]) -> Rgba,
) {
    let (o, ex, ey) = (
        q[0],
        (q[1].0 - q[0].0, q[1].1 - q[0].1),
        (q[2].0 - q[0].0, q[2].1 - q[0].1),
    );
    let det = ex.0 * ey.1 - ex.1 * ey.0;
    if det.abs() < 1e-3 {
        return;
    }
    let xs = [q[0].0, q[1].0, q[2].0, q[1].0 + ey.0];
    let ys = [q[0].1, q[1].1, q[2].1, q[1].1 + ey.1];
    let x0 = xs.iter().copied().fold(f32::MAX, f32::min).floor();
    let y0 = ys.iter().copied().fold(f32::MAX, f32::min).floor();
    let x1 = xs.iter().copied().fold(f32::MIN, f32::max).ceil();
    let y1 = ys.iter().copied().fold(f32::MIN, f32::max).ceil();
    let (w, h) = ((x1 - x0).max(0.0) as usize, (y1 - y0).max(0.0) as usize);
    let (u0, edge) = uv;
    let img = Canvas::from_fn(w, h, |px, py| {
        let (mut acc, mut n) = ([0u32; 3], 0u32);
        for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
            let dx = x0 + px as f32 + sx - o.0;
            let dy = y0 + py as f32 + sy - o.1;
            let a = (dx * ey.1 - dy * ey.0) / det;
            let b = (ex.0 * dy - ex.1 * dx) / det;
            if !(0.0..1.0).contains(&a) || !(0.0..1.0).contains(&b) {
                continue;
            }
            let rgb = proctex::sample(p, base, u0 + a as f64 * edge, b as f64 * edge);
            let Rgba(r, g, b, _) = shade(rgb);
            acc[0] += r as u32;
            acc[1] += g as u32;
            acc[2] += b as u32;
            n += 1;
        }
        if n == 0 {
            return Rgba(0, 0, 0, 0);
        }
        let avg = |v: u32| ((v + n / 2) / n) as u8;
        Rgba(avg(acc[0]), avg(acc[1]), avg(acc[2]), (n * 255 / 4) as u8)
    });
    c.blit(&img, x0 as i32, y0 as i32);
}

/// Hat das Muster Fugen für die Ansichtskachel? Putz und fugenloser
/// Sichtbeton nicht.
pub(crate) fn has_joints(p: &Pattern) -> bool {
    match p {
        Pattern::Masonry { .. }
        | Pattern::Timber { .. }
        | Pattern::Tiles { .. }
        | Pattern::Stone { .. } => true,
        Pattern::Concrete { joint, .. } => *joint > 0.0,
        Pattern::Plaster { .. } | Pattern::Foreign(_) => false,
    }
}

/// Ansichtskachel eines Musters (Paket 6, 7a): Fläche in der Farbe der
/// Ansichtsfläche, Fugen als Mittellinien im Stift „Ansichtsmuster“;
/// Mauerwerk 8 Schichten hoch, die anderen Arten so hoch wie die Kante des
/// Vorschauwürfels. Ohne Fugen, oder solange die Verbandstabelle im
/// Hintergrund läuft, bleibt die Fläche leer.
pub(crate) fn paint_elevation_tile(
    c: &mut Canvas,
    r: Rect,
    m: &Model,
    o: &Surface,
    t: &Theme,
    s: f32,
) {
    c.fill_rect(r.x, r.y, r.w, r.h, Rgba::from_rgb8(o.color));
    let Some(p) = o.pattern.as_ref() else {
        return;
    };
    if !has_joints(p) || !proctex::pattern_ready(p) {
        return;
    }
    let pen = m.attr().pen(m.attr().display().pattern.pen);
    let ink = pen.map_or(t.env.edge, |p| Rgba::from_rgb8(p.color));
    let lw = (pen.map_or(0.13, |p| p.width_mm) * t.px_per_mm * s).max(0.8);
    let high = match p {
        Pattern::Masonry { h, joint, .. } => 8.0 * (*h as f64 + *joint as f64),
        _ => cube_edge_mm(p),
    };
    let k = r.h as f64 / high;
    let rect = sk_math::Rect2::new(0.0, 0.0, r.w as f64 / k, r.h as f64 / k);
    let mut path = Path::new();
    for (a, b) in proctex::joint_lines(p, rect) {
        let pt = |v: sk_math::Vec2| (r.x + (v.x * k) as f32, r.y + r.h - (v.y * k) as f32);
        path.segment(pt(a), pt(b), lw);
    }
    c.fill(&path, ink);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;

    /// Paket 5-0 (M1): Kacheln bleiben bis zur nächsten Änderung im
    /// Speicher; eine geänderte Oberfläche malt ihre Kachel neu.
    #[test]
    fn kacheln_folgen_der_aenderung() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let t = Theme::dark();
        let mut tiles = Tiles::default();
        let (id, mut o) = s
            .model()
            .attr()
            .surfaces()
            .iter()
            .next()
            .map(|(i, o)| (i, o.clone()))
            .expect("Oberfläche");
        tiles.sync(s.model(), &t, 1.0);
        let vorher = tiles
            .get(s.model(), &t, 1.0, TileKey::Surface(id))
            .unwrap()
            .to_rgba8();
        tiles.sync(s.model(), &t, 1.0);
        assert_eq!(tiles.tiles.len(), 1, "bleibt im Speicher");
        o.color = [o.color[0] ^ 0x40, o.color[1], o.color[2]];
        assert!(s.edit_model("Oberfläche geändert", |m| m.set_surface(id, o)));
        tiles.sync(s.model(), &t, 1.0);
        assert!(tiles.tiles.is_empty(), "nach der Änderung leer");
        let nachher = tiles
            .get(s.model(), &t, 1.0, TileKey::Surface(id))
            .unwrap()
            .to_rgba8();
        assert_ne!(vorher, nachher);
        // Andere Skalierung: neu
        tiles.sync(s.model(), &t, 1.5);
        assert!(tiles.tiles.is_empty());
    }

    /// Koordinator #35: Eine Vorschau, die auf ihre Verbandstabelle wartet,
    /// bleibt vorläufig, bis `ready` kommt; dann wird neu gemalt und das
    /// vorläufige Bild blendet in `anim_ms` aus.
    #[test]
    fn vorschau_wartet_und_blendet_ein() {
        let m = Model::with_seed(5);
        let t = Theme::dark();
        let id = m.attr().surfaces().iter().next().expect("Oberfläche").0;
        let key = TileKey::Surface(id);
        let r = Rect::new(0.0, 0.0, 20.0, 20.0);
        let mut tiles = Tiles::default();
        tiles.sync(&m, &t, 1.0);
        let mut c = Canvas::new(40, 40);
        let red = Rgba(200, 0, 0, 255);
        let blue = Rgba(0, 0, 200, 255);
        let px = |c: &Canvas| c.to_rgba8()[(5 * 40 + 5) * 4..][..3].to_vec();
        tiles.preview(&mut c, 0, key, r, false, |c| {
            c.fill_rect(0.0, 0.0, 20.0, 20.0, red)
        });
        assert!(tiles.busy(), "wartet auf die Tabelle");
        tiles.preview(&mut c, 0, key, r, false, |_| panic!("nur kopieren"));
        assert!(t.size.anim_ms > 0.0);
        tiles.preview(&mut c, 0, key, r, true, |c| {
            c.fill_rect(0.0, 0.0, 20.0, 20.0, blue)
        });
        assert_eq!(
            px(&c),
            vec![200, 0, 0],
            "vorläufiges Bild liegt noch darüber"
        );
        assert!(tiles.busy() && tiles.tick(), "blendet ein");
        let f = tiles.previews[0].fade.as_mut().expect("Einblendung");
        f.1 -= std::time::Duration::from_secs(5);
        tiles.preview(&mut c, 0, key, r, true, |_| panic!("nur kopieren"));
        assert_eq!(px(&c), vec![0, 0, 200], "fertig eingeblendet");
        assert!(!tiles.busy() && !tiles.tick());
    }
}
