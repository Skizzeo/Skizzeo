//! Eigener TrueType-Leser: Umrisse der Glyphen aus einer `.ttf`-Datei.
//!
//! Gelesen werden nur die Tabellen, die für einfache Beschriftungen nötig sind
//! (`head`, `maxp`, `hhea`, `hmtx`, `cmap`, `loca`, `glyf`). Keine Hinting-
//! Anweisungen, keine Unterschneidung. Zusammengesetzte Glyphen (z. B. Umlaute)
//! werden aufgelöst.
//!
//! Gezeichnete Glyphen merkt sich die Schrift als Flächenmaske (Glyphen-Cache,
//! Review U6a): Dieselbe Glyphe in derselben Größe und an derselben
//! Bruchteil-Lage wird nicht neu gerastert.

use crate::{accumulate_line, Canvas, Path, Rgba};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Glyphe, Schriftgröße und Bruchteil der Lage (x, y), je als Bitmuster:
/// nur genau gleiche Eingaben teilen sich eine Maske.
type GlyphKey = (u16, u32, u32, u32);

/// Vorzeichenbehaftete Fläche je Pixel einer Glyphe (Laufsumme des
/// Akkumulators, noch ohne Betrag und Begrenzung auf 1). Weil diese Fläche
/// linear ist, ergibt die Summe der Masken eines Textes dieselbe Abdeckung
/// wie das Füllen des ganzen Textumrisses.
struct Mask {
    /// Lage der linken oberen Ecke zum ganzzahligen Anteil der Stiftlage.
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    area: Vec<f32>,
}

/// Höchstens so viele Masken; danach beginnt der Cache von vorn.
const CACHE_MAX: usize = 4096;

pub struct Font {
    data: Vec<u8>,
    units_per_em: f32,
    ascender: f32,
    descender: f32,
    long_loca: bool,
    num_glyphs: u16,
    num_hmetrics: u16,
    loca: usize,
    glyf: usize,
    hmtx: usize,
    cmap: Cmap,
    cache: RefCell<HashMap<GlyphKey, Rc<Mask>>>,
}

enum Cmap {
    /// Format 4 (Unicode BMP): Versatz der Untertabelle.
    Segments(usize),
    /// Format 12 (volles Unicode): Versatz der Untertabelle.
    Groups(usize),
}

mod teilmenge;

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(o)?, *d.get(o + 1)?]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    u16_at(d, o).map(|v| v as i16)
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *d.get(o)?,
        *d.get(o + 1)?,
        *d.get(o + 2)?,
        *d.get(o + 3)?,
    ]))
}

impl Font {
    /// Liest eine TrueType-Datei (Umrisse mit quadratischen Kurven).
    ///
    /// Nur eine einzelne Schrift mit `glyf`-Umrissen: eine Sammlung
    /// (`.ttc`, Kennung `ttcf`) und eine CFF-OpenType (`OTTO`) lehnt sie ab.
    /// Darauf verlassen sich [`Font::metrik_milli`] (Tabellenverzeichnis ab
    /// Byte 4) und das PDF, das daraus eine Teilmenge als `FontFile2` einbettet.
    pub fn parse(data: Vec<u8>) -> Option<Font> {
        let d = &data;
        if !matches!(d.get(..4)?, [0, 1, 0, 0] | b"true") {
            return None;
        }
        let num_tables = u16_at(d, 4)? as usize;
        let table = |tag: &[u8; 4]| -> Option<usize> {
            (0..num_tables).find_map(|i| {
                let rec = 12 + i * 16;
                (d.get(rec..rec + 4)? == tag).then(|| u32_at(d, rec + 8).map(|v| v as usize))?
            })
        };
        let head = table(b"head")?;
        let maxp = table(b"maxp")?;
        let hhea = table(b"hhea")?;
        let hmtx = table(b"hmtx")?;
        let cmap_t = table(b"cmap")?;
        let loca = table(b"loca")?;
        let glyf = table(b"glyf")?;

        let n = u16_at(d, cmap_t + 2)? as usize;
        let mut cmap = None;
        for i in 0..n {
            let rec = cmap_t + 4 + i * 8;
            let (pid, eid) = (u16_at(d, rec)?, u16_at(d, rec + 2)?);
            let off = cmap_t + u32_at(d, rec + 4)? as usize;
            match (pid, eid, u16_at(d, off)?) {
                (3, 10, 12) | (0, 4, 12) => {
                    cmap = Some(Cmap::Groups(off));
                    break;
                }
                (3, 1, 4) | (0, 3, 4) if cmap.is_none() => cmap = Some(Cmap::Segments(off)),
                _ => {}
            }
        }
        Some(Font {
            units_per_em: u16_at(d, head + 18)? as f32,
            long_loca: i16_at(d, head + 50)? != 0,
            num_glyphs: u16_at(d, maxp + 4)?,
            ascender: i16_at(d, hhea + 4)? as f32,
            descender: i16_at(d, hhea + 6)? as f32,
            num_hmetrics: u16_at(d, hhea + 34)?,
            loca,
            glyf,
            hmtx,
            cmap: cmap?,
            data,
            cache: RefCell::default(),
        })
    }

