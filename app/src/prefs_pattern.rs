//! Fenster „Muster“ (Paket 7b, soll-p7-1/-2): ein Zustand der
//! Einstellungen über der gewählten Oberfläche. Links die Werks- und
//! Firmenvorlagen, in der Mitte die große Vorschau mit dem Flächen-Shader
//! (Nah, Ansicht 1:100, Fern; Vorher/Nachher) und sechs Varianten, rechts
//! die Regler des Abschnitts „Muster“. Jede Wahl wirkt sofort wie jede
//! Eingabe der Einstellungen; „Abbrechen“ stellt Muster und Farbe vom
//! Öffnen her, „OK“ kehrt zu den Einstellungen zurück (deren OK/Abbrechen
//! gilt weiter). Die Vorschau zeichnet `main.rs` über
//! [`Prefs::pattern_preview`] in Löcher dieses Fensterbilds.

use super::attr_tabs::AttrLayout;
use super::*;
use crate::pattern_view::{Input, Look, Stage};
use sk_model::proctex::{self, Pattern};
use sk_model::Surface;

/// Vorgabegröße des Fensters (dip, soll-p7-1).
pub(super) const SIZE: (f32, f32) = (1120.0, 660.0);
const HEAD_H: f32 = 52.0;
const FOOT_H: f32 = 60.0;
const LEFT_W: f32 = 230.0;
const RIGHT_W: f32 = 256.0;
const ROW_H: f32 = 30.0;
/// Varianten: Anzahl und Kachel (dip).
const VARIANTS: usize = 6;
const TILE: (f32, f32) = (50.0, 32.0);
/// Helligkeit der Varianten bei Mauerwerk und Naturstein: ± 8 %.
const VARIANT_SHADE: f32 = 0.08;

/// Ziele im Fenster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pw {
    /// „Mehr Muster …“ im Reiter „Oberflächen“: öffnet das Fenster.
    Open,
    Close,
    /// Werksvorlage bzw. Firmenvorlage Nummer.
    Preset(usize),
    Company(usize),
    Stage(Stage),
    Compare,
    /// Teiler „Vorher/Nachher“ (ganze große Vorschau, solange an).
    Split,
    Variant(usize),
    Reroll,
    Save,
    Cancel,
    Ok,
}

/// Zustand des Fensters.
#[derive(Debug)]
pub(super) struct PatWin {
    surface: SurfaceId,
    /// Muster und Farbe beim Öffnen (Vorher, Abbrechen).
    before: Look,
    stage: Stage,
    compare: bool,
    /// Teiler als Anteil der Breite.
    split: f32,
    /// Reihe der Varianten: Zähler für ⟳, Muster, aus dem sie entstanden,
    /// die Varianten und die gewählte.
    salt: u32,
    variant_base: Option<Pattern>,
    variants: Vec<Pattern>,
    chosen: Option<usize>,
    /// Name im Dialog „Als Vorlage speichern …“ und Meldung im Fuß.
    pub(super) name: String,
    pub(super) error: Option<String>,
    /// Übergang nach Vorlagen- oder Variantenwahl: Beginn; `hold` = das
    /// jetzige Vorschaubild festhalten (holt `main.rs` ab).
    changed: Option<Instant>,
    /// `anim_ms` des laufenden Farbschemas (beim Zeichnen gemerkt).
    anim: f32,
    hold: bool,
    /// Zähler der Verbandstabellen, solange eine Vorschau darauf wartet.
    waiting: Option<u64>,
}

