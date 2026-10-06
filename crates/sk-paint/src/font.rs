//! Eigener TrueType-Leser: Umrisse der Glyphen aus einer `.ttf`-Datei.
//!
//! Gelesen werden nur die Tabellen, die für einfache Beschriftungen nötig sind
//! (`head`, `maxp`, `hhea`, `hmtx`, `cmap`, `loca`, `glyf`). Keine Hinting-
//! Anweisungen, keine Unterschneidung. Zusammengesetzte Glyphen (z. B. Umlaute)
//! werden aufgelöst.

use crate::{Canvas, Path, Rgba};

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
}

enum Cmap {
    /// Format 4 (Unicode BMP): Versatz der Untertabelle.
    Segments(usize),
    /// Format 12 (volles Unicode): Versatz der Untertabelle.
    Groups(usize),
}

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
    pub fn parse(data: Vec<u8>) -> Option<Font> {
        let d = &data;
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

    /// Zeichnet Text mit Grundlinie bei `(x, y)`.
    pub fn draw(&self, c: &mut Canvas, text: &str, px: f32, x: f32, y: f32, color: Rgba) {
        c.fill(&self.text_path(text, px, x, y), color);
    }
}