    /// Lädt die erste lesbare Schrift aus dem Schriftenordner des Systems.
    pub fn system(names: &[&str]) -> Option<Font> {
        let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        names.iter().find_map(|n| {
            let path = format!("{dir}\\Fonts\\{n}");
            std::fs::read(path).ok().and_then(Font::parse)
        })
    }

    fn glyph_index(&self, c: char) -> u16 {
        let d = &self.data;
        let c = c as u32;
        let r = match self.cmap {
            Cmap::Groups(off) => (|| {
                let n = u32_at(d, off + 12)? as usize;
                (0..n).find_map(|i| {
                    let g = off + 16 + i * 12;
                    let (start, end, gid) = (u32_at(d, g)?, u32_at(d, g + 4)?, u32_at(d, g + 8)?);
                    (start..=end).contains(&c).then(|| (gid + c - start) as u16)
                })
            })(),
            Cmap::Segments(off) => (|| {
                if c > 0xFFFF {
                    return None;
                }
                let seg2 = u16_at(d, off + 6)? as usize;
                let ends = off + 14;
                let starts = ends + seg2 + 2;
                let deltas = starts + seg2;
                let ranges = deltas + seg2;
                (0..seg2 / 2).find_map(|i| {
                    let end = u16_at(d, ends + i * 2)? as u32;
                    if c > end {
                        return None;
                    }
                    let start = u16_at(d, starts + i * 2)? as u32;
                    if c < start {
                        return Some(0);
                    }
                    let delta = u16_at(d, deltas + i * 2)?;
                    let ro = u16_at(d, ranges + i * 2)? as usize;
                    if ro == 0 {
                        return Some((c as u16).wrapping_add(delta));
                    }
                    let at = ranges + i * 2 + ro + (c - start) as usize * 2;
                    let g = u16_at(d, at)?;
                    Some(if g == 0 { 0 } else { g.wrapping_add(delta) })
                })
            })(),
        };
        r.unwrap_or(0)
    }

    fn advance(&self, gid: u16) -> f32 {
        let i = gid.min(self.num_hmetrics.saturating_sub(1)) as usize;
        u16_at(&self.data, self.hmtx + i * 4).unwrap_or(0) as f32
    }

    fn glyph_range(&self, gid: u16) -> Option<(usize, usize)> {
        if gid >= self.num_glyphs {
            return None;
        }
        let d = &self.data;
        let i = gid as usize;
        let (a, b) = if self.long_loca {
            (
                u32_at(d, self.loca + i * 4)? as usize,
                u32_at(d, self.loca + i * 4 + 4)? as usize,
            )
        } else {
            (
                u16_at(d, self.loca + i * 2)? as usize * 2,
                u16_at(d, self.loca + i * 2 + 2)? as usize * 2,
            )
        };
        (b > a).then_some((self.glyf + a, self.glyf + b))
    }