/// Lage im Fenster (Fensterkoordinaten, ganze Bildpunkte bei Vorschauen).
struct PwLayout {
    close: Rect,
    left: Rect,
    right: Rect,
    mid: Rect,
    presets: Vec<Rect>,
    company: Vec<Rect>,
    company_y: f32,
    big: Rect,
    stages: [(Stage, Rect, &'static str); 3],
    compare: Rect,
    sep_y: f32,
    tiles: Vec<Rect>,
    reroll: Rect,
    save: Rect,
    cancel: Rect,
    ok: Rect,
}

/// Varianten eines Musters: anderer Startwert, bei Mauerwerk und
/// Naturstein die Steinfarben um bis zu ± 8 % heller oder dunkler.
fn variants_of(p: &Pattern, salt: u32) -> Vec<Pattern> {
    let base = proctex::seed_of(p);
    (0..VARIANTS as u32)
        .map(|i| {
            let k = salt.wrapping_mul(VARIANTS as u32) + i + 1;
            let h = proctex::lowbias32(base ^ k.wrapping_mul(0x9e37_79b9));
            let mut v = proctex::with_seed(p, h & proctex::SEED_MAX);
            let f = 1.0 + VARIANT_SHADE * (2.0 * ((h >> 8) & 0xffff) as f32 / 65535.0 - 1.0);
            let shade = |c: &mut [u8; 3]| {
                *c = c.map(|x| (x as f32 * f).round().clamp(0.0, 255.0) as u8);
            };
            match &mut v {
                Pattern::Masonry { palette, hpal, .. } => {
                    palette.iter_mut().for_each(|(c, _)| shade(c));
                    if let Some(hp) = hpal {
                        hp.iter_mut().for_each(|(c, _)| shade(c));
                    }
                }
                Pattern::Stone { palette, .. } => palette.iter_mut().for_each(|(c, _)| shade(c)),
                _ => {}
            }
            v
        })
        .collect()
}

impl PatWin {
    fn new(surface: SurfaceId, o: &Surface) -> PatWin {
        PatWin {
            surface,
            before: Look {
                pattern: o.pattern.clone(),
                base: o.color,
            },
            stage: Stage::Near,
            compare: false,
            split: 0.5,
            salt: 0,
            variant_base: None,
            variants: Vec::new(),
            chosen: None,
            name: String::new(),
            error: None,
            changed: None,
            anim: 0.0,
            hold: false,
            waiting: None,
        }
    }

    /// Hält die Variantenreihe zum jetzigen Muster: Hat es sich anders als
    /// durch die Wahl einer Variante geändert, entsteht eine neue Reihe.
    fn sync(&mut self, now: Option<&Pattern>) {
        let picked = self.chosen.and_then(|i| self.variants.get(i));
        if now.is_some() && now == picked {
            return;
        }
        if now != self.variant_base.as_ref() || self.variants.is_empty() {
            self.variant_base = now.cloned();
            self.variants = now
                .filter(|p| !matches!(p, Pattern::Foreign(_)))
                .map_or_else(Vec::new, |p| variants_of(p, self.salt));
            self.chosen = None;
        }
    }

    /// Fortschritt des Übergangs 0..1 (`None`: keiner).
    fn progress(&self, anim_ms: f32) -> Option<f32> {
        let t0 = self.changed?;
        let f = t0.elapsed().as_secs_f32() * 1000.0 / anim_ms.max(1.0);
        (anim_ms > 0.0 && f < 1.0).then_some(f)
    }
}

/// Was `main.rs` für die Vorschau braucht: Lage des Vorschaubilds im
/// Programmfenster (links, oben, Breite, Höhe), die Szene (Kanten, Stift
/// und Papier setzt `main.rs`), ob das jetzige Bild zum Überblenden
/// festzuhalten ist, und die Deckkraft des festgehaltenen.
pub struct PwPreview {
    pub at: [i32; 4],
    pub input: Input,
    pub hold: bool,
}

impl Prefs {
    /// Firmenvorlagen aus dem Firmenkatalog (beim Öffnen und nach dem
    /// Speichern einer Vorlage).
    pub fn set_company_presets(&mut self, v: Vec<sk_model::proctex::CompanyPreset>) {
        self.company_presets = v;
    }

    /// Wartende „Als Vorlage speichern …“ (nach [`Out::save_preset`]).
    pub fn take_save_preset(&mut self) -> Option<(String, Pattern, [u8; 3])> {
        self.save_preset.take()
    }

    /// Ergebnis von „Als Vorlage speichern …“ für den Fuß des Fensters.
    pub fn preset_saved(&mut self, r: Result<String, String>) {
        if let Some(p) = self.pw.as_mut() {
            p.error = Some(match r {
                Ok(name) => format!("„{name}“ steht jetzt unter „Firma“."),
                Err(e) => e,
            });
        }
    }

    fn pw_layout(&self, t: &Theme, w: &Win) -> PwLayout {
        let f = self.frame(t, w);
        let s = w.scale;
        let top = f.y + HEAD_H * s;
        let bottom = f.y + f.h - FOOT_H * s;
        let left = Rect::new(f.x, top, LEFT_W * s, bottom - top);
        let right = Rect::new(f.x + f.w - RIGHT_W * s, top, RIGHT_W * s, bottom - top);
        let mid = Rect::new(
            left.x + left.w,
            top,
            right.x - left.x - left.w,
            bottom - top,
        );
        let r =
            |x: f32, y: f32, w: f32, h: f32| Rect::new(x.round(), y.round(), w.round(), h.round());
        // Vorlagen untereinander, darunter „Firma“
        let row = |i: usize, y0: f32| {
            r(
                f.x + 12.0 * s,
                y0 + i as f32 * ROW_H * s,
                (LEFT_W - 24.0) * s,
                (ROW_H - 2.0) * s,
            )
        };
        let y0 = top + 34.0 * s;
        let presets: Vec<Rect> = (0..proctex::presets().len()).map(|i| row(i, y0)).collect();
        let company_y = y0 + presets.len() as f32 * ROW_H * s + 14.0 * s;
        let company: Vec<Rect> = (0..self.company_presets.len())
            .map(|i| row(i, company_y + 20.0 * s))
            .filter(|q| q.y + q.h <= bottom)
            .collect();
        // Große Vorschau, Stufen, Varianten
        let pad = 40.0 * s;
        let bw = (mid.w - 2.0 * pad).max(80.0 * s);
        let bh = (bw * 0.643).min(mid.h - 190.0 * s).max(60.0 * s);
        let big = r(mid.x + pad, top + 38.0 * s, bw, bh);
        let sy = big.y + big.h + 14.0 * s;
        let mut x = big.x;
        let mut stages = [(Stage::Near, Rect::new(0.0, 0.0, 0.0, 0.0), ""); 3];
        for (k, (st, text, sw)) in [
            (Stage::Near, "Nah", 54.0),
            (Stage::Elevation, "Ansicht 1:100", 110.0),
            (Stage::Far, "Fern", 58.0),
        ]
        .into_iter()
        .enumerate()
        {
            stages[k] = (st, r(x + 2.0 * s, sy + 2.0 * s, sw * s, 24.0 * s), text);
            x += sw * s + 2.0 * s;
        }
        let compare = r(x + 18.0 * s, sy, 128.0 * s, 28.0 * s);
        let sep_y = (sy + 28.0 * s + 20.0 * s).round();
        let ty = sep_y + 16.0 * s;
        let tx = big.x + 72.0 * s;
        let tiles: Vec<Rect> = (0..VARIANTS)
            .map(|i| {
                r(
                    tx + i as f32 * (TILE.0 + 8.0) * s,
                    ty,
                    TILE.0 * s,
                    TILE.1 * s,
                )
            })
            .collect();
        let rx = tx + VARIANTS as f32 * (TILE.0 + 8.0) * s + 8.0 * s;
        let reroll = r(rx, ty + 4.0 * s, 24.0 * s, 24.0 * s);
        // Fuß
        let by = f.y + f.h - 44.0 * s;
        let fr = f.x + f.w - 16.0 * s;
        PwLayout {
            close: r(f.x + f.w - 40.0 * s, f.y + 10.0 * s, 28.0 * s, 28.0 * s),
            left,
            right,
            mid,
            presets,
            company,
            company_y,
            big,
            stages,
            compare,
            sep_y,
            tiles,
            reroll,
            save: r(f.x + 16.0 * s, by, 190.0 * s, 30.0 * s),
            ok: r(fr - 110.0 * s, by, 110.0 * s, 30.0 * s),
            cancel: r(
                fr - 110.0 * s - 10.0 * s - 100.0 * s,
                by,
                100.0 * s,
                30.0 * s,
            ),
        }
    }

    /// Regler der rechten Spalte (wie der Abschnitt „Muster“).
    pub(super) fn pw_controls(&self, t: &Theme, w: &Win, sc: &Scene) -> AttrLayout {
        let s = w.scale;
        let lay = self.pw_layout(t, w);
        let side = Rect::new(
            lay.right.x + 18.0 * s,
            lay.right.y + 20.0 * s,
            lay.right.w - 36.0 * s,
            lay.right.h - 40.0 * s,
        );
        let zero = Rect::new(0.0, 0.0, 0.0, 0.0);
        let mut l = AttrLayout {
            list: zero,
            body: zero,
            rows: Vec::new(),
            bar: None,
            scroll: 0.0,
            content_h: 0.0,
            buttons: None,
            hint_y: 0.0,
            thumb_x: 0.0,
            side,
            items: Vec::new(),
            texts: Vec::new(),
            readonly: Vec::new(),
            preview: None,
            preview2: None,
        };
        let m = sc.model();
        if let Some(pw) = &self.pw {
            if let Some(o) = m.attr().surface(pw.surface) {
                self.pattern_side(&mut l, t, s, m, (pw.surface, o), side.y, true);
            }
        }
        l
    }

    pub(super) fn pw_hit(&self, t: &Theme, w: &Win, sc: &Scene, x: f64, y: f64) -> Option<Target> {
        let f = self.frame(t, w);
        let l = self.pw_layout(t, w);
        let pw = self.pw.as_ref()?;
        if l.close.contains(x, y) {
            return Some(Target::Pw(Pw::Close));
        }
        if y < (f.y + HEAD_H * w.scale) as f64 {
            return Some(Target::Head);
        }
        for (r, p) in [(l.save, Pw::Save), (l.cancel, Pw::Cancel), (l.ok, Pw::Ok)] {
            if r.contains(x, y) {
                return Some(Target::Pw(p));
            }
        }
        if let Some(i) = l.presets.iter().position(|r| r.contains(x, y)) {
            return Some(Target::Pw(Pw::Preset(i)));
        }
        if let Some(i) = l.company.iter().position(|r| r.contains(x, y)) {
            return Some(Target::Pw(Pw::Company(i)));
        }
        for (st, r, _) in l.stages {
            if r.contains(x, y) {
                return Some(Target::Pw(Pw::Stage(st)));
            }
        }
        if l.compare.contains(x, y) {
            return Some(Target::Pw(Pw::Compare));
        }
        if pw.compare && l.big.contains(x, y) {
            return Some(Target::Pw(Pw::Split));
        }
        let n = pw.variants.len();
        if let Some(i) = l.tiles.iter().take(n).position(|r| r.contains(x, y)) {
            return Some(Target::Pw(Pw::Variant(i)));
        }
        if n > 0 && l.reroll.contains(x, y) {
            return Some(Target::Pw(Pw::Reroll));
        }
        if l.right.contains(x, y) {
            return self.attr_hit(t, w, sc, x, y);
        }
        None
    }

    pub(super) fn pw_drag_split(&mut self, x: f64, cx: &Ctx) {
        let l = self.pw_layout(cx.theme, &cx.win);
        if let Some(pw) = self.pw.as_mut() {
            pw.split = ((x as f32 - l.big.x) / l.big.w.max(1.0)).clamp(0.0, 1.0);
        }
    }

    /// Setzt Muster und (auf Wunsch) Farbe der Oberfläche des Fensters in
    /// einem Schritt der Einstellungen.
    fn pw_put(&mut self, p: Option<Pattern>, base: Option<[u8; 3]>, cx: &mut Ctx, out: &mut Out) {
        let Some(pw) = self.pw.as_ref() else {
            return;
        };
        if let Some(p) = &p {
            if let Err(e) = proctex::validate(p) {
                self.error = Some(e);
                return;
            }
        }
        let id = pw.surface;
        out.model |= cx.scene.edit_attr(|m| {
            let Some(o) = m.attr().surface(id).cloned() else {
                return false;
            };
            let mut changed = false;
            if let Some(b) = base.filter(|b| *b != o.color) {
                changed |= m.set_surface(
                    id,
                    Surface {
                        color: b,
                        ..o.clone()
                    },
                );
            }
            if o.pattern != p {
                changed |= m.set_surface_pattern(id, p);
            }
            changed
        });
    }

    /// Übergang beginnen: das alte Vorschaubild blendet aus.
    fn pw_changed(&mut self) {
        if let Some(pw) = self.pw.as_mut() {
            pw.changed = Some(Instant::now());
            pw.hold = true;
        }
    }

    pub(super) fn pw_click(&mut self, p: Pw, cx: &mut Ctx, out: &mut Out) {
        *out = Out {
            model: out.model,
            ..Out::all()
        };
        if p == Pw::Open {
            if let Some((id, o)) = self.sel_surf(cx.scene.model()) {
                self.edit = None;
                self.popup = None;
                let mut pw = PatWin::new(id, &o);
                pw.anim = cx.theme.size.anim_ms;
                self.pw = Some(pw);
            }
            return;
        }
        let Some(pw) = self.pw.as_mut() else {
            return;
        };
        pw.error = None;
        match p {
            Pw::Open => {}
            Pw::Preset(i) => {
                if let Some(v) = proctex::presets().get(i) {
                    let (pat, base) = (v.pattern.clone(), v.base);
                    self.pw_put(Some(pat), Some(base), cx, out);
                    self.pw_changed();
                }
            }
            Pw::Company(i) => {
                if let Some(v) = self.company_presets.get(i) {
                    let (pat, base) = (v.pattern.clone(), v.base);
                    self.pw_put(Some(pat), Some(base), cx, out);
                    self.pw_changed();
                }
            }
            Pw::Stage(st) => pw.stage = st,
            Pw::Compare => pw.compare = !pw.compare,
            Pw::Split => {}
            Pw::Variant(i) => {
                if let Some(v) = pw.variants.get(i).cloned() {
                    pw.chosen = Some(i);
                    self.pw_put(Some(v), None, cx, out);
                    self.pw_changed();
                }
            }
            Pw::Reroll => {
                pw.salt = pw.salt.wrapping_add(1);
                pw.variants.clear();
                pw.chosen = None;
            }
            Pw::Save => {
                pw.name.clear();
                self.popup = Some(Popup::SaveAs(0));
                self.begin_edit(FieldId::PresetName, cx);
            }
            Pw::Ok => {
                self.pw = None;
                self.edit = None;
                self.popup = None;
            }
            Pw::Cancel | Pw::Close => {
                let before = pw.before.clone();
                self.pw_put(before.pattern, Some(before.base), cx, out);
                self.pw = None;
                self.edit = None;
                self.popup = None;
            }
        }
    }

    /// Läuft ein Übergang oder wartet eine Vorschau auf ihre
    /// Verbandstabelle?
    pub(super) fn pw_busy(&self) -> bool {
        self.pw
            .as_ref()
            .is_some_and(|p| p.waiting.is_some() || p.progress(p.anim).is_some())
    }

    /// Neu zeichnen: Übergang läuft oder Tabelle fertig geworden.
    pub(super) fn pw_tick(&mut self) -> bool {
        let Some(p) = self.pw.as_mut() else {
            return false;
        };
        let anim = p.anim;
        let fading = p.changed.is_some();
        if fading && p.progress(anim).is_none() {
            p.changed = None;
        }
        // Einmal je neuer Tabelle (Review 3u): die nächste Vorschau setzt
        // `waiting` wieder, wenn sie noch wartet
        let ready = p.waiting.is_some_and(|g| g != proctex::bond_generation());
        if ready {
            p.waiting = None;
        }
        fading || ready
    }

    /// Deckkraft des festgehaltenen Vorschaubilds (Übergang in `anim_ms`).
    pub fn pattern_fade(&self, t: &Theme) -> f32 {
        self.pw
            .as_ref()
            .and_then(|p| p.progress(t.size.anim_ms))
            .map_or(0.0, |f| 1.0 - crate::scene::ease_out(f))
    }

    /// Öffnet das Fenster „Muster“ für die Oberfläche `id` wie der Knopf
    /// „Muster …“ (für Tests); `false`, wenn es sie nicht gibt.
    #[cfg(test)]
    pub fn open_pattern(&mut self, sc: &Scene, t: &Theme, id: SurfaceId) -> bool {
        let Some(o) = sc.model().attr().surface(id) else {
            return false;
        };
        self.edit = None;
        self.popup = None;
        let mut pw = PatWin::new(id, o);
        pw.anim = t.size.anim_ms;
        self.pw = Some(pw);
        self.full_frame = true;
        true
    }

    /// Szene der Vorschau im Fenster „Muster“, `None` ohne Fenster.
    pub fn pattern_preview(&mut self, t: &Theme, w: &Win, sc: &Scene) -> Option<PwPreview> {
        let l = self.pw_layout(t, w);
        let s = w.scale;
        let pw = self.pw.as_mut()?;
        // Oberfläche weg (etwa nach Rückgängig, Löschen): das Fenster
        // schließt und wartet auf nichts mehr (A305)
        let Some(o) = sc.model().attr().surface(pw.surface) else {
            self.pw = None;
            self.full_frame = true;
            return None;
        };
        pw.sync(o.pattern.as_ref());
        let after = Look {
            pattern: o.pattern.clone(),
            base: o.color,
        };
        let variants: Vec<Look> = pw
            .variants
            .iter()
            .map(|v| Look {
                pattern: Some(v.clone()),
                base: o.color,
            })
            .collect();
        let mut rects = vec![l.big];
        rects.extend(l.tiles.iter().take(variants.len()));
        let x0 = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let y0 = rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let x1 = rects.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
        let y1 = rects.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
        let rel = |r: &Rect| [(r.x - x0) as i32, (r.y - y0) as i32, r.w as i32, r.h as i32];
        let pending = std::iter::once(&after)
            .chain([&pw.before])
            .chain(&variants)
            .filter_map(|l| l.pattern.as_ref())
            .any(|p| !proctex::pattern_ready(p));
        pw.waiting = pending.then(proctex::bond_generation);
        let hold = std::mem::take(&mut pw.hold);
        let input = Input {
            after,
            before: pw.before.clone(),
            variants,
            stage: pw.stage,
            big: rel(&l.big),
            split: pw.compare.then(|| (pw.split * l.big.w).round() as i32),
            tiles: l.tiles.iter().map(rel).collect(),
            ink: [0.0, 0.0, 0.0, 1.0],
            paper: [1.0; 3],
            px_per_mm: t.px_per_mm * s,
            model_edges: Default::default(),
            drawing_edges: Default::default(),
        };
        Some(PwPreview {
            at: [x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32],
            input,
            hold,
        })
    }

    /// Passt das jetzige Muster genau zu einer Vorlage? (Werks-, dann
    /// Firmenvorlagen; Nummer in der jeweiligen Liste.)
    fn pw_match(&self, o: &Surface) -> (Option<usize>, Option<usize>) {
        let same = |p: &Pattern, b: [u8; 3]| o.pattern.as_ref() == Some(p) && o.color == b;
        let w = proctex::presets()
            .iter()
            .position(|v| same(&v.pattern, v.base));
        let c = self
            .company_presets
            .iter()
            .position(|v| same(&v.pattern, v.base));
        (w, c)
    }

    pub(super) fn pw_paint(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sc: &Scene,
    ) -> (Canvas, i32, i32) {
        if let Some(p) = self.pw.as_mut() {
            p.anim = t.size.anim_ms;
        }
        let f = self.frame(t, w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let mut c = self.spare.take().unwrap_or_else(|| Canvas::new(0, 0));
        c.reuse((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let key = (c.width, c.height, s.to_bits(), t.rev);
        if self.ground.as_ref().map(|g| g.0) != Some(key) {
            let mut g = Canvas::new(c.width, c.height);
            widgets::panel(&mut g, Rect::new(m, m, f.w, f.h), s, t);
            self.ground = Some((key, g));
        }
        if let Some((_, g)) = &self.ground {
            c.copy_rows(g, 0, c.height);
        }
        let model = sc.model();
        let surface = self
            .pw
            .as_ref()
            .and_then(|p| model.attr().surface(p.surface).cloned());
        if let (Some(pw), Some(o)) = (self.pw.as_mut(), &surface) {
            pw.sync(o.pattern.as_ref());
        }
        let Some(pw) = self.pw.as_ref() else {
            let (x, y) = self.origin(t, w);
            return (c, x, y);
        };
        let at = |r: Rect| Rect::new(r.x - f.x + m, r.y - f.y + m, r.w, r.h);
        let u = &t.ui;
        let l = self.pw_layout(t, w);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (font, small) = (t.size.font * s, t.size.font_small * s);
        let line = s.round().max(1.0);
        let cap = regular.map_or(font * 0.7, |ft| ft.cap_height(font));
        // Kopf
        let name = surface.as_ref().map_or("", |o| o.name.as_str());
        let title = widgets::ellipsize(
            bold,
            &format!("Muster · Oberfläche „{name}“"),
            t.size.font_title * s,
            f.w - 100.0 * s,
        );
        label(
            &mut c,
            bold,
            &title,
            t.size.font_title * s,
            m + 20.0 * s,
            m + 32.0 * s,
            u.text,
        );
        let cr = at(l.close);
        if self.hover == Some(Target::Pw(Pw::Close)) {
            let mut p = Path::new();
            p.rounded_rect(cr.x, cr.y, cr.w, cr.h, 4.0 * s);
            c.fill(&p, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        c.fill_rect(m, m + HEAD_H * s, f.w, line, u.border);
        c.fill_rect(m, m + f.h - FOOT_H * s, f.w, line, u.border);
        for x in [l.left.x + l.left.w, l.right.x] {
            let r = at(Rect::new(x, l.left.y, 0.0, l.left.h));
            c.fill_rect(r.x, r.y, line, r.h, u.border);
        }
        // Vorlagen
        let (w_match, c_match) = surface.as_ref().map_or((None, None), |o| self.pw_match(o));
        let lx = at(l.left).x + 20.0 * s;
        label(
            &mut c,
            bold,
            "VORLAGEN",
            small,
            lx,
            at(l.left).y + 24.0 * s,
            u.text_dim,
        );
        let row = |c: &mut Canvas, r: Rect, text: &str, icon: Rgba, on: bool, hover: bool| {
            let rr = at(r);
            if on {
                let mut p = Path::new();
                p.rounded_rect(rr.x, rr.y, rr.w, rr.h, 5.0 * s);
                c.fill(&p, u.pressed);
                c.fill_rect(
                    rr.x - 8.0 * s,
                    rr.y + 4.0 * s,
                    3.0 * s,
                    rr.h - 8.0 * s,
                    u.accent,
                );
            } else if hover {
                let mut p = Path::new();
                p.rounded_rect(rr.x, rr.y, rr.w, rr.h, 5.0 * s);
                c.fill(&p, u.hover);
            }
            let ib = Rect::new(
                rr.x + 10.0 * s,
                rr.y + (rr.h - 18.0 * s) * 0.5,
                18.0 * s,
                18.0 * s,
            );
            c.fill_rect(ib.x, ib.y, ib.w, ib.h, u.border);
            c.fill_rect(
                ib.x + line,
                ib.y + line,
                ib.w - 2.0 * line,
                ib.h - 2.0 * line,
                icon,
            );
            let tx = ib.x + ib.w + 12.0 * s;
            let ft = if on { bold } else { regular };
            let max = rr.x + rr.w - tx - 18.0 * s;
            let text = widgets::ellipsize(ft, text, font, max);
            let base = rr.y + (rr.h + cap) * 0.5;
            label(c, ft, &text, font, tx, base, u.text);
            if on {
                // Punkt an der Vorlage, die gerade passt (p7 §3.2)
                let tw = ft.map_or(0.0, |x| x.width(&text, font));
                let mut p = Path::new();
                let (r0, cx0, cy0) = (2.5 * s, tx + tw + 9.0 * s, base - cap * 0.45);
                p.rounded_rect(cx0 - r0, cy0 - r0, 2.0 * r0, 2.0 * r0, r0);
                c.fill(&p, u.text_dim);
            }
        };
        for (i, (r, v)) in l.presets.iter().zip(proctex::presets()).enumerate() {
            let icon = Rgba::from_rgb8(proctex::mix(&v.pattern, v.base));
            let hover = self.hover == Some(Target::Pw(Pw::Preset(i)));
            row(&mut c, *r, &v.name, icon, w_match == Some(i), hover);
        }
        label(
            &mut c,
            bold,
            "FIRMA",
            small,
            lx,
            at(Rect::new(0.0, l.company_y, 0.0, 0.0)).y + 12.0 * s,
            u.text_dim,
        );
        if self.company_presets.is_empty() {
            label(
                &mut c,
                regular,
                "keine eigenen Vorlagen",
                small,
                lx,
                at(Rect::new(0.0, l.company_y, 0.0, 0.0)).y + 36.0 * s,
                u.text_dim,
            );
        }
        for (i, (r, v)) in l.company.iter().zip(&self.company_presets).enumerate() {
            let icon = Rgba::from_rgb8(proctex::mix(&v.pattern, v.base));
            let hover = self.hover == Some(Target::Pw(Pw::Company(i)));
            row(&mut c, *r, &v.name, icon, c_match == Some(i), hover);
        }
        // Große Vorschau: ein Loch, durch das das Bild des Renderers scheint
        let hole = |c: &mut Canvas, r: Rect| {
            let r = at(r);
            c.shade_rect(r.x, r.y, r.x + r.w, r.y + r.h, |_, _| Rgba(0, 0, 0, 0));
        };
        hole(&mut c, l.big);
        if pw.compare {
            let b = at(l.big);
            let sx = (b.x + pw.split * b.w).round();
            c.fill_rect(sx - s, b.y, (2.0 * s).max(1.0), b.h, u.text);
            let (gx, gy, gr) = (sx, b.y + b.h * 0.5, 13.0 * s);
            let mut p = Path::new();
            p.rounded_rect(gx - gr, gy - gr, 2.0 * gr, 2.0 * gr, gr);
            c.fill(&p, u.text);
            let mut p = Path::new();
            let a = 4.0 * s;
            p.move_to(gx - 2.0 * s, gy - a);
            p.line_to(gx - 2.0 * s - a, gy);
            p.line_to(gx - 2.0 * s, gy + a);
            p.close();
            p.move_to(gx + 2.0 * s, gy - a);
            p.line_to(gx + 2.0 * s + a, gy);
            p.line_to(gx + 2.0 * s, gy + a);
            p.close();
            c.fill(&p, u.bg);
            for (text, right) in [("Vorher", false), ("Nachher", true)] {
                let tw = bold.map_or(0.0, |x| x.width(text, small));
                let (pw_, ph) = (tw + 28.0 * s, 22.0 * s);
                let px = if right {
                    b.x + b.w - 10.0 * s - pw_
                } else {
                    b.x + 10.0 * s
                };
                let mut p = Path::new();
                p.rounded_rect(px, b.y + 10.0 * s, pw_, ph, 5.0 * s);
                c.fill(&p, u.hud_bg);
                label(
                    &mut c,
                    bold,
                    text,
                    small,
                    px + 14.0 * s,
                    b.y + 25.0 * s,
                    u.text,
                );
            }
        }
        // Stufen als Segment, daneben Vorher/Nachher
        let seg = at(Rect::new(
            l.stages[0].1.x - 2.0 * s,
            l.stages[0].1.y - 2.0 * s,
            l.stages[2].1.x + l.stages[2].1.w - l.stages[0].1.x + 4.0 * s,
            28.0 * s,
        ));
        let mut p = Path::new();
        p.rounded_rect(seg.x, seg.y, seg.w, seg.h, 7.0 * s);
        c.fill(&p, u.field);
        for (st, r, text) in l.stages {
            let rr = at(r);
            let on = pw.stage == st;
            let hover = self.hover == Some(Target::Pw(Pw::Stage(st)));
            if on {
                let mut p = Path::new();
                p.rounded_rect(rr.x, rr.y, rr.w, rr.h, 6.0 * s);
                c.fill(&p, u.pressed);
                let mut p = Path::new();
                p.rounded_rect(rr.x, rr.y, rr.w, rr.h, 6.0 * s);
                p.rounded_rect_hole(
                    rr.x + line,
                    rr.y + line,
                    rr.w - 2.0 * line,
                    rr.h - 2.0 * line,
                    6.0 * s - line,
                );
                c.fill(&p, u.accent);
            } else if hover {
                let mut p = Path::new();
                p.rounded_rect(rr.x, rr.y, rr.w, rr.h, 6.0 * s);
                c.fill(&p, u.hover);
            }
            let ft = if on { bold } else { regular };
            let tw = ft.map_or(0.0, |x| x.width(text, small));
            let col = if on { u.text } else { u.text_dim };
            label(
                &mut c,
                ft,
                text,
                small,
                rr.x + (rr.w - tw) * 0.5,
                rr.y + 17.0 * s,
                col,
            );
        }
        let st = ButtonState {
            hover: self.hover == Some(Target::Pw(Pw::Compare)),
            pressed: self.pressed == Some(Target::Pw(Pw::Compare)),
            active: pw.compare,
            disabled: false,
        };
        widgets::button(&mut c, fonts, at(l.compare), "◧ Vorher/Nachher", st, s, t);
        let sep = at(Rect::new(
            l.mid.x + 20.0 * s,
            l.sep_y,
            l.mid.w - 35.0 * s,
            0.0,
        ));
        c.fill_rect(sep.x, sep.y, sep.w, line, u.border);
        // Varianten
        let t0 = at(l.tiles[0]);
        label(
            &mut c,
            regular,
            "Varianten",
            font,
            at(l.big).x,
            t0.y + (t0.h + cap) * 0.5,
            u.text_dim,
        );
        for (i, r) in l.tiles.iter().enumerate() {
            let rr = at(*r);
            if i < pw.variants.len() {
                let chosen = pw.chosen == Some(i);
                let hover = self.hover == Some(Target::Pw(Pw::Variant(i)));
                if chosen || hover {
                    let b = if chosen { 2.0 * s } else { line };
                    let col = if chosen { u.accent } else { u.text_dim };
                    let mut p = Path::new();
                    p.rounded_rect(rr.x - b, rr.y - b, rr.w + 2.0 * b, rr.h + 2.0 * b, 0.0);
                    p.rounded_rect_hole(rr.x, rr.y, rr.w, rr.h, 0.0);
                    c.fill(&p, col);
                }
                hole(&mut c, *r);
            } else {
                c.fill_rect(rr.x, rr.y, rr.w, rr.h, u.field);
            }
        }
        if !pw.variants.is_empty() {
            // ⟳ in Akzent: Bogen mit Pfeilspitze
            let rr = at(l.reroll);
            let hover = self.hover == Some(Target::Pw(Pw::Reroll));
            let col = if hover { u.accent_hover } else { u.accent };
            let (ox, oy, rad) = (rr.x + rr.w * 0.5, rr.y + rr.h * 0.5, 7.0 * s);
            let mut p = Path::new();
            let n = 20;
            for k in 0..n {
                let a0 = 0.35 + 5.4 * k as f32 / n as f32;
                let a1 = 0.35 + 5.4 * (k + 1) as f32 / n as f32;
                p.segment(
                    (ox + rad * a0.cos(), oy - rad * a0.sin()),
                    (ox + rad * a1.cos(), oy - rad * a1.sin()),
                    1.8 * s,
                );
            }
            let (ax, ay) = (ox + rad * 0.35f32.cos(), oy - rad * 0.35f32.sin());
            p.move_to(ax - 4.0 * s, ay - 1.0 * s);
            p.line_to(ax + 3.0 * s, ay - 2.0 * s);
            p.line_to(ax + 1.0 * s, ay + 5.0 * s);
            p.close();
            c.fill(&p, col);
        }
        // Regler rechts
        let lc = self.pw_controls(t, w, sc);
        self.paint_attr_items(&mut c, &lc, t, fonts, s, sc, &at);
        // Fuß
        let pw = self.pw.as_ref().expect("Fenster offen");
        for (r, p, text) in [
            (l.save, Pw::Save, "Als Vorlage speichern …"),
            (l.cancel, Pw::Cancel, "Abbrechen"),
            (l.ok, Pw::Ok, "OK"),
        ] {
            let tg = Target::Pw(p);
            let st = ButtonState {
                hover: self.hover == Some(tg),
                pressed: self.pressed == Some(tg) && self.hover == Some(tg),
                active: p == Pw::Ok,
                disabled: false,
            };
            widgets::button(&mut c, fonts, at(r), text, st, s, t);
        }
        let msg = self
            .edit
            .as_ref()
            .and_then(|e| e.invalid.clone())
            .or_else(|| self.error.clone())
            .or_else(|| pw.error.clone());
        if let Some(msg) = msg {
            let x0 = at(l.save).x + l.save.w + 14.0 * s;
            let max = at(l.cancel).x - 14.0 * s - x0;
            let text = widgets::ellipsize(regular, &msg, small, max);
            let y = at(l.save).y + 20.0 * s;
            label(&mut c, regular, &text, small, x0, y, u.text_dim);
        }
        let (x, y) = self.origin(t, w);
        (c, x, y)
    }

    // --- „Als Vorlage speichern …“ ----------------------------------------

    /// Dialog, Namensfeld und Knöpfe (Speichern, Abbrechen).
    pub(super) fn save_as_layout(&self, t: &Theme, w: &Win) -> (Rect, Rect, [Rect; 2]) {
        let f = self.frame(t, w);
        let s = w.scale;
        let (cw, ch) = ((460.0 * s).round(), (196.0 * s).round());
        let r = Rect::new(
            (f.x + (f.w - cw) * 0.5).round(),
            (f.y + (f.h - ch) * 0.4).round(),
            cw,
            ch,
        );
        let field = Rect::new(r.x + 90.0 * s, r.y + 58.0 * s, cw - 110.0 * s, 30.0 * s);
        let y = r.y + ch - 20.0 * s - 30.0 * s;
        let b2 = Rect::new(r.x + cw - 20.0 * s - 104.0 * s, y, 104.0 * s, 30.0 * s);
        let b1 = Rect::new(b2.x - 8.0 * s - 120.0 * s, y, 120.0 * s, 30.0 * s);
        (r, field, [b1, b2])
    }

    pub(super) fn save_as_hit(&self, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        let (r, field, b) = self.save_as_layout(t, w);
        if !r.contains(x, y) {
            return None;
        }
        if field.contains(x, y) {
            return Some(Target::Field(FieldId::PresetName));
        }
        Some(
            b.iter()
                .position(|b| b.contains(x, y))
                .map_or(Target::Item(usize::MAX), Target::Confirm),
        )
    }

    /// Speichern (0) gibt Name und Muster an `main.rs`, Abbrechen (1)
    /// schließt den Dialog.
    pub(super) fn save_as_click(&mut self, i: usize, cx: &mut Ctx, out: &mut Out) {
        if i != 0 {
            self.popup = None;
            self.edit = None;
            return;
        }
        let Some(pw) = self.pw.as_mut() else {
            return;
        };
        let name = pw.name.trim().to_string();
        if name.is_empty() {
            pw.error = Some("Bitte einen Namen eingeben.".into());
            return;
        }
        let Some(o) = cx.scene.model().attr().surface(pw.surface) else {
            return;
        };
        match &o.pattern {
            Some(p) => {
                self.save_preset = Some((name, p.clone(), o.color));
                out.save_preset = true;
            }
            None => pw.error = Some("Die Oberfläche hat kein Muster.".into()),
        }
        self.popup = None;
        self.edit = None;
    }

    pub(super) fn save_as_paint(
        &self,
        focus: usize,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
    ) -> Option<(Canvas, i32, i32)> {
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let (r, field, b) = self.save_as_layout(t, w);
        let mut c = Canvas::new((r.w + 2.0 * m) as usize, (r.h + 2.0 * m) as usize);
        widgets::panel(&mut c, Rect::new(m, m, r.w, r.h), s, t);
        let at = |q: Rect| Rect::new(q.x - r.x + m, q.y - r.y + m, q.w, q.h);
        label(
            &mut c,
            bold,
            "Als Vorlage speichern",
            t.size.font_title * s,
            m + 20.0 * s,
            m + 36.0 * s,
            u.text,
        );
        let font = t.size.font * s;
        label(
            &mut c,
            regular,
            "Name",
            font,
            m + 20.0 * s,
            at(field).y + 20.0 * s,
            u.text_dim,
        );
        let v = self.pw.as_ref().map_or(String::new(), |p| p.name.clone());
        let st = self.field_state(FieldId::PresetName, &v, "", at(field));
        widgets::text_field(&mut c, fonts, at(field), &st, s, t);
        // Kein Rückgängig: die Rückfrage steht gleich hier (paket-7 §1.2)
        let note = "Kommt in den Firmenkatalog; das lässt sich hier nicht rückgängig machen.";
        let small = t.size.font_small * s;
        for (k, line) in widgets::wrap(regular, note, small, r.w - 40.0 * s)
            .iter()
            .enumerate()
        {
            let y = at(field).y + field.h + 22.0 * s + k as f32 * 17.0 * s;
            label(&mut c, regular, line, small, m + 20.0 * s, y, u.text_dim);
        }
        for (i, (br, text)) in b.iter().zip(["Speichern", "Abbrechen"]).enumerate() {
            let st = ButtonState {
                hover: self.hover == Some(Target::Confirm(i)),
                pressed: self.pressed == Some(Target::Confirm(i)),
                active: focus == i,
                disabled: false,
            };
            widgets::button(&mut c, fonts, at(*br), text, st, s, t);
        }
        Some((c, (r.x - m) as i32, (r.y - m) as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Varianten: sechs verschiedene Startwerte, bei Mauerwerk Farben in
    /// ± 8 %, gültig nach Regel 57.
    #[test]
    fn varianten() {
        let p = proctex::masonry_default();
        let v = variants_of(&p, 0);
        assert_eq!(v.len(), VARIANTS);
        let mut seeds: Vec<u32> = v.iter().map(proctex::seed_of).collect();
        seeds.sort();
        seeds.dedup();
        assert_eq!(seeds.len(), VARIANTS);
        for x in &v {
            assert!(proctex::validate(x).is_ok());
            let (Pattern::Masonry { palette: a, .. }, Pattern::Masonry { palette: b, .. }) =
                (&p, x)
            else {
                panic!()
            };
            for k in 0..3 {
                for ch in 0..3 {
                    let (o, n) = (a[k].0[ch] as f32, b[k].0[ch] as f32);
                    assert!((n - o).abs() <= o * VARIANT_SHADE + 1.0, "{o} → {n}");
                }
            }
        }
        assert_ne!(variants_of(&p, 1), v, "⟳ zieht neue");
    }
}