    /// Umriss einer Glyphe in Schrifteinheiten; `m` bildet (x, y) ab (2×2 + Versatz).
    fn outline(&self, gid: u16, m: [f32; 6], out: &mut Path, depth: u32) -> Option<()> {
        let (start, _) = self.glyph_range(gid)?;
        let d = &self.data;
        let contours = i16_at(d, start)?;
        let tf = |x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        if contours < 0 {
            if depth > 4 {
                return None;
            }
            // Zusammengesetzte Glyphe
            let mut o = start + 10;
            loop {
                let flags = u16_at(d, o)?;
                let sub = u16_at(d, o + 2)?;
                o += 4;
                let (dx, dy) = if flags & 1 != 0 {
                    let r = (i16_at(d, o)? as f32, i16_at(d, o + 2)? as f32);
                    o += 4;
                    r
                } else {
                    let r = (d[o] as i8 as f32, d[o + 1] as i8 as f32);
                    o += 2;
                    r
                };
                let (mut a, mut b, mut c, mut e) = (1.0, 0.0, 0.0, 1.0);
                let f2 = |o: usize| i16_at(d, o).map(|v| v as f32 / 16384.0);
                if flags & 0x8 != 0 {
                    a = f2(o)?;
                    e = a;
                    o += 2;
                } else if flags & 0x40 != 0 {
                    a = f2(o)?;
                    e = f2(o + 2)?;
                    o += 4;
                } else if flags & 0x80 != 0 {
                    a = f2(o)?;
                    b = f2(o + 2)?;
                    c = f2(o + 4)?;
                    e = f2(o + 6)?;
                    o += 8;
                }
                // Nur Versatz in Koordinaten (Punktbezüge werden nicht unterstützt)
                let (dx, dy) = if flags & 2 != 0 { (dx, dy) } else { (0.0, 0.0) };
                let local = [a, b, c, e, dx, dy];
                let comb = [
                    m[0] * local[0] + m[2] * local[1],
                    m[1] * local[0] + m[3] * local[1],
                    m[0] * local[2] + m[2] * local[3],
                    m[1] * local[2] + m[3] * local[3],
                    m[0] * dx + m[2] * dy + m[4],
                    m[1] * dx + m[3] * dy + m[5],
                ];
                self.outline(sub, comb, out, depth + 1);
                if flags & 0x20 == 0 {
                    break;
                }
            }
            return Some(());
        }

        let nc = contours as usize;
        let mut ends = Vec::with_capacity(nc);
        for i in 0..nc {
            ends.push(u16_at(d, start + 10 + i * 2)? as usize);
        }
        let npts = ends.last().map_or(0, |e| e + 1);
        let ins_len = u16_at(d, start + 10 + nc * 2)? as usize;
        let mut o = start + 12 + nc * 2 + ins_len;
        let mut flags = Vec::with_capacity(npts);
        while flags.len() < npts {
            let f = *d.get(o)?;
            o += 1;
            flags.push(f);
            if f & 8 != 0 {
                let n = *d.get(o)?;
                o += 1;
                for _ in 0..n {
                    flags.push(f);
                }
            }
        }
        flags.truncate(npts);
        let mut coords = |short: u8, same: u8| -> Option<Vec<f32>> {
            let mut v = Vec::with_capacity(npts);
            let mut acc = 0i32;
            for &f in &flags {
                if f & short != 0 {
                    let b = *d.get(o)? as i32;
                    o += 1;
                    acc += if f & same != 0 { b } else { -b };
                } else if f & same == 0 {
                    acc += i16_at(d, o)? as i32;
                    o += 2;
                }
                v.push(acc as f32);
            }
            Some(v)
        };
        let xs = coords(2, 16)?;
        let ys = coords(4, 32)?;

        let mut s = 0;
        for &e in &ends {
            let pts: Vec<(f32, f32, bool)> =
                (s..=e).map(|i| (xs[i], ys[i], flags[i] & 1 != 0)).collect();
            s = e + 1;
            if pts.len() < 2 {
                continue;
            }
            let n = pts.len();
            // Startpunkt: erster Punkt auf der Kurve, sonst Mitte zweier Kontrollpunkte
            let first_on = pts.iter().position(|p| p.2);
            let (start_pt, k0) = match first_on {
                Some(i) => ((pts[i].0, pts[i].1), i),
                None => (
                    ((pts[0].0 + pts[1].0) * 0.5, (pts[0].1 + pts[1].1) * 0.5),
                    0,
                ),
            };
            let p0 = tf(start_pt.0, start_pt.1);
            out.move_to(p0.0, p0.1);
            let mut ctrl: Option<(f32, f32)> = None;
            for step in 1..=n {
                let (x, y, on) = pts[(k0 + step) % n];
                if on {
                    let p = tf(x, y);
                    match ctrl.take() {
                        Some(c) => {
                            let c = tf(c.0, c.1);
                            out.quad_to(c, p);
                        }
                        None => {
                            out.line_to(p.0, p.1);
                        }
                    }
                } else {
                    if let Some(c) = ctrl {
                        let mid = ((c.0 + x) * 0.5, (c.1 + y) * 0.5);
                        let (cc, mm) = (tf(c.0, c.1), tf(mid.0, mid.1));
                        out.quad_to(cc, mm);
                    }
                    ctrl = Some((x, y));
                }
            }
            if let Some(c) = ctrl {
                let (cc, pp) = (tf(c.0, c.1), p0);
                out.quad_to(cc, pp);
            }
            out.close();
        }
        Some(())
    }

    /// Glyphe zu einem Zeichen (0: fehlt in der Schrift); für das PDF.
    pub fn glyph(&self, c: char) -> u16 {
        self.glyph_index(c)
    }

    /// Vorschub einer Glyphe in Tausendstel des Gevierts (PDF `/W`).
    pub fn advance_milli(&self, gid: u16) -> f32 {
        self.advance(gid) * 1000.0 / self.units_per_em
    }

    /// Ober- und Unterlänge und Umgrenzung (x0, y0, x1, y1) aller Glyphen
    /// in Tausendstel des Gevierts (PDF `/FontDescriptor`).
    pub fn metrik_milli(&self) -> (f32, f32, [f32; 4]) {
        let k = 1000.0 / self.units_per_em;
        let d = &self.data;
        let n = u16_at(d, 4).unwrap_or(0) as usize;
        let head = (0..n).find_map(|i| {
            let rec = 12 + i * 16;
            (d.get(rec..rec + 4)? == b"head").then(|| u32_at(d, rec + 8))?
        });
        let bbox = head.map_or([0.0, self.descender, 1000.0, self.ascender], |h| {
            let h = h as usize;
            let v = |o: usize| f32::from(i16_at(d, h + o).unwrap_or(0));
            [v(36), v(38), v(40), v(42)]
        });
        (self.ascender * k, self.descender * k, bbox.map(|v| v * k))
    }

    /// Die ganze Schriftdatei (zum Einbetten ins PDF).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Breite eines Textes in Pixeln bei Schriftgröße `px` (Höhe des Gevierts).
    pub fn width(&self, text: &str, px: f32) -> f32 {
        let s = px / self.units_per_em;
        text.chars()
            .map(|c| self.advance(self.glyph_index(c)))
            .sum::<f32>()
            * s
    }

    /// Oberlänge minus Unterlänge in Pixeln.
    pub fn line_height(&self, px: f32) -> f32 {
        (self.ascender - self.descender) * px / self.units_per_em
    }

    /// Höhe der Großbuchstaben in Pixeln (für senkrechtes Zentrieren).
    pub fn cap_height(&self, px: f32) -> f32 {
        0.7 * px
    }

    /// Umriss eines Textes mit Grundlinie bei `(x, y)` (Pixel, y nach unten).
    pub fn text_path(&self, text: &str, px: f32, x: f32, y: f32) -> Path {
        let s = px / self.units_per_em;
        let mut out = Path::new();
        let mut pen = x;
        for c in text.chars() {
            let g = self.glyph_index(c);
            self.outline(g, [s, 0.0, 0.0, -s, pen, y], &mut out, 0);
            pen += self.advance(g) * s;
        }
        out
    }

    /// Zeichnet Text mit Grundlinie bei `(x, y)`. Die Glyphen kommen aus dem
    /// Cache; das Bild gleicht dem Füllen von [`Font::text_path`] bis auf
    /// Gleitkomma-Rundung (höchstens eine Stufe von 255).
    pub fn draw(&self, c: &mut Canvas, text: &str, px: f32, x: f32, y: f32, color: Rgba) {
        let (ox, oy) = c.origin;
        // Nur ganz innerhalb der Leinwand und bei ganzzahligem Ursprung; sonst
        // wie bisher über den Umriss (Begrenzung am Rand, seltene Fälle)
        if ox.fract() != 0.0 || oy.fract() != 0.0 || !px.is_finite() {
            c.fill(&self.text_path(text, px, x, y), color);
            return;
        }
        let s = px / self.units_per_em;
        let yl = y - oy;
        let (yi, fy) = (yl.floor(), yl - yl.floor());
        let mut glyphs = Vec::with_capacity(text.len());
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut pen = x - ox;
        for ch in text.chars() {
            let g = self.glyph_index(ch);
            let (xi, fx) = (pen.floor(), pen - pen.floor());
            let m = self.mask(g, px, fx, fy, s);
            if m.w > 0 {
                let (gx, gy) = (xi as i32 + m.x, yi as i32 + m.y);
                (x0, y0) = (x0.min(gx), y0.min(gy));
                (x1, y1) = (x1.max(gx + m.w as i32), y1.max(gy + m.h as i32));
                glyphs.push((gx, gy, m));
            }
            pen += self.advance(g) * s;
        }
        if glyphs.is_empty() {
            return;
        }
        // Ganz außerhalb (etwa beim Zeichnen eines Streifens): nichts zu tun
        if x1 <= 0 || y1 <= 0 || x0 >= c.width as i32 || y0 >= c.height as i32 {
            return;
        }
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut area = vec![0.0f32; w * h];
        for (gx, gy, m) in &glyphs {
            let (dx, dy) = ((gx - x0) as usize, (gy - y0) as usize);
            for r in 0..m.h {
                let src = &m.area[r * m.w..(r + 1) * m.w];
                let dst = &mut area[(dy + r) * w + dx..(dy + r) * w + dx + m.w];
                for (d, a) in dst.iter_mut().zip(src) {
                    *d += a;
                }
            }
        }
        // Am Rand der Leinwand (Teilbild): nur der sichtbare Teil, mit
        // denselben Masken wie im ganzen Bild
        let (cx0, cy0) = (x0.max(0), y0.max(0));
        let (cx1, cy1) = (x1.min(c.width as i32), y1.min(c.height as i32));
        if (cx0, cy0, cx1, cy1) == (x0, y0, x1, y1) {
            c.blend_area(x0 as usize, y0 as usize, w, h, &area, color);
            return;
        }
        let (vw, vh) = ((cx1 - cx0) as usize, (cy1 - cy0) as usize);
        let (sx, sy) = ((cx0 - x0) as usize, (cy0 - y0) as usize);
        let mut part = Vec::with_capacity(vw * vh);
        for r in 0..vh {
            part.extend_from_slice(&area[(sy + r) * w + sx..][..vw]);
        }
        c.blend_area(cx0 as usize, cy0 as usize, vw, vh, &part, color);
    }

    /// Maske der Glyphe `g` an der Bruchteil-Lage `(fx, fy)`, aus dem Cache
    /// oder neu gerastert wie in [`Canvas::fill`].
    fn mask(&self, g: u16, px: f32, fx: f32, fy: f32, s: f32) -> Rc<Mask> {
        let key = (g, px.to_bits(), fx.to_bits(), fy.to_bits());
        if let Some(m) = self.cache.borrow().get(&key) {
            return m.clone();
        }
        let mut path = Path::new();
        self.outline(g, [s, 0.0, 0.0, -s, fx, fy], &mut path, 0);
        let polys = path.flatten(0.2);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in polys.iter().flatten() {
            (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
        }
        let m = if x0 > x1 || y0 > y1 {
            Mask {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                area: Vec::new(),
            }
        } else {
            // Ganzzahlig verschoben: Bruchteile und damit die Flächen bleiben
            let (mx, my) = (x0.floor() - 1.0, y0.floor() - 1.0);
            let w = (x1 - mx).ceil() as usize + 2;
            let h = (y1 - my).ceil() as usize + 1;
            let stride = w + 2;
            let mut acc = vec![0.0f32; stride * h];
            for poly in &polys {
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    let a = crate::pt(a.x - mx, a.y - my);
                    let b = crate::pt(b.x - mx, b.y - my);
                    accumulate_line(&mut acc, stride, w, h, a, b, None);
                }
            }
            let mut area = vec![0.0f32; w * h];
            for r in 0..h {
                let mut sum = 0.0f32;
                for x in 0..w {
                    sum += acc[r * stride + x];
                    area[r * w + x] = sum;
                }
            }
            Mask {
                x: mx as i32,
                y: my as i32,
                w,
                h,
                area,
            }
        };
        let m = Rc::new(m);
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, m.clone());
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eine Schrift für die Tests: unter Windows aus dem System, sonst eine
    /// verbreitete freie Schrift. Ohne Schrift prüfen die Tests nichts.
    fn some_font() -> Option<Font> {
        Font::system(&["segoeui.ttf", "arial.ttf"]).or_else(|| {
            [
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ]
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(Font::parse))
        })
    }

    /// Review 3bk, Hinweis 5: Sammlung und CFF-OpenType sind keine Schrift
    /// für Leinwand und PDF; eine einfache TrueType schon.
    #[test]
    fn nur_einfache_truetype() {
        let Some(f) = some_font() else {
            return;
        };
        let ttf = f.data().to_vec();
        assert!(Font::parse(ttf.clone()).is_some());
        for kennung in [*b"ttcf", *b"OTTO", [0, 2, 0, 0]] {
            let mut d = ttf.clone();
            d[..4].copy_from_slice(&kennung);
            assert!(Font::parse(d).is_none(), "{kennung:?}");
        }
        // Eine echte Sammlung: Kopf „ttcf“ mit Verweis auf die Schrift
        let mut ttc = b"ttcf\0\x01\0\0\0\0\0\x01\0\0\0\x10".to_vec();
        ttc.extend(&ttf);
        assert!(Font::parse(ttc).is_none());
    }

    fn worst(a: &Canvas, b: &Canvas) -> u8 {
        let (a, b) = (a.to_premul_rgba8(), b.to_premul_rgba8());
        a.iter()
            .zip(&b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    /// U6a: Text aus dem Glyphen-Cache gleicht dem gefüllten Textumriss bis
    /// auf eine Rundungsstufe, auch beim zweiten Mal (aus dem Cache), an
    /// Bruchteil-Lagen, halb außerhalb, mit verschobenem Ursprung und mit
    /// halbdurchsichtiger Farbe.
    #[test]
    fn cache_gleicht_dem_umriss() {
        let Some(f) = some_font() else {
            eprintln!("keine Schrift gefunden, Test übersprungen");
            return;
        };
        let cases: [(&str, f32, f32, f32, Rgba); 7] = [
            (
                "Außenwand AW-003 · 12,45 m²",
                13.0,
                10.0,
                20.0,
                Rgba(20, 20, 20, 255),
            ),
            ("Länge · Stück", 19.5, 3.37, 41.6, Rgba(200, 40, 40, 255)),
            (
                "Mengenermittlung",
                28.5,
                100.25,
                70.5,
                Rgba(10, 10, 200, 140),
            ),
            ("Wird gekürzt…", 15.0, -6.5, 90.0, Rgba(0, 0, 0, 255)),
            ("unten raus", 16.0, 30.0, 118.0, Rgba(0, 0, 0, 255)),
            ("ganz draußen", 16.0, 30.0, 160.0, Rgba(0, 0, 0, 255)),
            (
                "ÄÖÜ äöü ß 0123456789",
                11.0,
                160.7,
                33.3,
                Rgba(60, 60, 60, 255),
            ),
        ];
        for origin in [(0.0, 0.0), (0.0, 17.0)] {
            for round in 0..2 {
                // Vorbild: Umriss auf einer größeren Leinwand, dann der
                // Ausschnitt (am Rand ist das genauer als die Begrenzung beim
                // Füllen)
                const M: usize = 40;
                let (mut a, mut big) =
                    (Canvas::new(260, 120), Canvas::new(260 + 2 * M, 120 + 2 * M));
                a.set_origin(origin.0, origin.1);
                big.set_origin(origin.0 - M as f32, origin.1 - M as f32);
                for c in [&mut a, &mut big] {
                    c.clear(Rgba(240, 238, 230, 255));
                }
                for (text, px, x, y, col) in cases {
                    let y = y + origin.1;
                    f.draw(&mut a, text, px, x, y, col);
                    big.fill(&f.text_path(text, px, x, y), col);
                }
                let mut b = Canvas::new(260, 120);
                for r in 0..120 {
                    let src = &big.px[(r + M) * big.width + M..][..260];
                    b.px[r * 260..(r + 1) * 260].copy_from_slice(src);
                }
                let d = worst(&a, &b);
                assert!(d <= 1, "Ursprung {origin:?}, Durchgang {round}: {d} Stufen");
            }
        }
        assert!(!f.cache.borrow().is_empty(), "Masken gemerkt");
    }
}
