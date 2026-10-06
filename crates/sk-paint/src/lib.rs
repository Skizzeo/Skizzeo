//! Eigene 2D-Vektorgrafik: Pfade, kantengeglättete Füllung, PNG- und SVG-Ausgabe.
//!
//! Die Füllung arbeitet mit exakter Flächenabdeckung pro Pixel (vorzeichenbehaftete
//! Flächenakkumulation). Gegenläufig orientierte Teilpfade stanzen Löcher aus.

#![forbid(unsafe_code)]

pub mod font;
mod png;

pub use png::encode_png;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

pub const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move(Pt),
    Line(Pt),
    Cubic(Pt, Pt, Pt),
    Close,
}

/// Vektorpfad aus Teilpfaden. Jeder Teilpfad beginnt mit `Move`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub cmds: Vec<Cmd>,
}

/// Kreisnäherung für Viertelbögen mit kubischen Bézierkurven.
const KAPPA: f32 = 0.552_284_8;

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Path {
        self.cmds.push(Cmd::Move(pt(x, y)));
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Path {
        self.cmds.push(Cmd::Line(pt(x, y)));
        self
    }

    /// Quadratische Bézierkurve (als kubische gespeichert).
    pub fn quad_to(&mut self, c: (f32, f32), p: (f32, f32)) -> &mut Path {
        let p0 = match self.cmds.last() {
            Some(Cmd::Move(q)) | Some(Cmd::Line(q)) | Some(Cmd::Cubic(_, _, q)) => *q,
            _ => pt(c.0, c.1),
        };
        let k = 2.0 / 3.0;
        let c1 = (p0.x + k * (c.0 - p0.x), p0.y + k * (c.1 - p0.y));
        let c2 = (p.0 + k * (c.0 - p.0), p.1 + k * (c.1 - p.1));
        self.cubic_to(c1, c2, p)
    }

    pub fn cubic_to(&mut self, c1: (f32, f32), c2: (f32, f32), p: (f32, f32)) -> &mut Path {
        self.cmds
            .push(Cmd::Cubic(pt(c1.0, c1.1), pt(c2.0, c2.1), pt(p.0, p.1)));
        self
    }

    pub fn close(&mut self) -> &mut Path {
        self.cmds.push(Cmd::Close);
        self
    }

    /// Rechteck mit abgerundeten Ecken, im Uhrzeigersinn (Bildschirmkoordinaten).
    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) -> &mut Path {
        let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
        let k = r * KAPPA;
        let (x1, y1) = (x + w, y + h);
        self.move_to(x + r, y).line_to(x1 - r, y);
        self.cubic_to((x1 - r + k, y), (x1, y + r - k), (x1, y + r));
        self.line_to(x1, y1 - r);
        self.cubic_to((x1, y1 - r + k), (x1 - r + k, y1), (x1 - r, y1));
        self.line_to(x + r, y1);
        self.cubic_to((x + r - k, y1), (x, y1 - r + k), (x, y1 - r));
        self.line_to(x, y + r);
        self.cubic_to((x, y + r - k), (x + r - k, y), (x + r, y));
        self.close()
    }

    /// Gleiches Rechteck, aber gegen den Uhrzeigersinn: stanzt innerhalb eines
    /// anderen Pfades ein Loch aus.
    pub fn rounded_rect_hole(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) -> &mut Path {
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, r);
        self.cmds.extend(p.reversed().cmds);
        self
    }

    /// Strich zwischen zwei Punkten als gefülltes Rechteck mit flachen Enden.
    pub fn segment(&mut self, a: (f32, f32), b: (f32, f32), width: f32) -> &mut Path {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        self.move_to(a.0 + nx, a.1 + ny)
            .line_to(b.0 + nx, b.1 + ny)
            .line_to(b.0 - nx, b.1 - ny)
            .line_to(a.0 - nx, a.1 - ny)
            .close()
    }

    /// Kehrt die Laufrichtung aller Teilpfade um.
    pub fn reversed(&self) -> Path {
        let mut out = Path::new();
        for sub in self.subpaths() {
            // Segmente als (Start, Befehl) sammeln und rückwärts ablaufen
            let mut segs: Vec<(Pt, Cmd)> = Vec::new();
            let mut cur = pt(0.0, 0.0);
            let mut start = cur;
            for c in sub {
                match *c {
                    Cmd::Move(p) => {
                        cur = p;
                        start = p;
                    }
                    Cmd::Line(p) => {
                        segs.push((cur, Cmd::Line(p)));
                        cur = p;
                    }
                    Cmd::Cubic(a, b, p) => {
                        segs.push((cur, Cmd::Cubic(a, b, p)));
                        cur = p;
                    }
                    Cmd::Close => {}
                }
            }
            if cur != start {
                segs.push((cur, Cmd::Line(start)));
            }
            out.cmds.push(Cmd::Move(start));
            for (from, c) in segs.iter().rev() {
                match *c {
                    Cmd::Line(_) => out.cmds.push(Cmd::Line(*from)),
                    Cmd::Cubic(a, b, _) => out.cmds.push(Cmd::Cubic(b, a, *from)),
                    _ => {}
                }
            }
            out.cmds.push(Cmd::Close);
        }
        out
    }

    fn subpaths(&self) -> Vec<&[Cmd]> {
        let mut out = Vec::new();
        let mut begin = 0;
        for (i, c) in self.cmds.iter().enumerate() {
            if matches!(c, Cmd::Move(_)) && i > begin {
                out.push(&self.cmds[begin..i]);
                begin = i;
            }
        }
        if begin < self.cmds.len() {
            out.push(&self.cmds[begin..]);
        }
        out
    }

    /// Wendet `p * scale + offset` auf alle Punkte an.
    pub fn transformed(&self, scale: f32, dx: f32, dy: f32) -> Path {
        let t = |p: Pt| pt(p.x * scale + dx, p.y * scale + dy);
        Path {
            cmds: self
                .cmds
                .iter()
                .map(|c| match *c {
                    Cmd::Move(p) => Cmd::Move(t(p)),
                    Cmd::Line(p) => Cmd::Line(t(p)),
                    Cmd::Cubic(a, b, p) => Cmd::Cubic(t(a), t(b), t(p)),
                    Cmd::Close => Cmd::Close,
                })
                .collect(),
        }
    }

    /// Zerlegt den Pfad in geschlossene Polygone (Kurven mit Toleranz `tol` in Pixeln).
    pub fn flatten(&self, tol: f32) -> Vec<Vec<Pt>> {
        let mut polys: Vec<Vec<Pt>> = Vec::new();
        let mut cur: Vec<Pt> = Vec::new();
        let mut last = pt(0.0, 0.0);
        for c in &self.cmds {
            match *c {
                Cmd::Move(p) => {
                    if cur.len() > 2 {
                        polys.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    cur.push(p);
                    last = p;
                }
                Cmd::Line(p) => {
                    cur.push(p);
                    last = p;
                }
                Cmd::Cubic(a, b, p) => {
                    // Kontrollpolygonlänge bestimmt die Unterteilung
                    let l = dist(last, a) + dist(a, b) + dist(b, p);
                    let n = ((l / tol.max(0.01)).sqrt().ceil() as usize).clamp(1, 256);
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        let w0 = u * u * u;
                        let w1 = 3.0 * u * u * t;
                        let w2 = 3.0 * u * t * t;
                        let w3 = t * t * t;
                        cur.push(pt(
                            w0 * last.x + w1 * a.x + w2 * b.x + w3 * p.x,
                            w0 * last.y + w1 * a.y + w2 * b.y + w3 * p.y,
                        ));
                    }
                    last = p;
                }
                Cmd::Close => {
                    if cur.len() > 2 {
                        polys.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
            }
        }
        if cur.len() > 2 {
            polys.push(cur);
        }
        polys
    }

    /// SVG-Pfadbeschreibung (`d`-Attribut).
    pub fn to_svg_d(&self) -> String {
        let mut s = String::new();
        for c in &self.cmds {
            match *c {
                Cmd::Move(p) => s += &format!("M{} {} ", n(p.x), n(p.y)),
                Cmd::Line(p) => s += &format!("L{} {} ", n(p.x), n(p.y)),
                Cmd::Cubic(a, b, p) => {
                    s += &format!(
                        "C{} {} {} {} {} {} ",
                        n(a.x),
                        n(a.y),
                        n(b.x),
                        n(b.y),
                        n(p.x),
                        n(p.y)
                    )
                }
                Cmd::Close => s += "Z ",
            }
        }
        s.trim_end().to_string()
    }
}

fn n(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    format!("{}", r)
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Farbe in sRGB, 0..=255, nicht vormultipliziert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba(r, g, b, 255)
    }

    /// Deckende Farbe aus Rot, Grün, Blau (z. B. aus den Attributtabellen).
    pub const fn from_rgb8([r, g, b]: [u8; 3]) -> Rgba {
        Rgba(r, g, b, 255)
    }

    /// Farbe als 0..1 (nicht vormultipliziert).
    pub fn to_f32(self) -> [f32; 4] {
        [
            self.0 as f32 / 255.0,
            self.1 as f32 / 255.0,
            self.2 as f32 / 255.0,
            self.3 as f32 / 255.0,
        ]
    }

    /// Farbe aus 0..1, gerundet.
    pub fn from_f32(c: [f32; 4]) -> Rgba {
        let u = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgba(u(c[0]), u(c[1]), u(c[2]), u(c[3]))
    }
}

/// RGBA-Bild mit vormultiplizierten Farbwerten (0..1).
#[derive(Clone, Debug)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    px: Vec<[f32; 4]>,
    acc: Vec<f32>,
    /// Lage der Leinwand im Gesamtbild: gefüllte Pfade werden um diesen
    /// Betrag verschoben (Ausschnitt eines größeren Bildes zeichnen).
    origin: (f32, f32),
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Canvas {
        Canvas {
            width,
            height,
            px: vec![[0.0; 4]; width * height],
            acc: Vec::new(),
            origin: (0.0, 0.0),
        }
    }

    /// Die Leinwand zeigt ab jetzt den Ausschnitt ab `(x, y)` eines größeren
    /// Bildes: Pfade in dessen Koordinaten landen an der richtigen Stelle.
    /// Ganzzahlig gewählt, gleicht der Ausschnitt dem ganzen Bild bis auf
    /// Rundung (höchstens eine Stufe bei Kurven mit Bruchteilkoordinaten).
    pub fn set_origin(&mut self, x: f32, y: f32) {
        self.origin = (x, y);
    }

    /// Senkrechter Bereich des Gesamtbildes, den diese Leinwand zeigt (von,
    /// bis). Was ganz außerhalb liegt, braucht nicht gezeichnet zu werden.
    pub fn visible_y(&self) -> (f32, f32) {
        (self.origin.1, self.origin.1 + self.height as f32)
    }

    pub fn clear(&mut self, c: Rgba) {
        let v = premul(c, 1.0);
        self.px.iter_mut().for_each(|p| *p = v);
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
        let mut p = Path::new();
        p.move_to(x, y)
            .line_to(x + w, y)
            .line_to(x + w, y + h)
            .line_to(x, y + h)
            .close();
        self.fill(&p, c);
    }

    /// Füllt den Pfad kantengeglättet (Nonzero-Regel für nicht überlappende Teilpfade).
    pub fn fill(&mut self, path: &Path, c: Rgba) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        if self.origin != (0.0, 0.0) {
            let (ox, oy) = self.origin;
            let moved = path.transformed(1.0, -ox, -oy);
            self.origin = (0.0, 0.0);
            self.fill(&moved, c);
            self.origin = (ox, oy);
            return;
        }
        let stride = w + 2;
        // Akkumulator bleibt zwischen Aufrufen genullt; bearbeitet wird nur der
        // umschließende Bereich des Pfads
        if self.acc.len() != stride * h {
            self.acc.clear();
            self.acc.resize(stride * h, 0.0);
        }
        let polys = path.flatten(0.2);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in polys.iter().flatten() {
            (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
        }
        if x0 > x1 || x1 < 0.0 || y1 < 0.0 || x0 >= w as f32 || y0 >= h as f32 {
            return;
        }
        let (cx0, cy0) = (x0.max(0.0) as usize, y0.max(0.0) as usize);
        let cx1 = ((x1.ceil().max(0.0) as usize) + 2).min(stride);
        let cy1 = (y1.ceil().max(0.0) as usize).min(h);
        for poly in &polys {
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                accumulate_line(&mut self.acc, stride, w, h, a, b);
            }
        }
        // Volle Abdeckung ist der häufigste Fall: Farbe einmal vorausrechnen,
        // deckende Farbe einfach schreiben
        let full = premul(c, 1.0);
        let opaque = full[3] >= 1.0;
        for y in cy0..cy1 {
            let mut sum = 0.0f32;
            for x in cx0..cx1 {
                let v = std::mem::take(&mut self.acc[y * stride + x]);
                if x >= w {
                    continue;
                }
                sum += v;
                let cov = sum.abs().min(1.0);
                if cov <= 0.0 {
                    continue;
                }
                let d = &mut self.px[y * w + x];
                if cov >= 1.0 && opaque {
                    *d = full;
                    continue;
                }
                let s = if cov >= 1.0 { full } else { premul(c, cov) };
                let ia = 1.0 - s[3];
                for k in 0..4 {
                    d[k] = s[k] + d[k] * ia;
                }
            }
        }
    }

    /// RGBA8 in sRGB, nicht vormultipliziert, Zeilen von oben nach unten.
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = p[3];
            let un = |v: f32| {
                if a > 0.0 {
                    (v / a * 255.0).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            out.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]);
        }
        out
    }

    /// Vormultiplizierte RGBA8-Werte, wie sie zum Überblenden auf der GPU gebraucht werden.
    pub fn to_premul_rgba8(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.px.len() * 4];
        for (o, p) in out.chunks_exact_mut(4).zip(&self.px) {
            for k in 0..4 {
                o[k] = unit_to_u8(p[k]);
            }
        }
        out
    }

    /// Ausschnitt als vormultipliziertes RGBA8 (Zeilen von oben), auf die
    /// Bildfläche begrenzt. Liefert `(x, y, w, h, Bytes)` des wirklichen Ausschnitts.
    pub fn region_premul_rgba8(
        &self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) -> (usize, usize, usize, usize, Vec<u8>) {
        let (x, y) = (x.min(self.width), y.min(self.height));
        let (w, h) = (w.min(self.width - x), h.min(self.height - y));
        let mut out = vec![0u8; w * h * 4];
        for (row, o) in out.chunks_exact_mut((w * 4).max(1)).enumerate().take(h) {
            let src = &self.px[(y + row) * self.width + x..][..w];
            for (o, p) in o.chunks_exact_mut(4).zip(src) {
                for k in 0..4 {
                    o[k] = unit_to_u8(p[k]);
                }
            }
        }
        (x, y, w, h, out)
    }

    /// Übernimmt einen Ausschnitt aus einem gleich großen Bild.
    pub fn copy_region(&mut self, from: &Canvas, x: usize, y: usize, w: usize, h: usize) {
        if (from.width, from.height) != (self.width, self.height) {
            return;
        }
        let (x, y) = (x.min(self.width), y.min(self.height));
        let (w, h) = (w.min(self.width - x), h.min(self.height - y));
        for row in y..y + h {
            let i = row * self.width + x;
            self.px[i..i + w].copy_from_slice(&from.px[i..i + w]);
        }
    }

    /// Zeichnet `src` mit der linken oberen Ecke bei `(x, y)` darüber
    /// (Quelle über Ziel); was außerhalb liegt, fällt weg.
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32) {
        for sy in 0..src.height {
            let dy = y + sy as i32;
            if dy < 0 || dy >= self.height as i32 {
                continue;
            }
            for sx in 0..src.width {
                let dx = x + sx as i32;
                if dx < 0 || dx >= self.width as i32 {
                    continue;
                }
                let s = src.px[sy * src.width + sx];
                let d = &mut self.px[dy as usize * self.width + dx as usize];
                let ia = 1.0 - s[3];
                for k in 0..4 {
                    d[k] = s[k] + d[k] * ia;
                }
            }
        }
    }

    /// Deckendes Bild, dessen Bildpunkte `f(x, y)` liefert (Farbverläufe,
    /// Farbwähler).
    pub fn from_fn(width: usize, height: usize, f: impl Fn(usize, usize) -> Rgba) -> Canvas {
        let mut c = Canvas::new(width, height);
        for y in 0..height {
            for x in 0..width {
                c.px[y * width + x] = premul(f(x, y), 1.0);
            }
        }
        c
    }

    pub fn to_png(&self) -> Vec<u8> {
        encode_png(self.width as u32, self.height as u32, &self.to_rgba8())
    }
}

/// `(v * 255).round().clamp(0, 255)` ohne Bibliotheksaufruf für `round`
/// (der Grundbefehlssatz von x86-64 kennt kein Runden; je Pixel vier Aufrufe
/// kosteten bei großen Paneelen mehrere Millisekunden). Ergebnis identisch.
#[inline]
fn unit_to_u8(v: f32) -> u8 {
    // NaN bleibt NaN und wird beim Umwandeln zu 0, wie beim Vorbild
    let x = (v * 255.0).clamp(0.0, 255.0);
    let i = x as i32;
    (i + (x - i as f32 >= 0.5) as i32) as u8
}

/// Farbe aus Farbton (Grad), Sättigung und Helligkeit (0..1).
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgba {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Rgba::from_f32([r + m, g + m, b + m, 1.0])
}

/// Farbton (Grad), Sättigung und Helligkeit (0..1) einer Farbe. Grau hat
/// den Farbton 0.
pub fn rgb_to_hsv(c: Rgba) -> (f32, f32, f32) {
    let [r, g, b, _] = c.to_f32();
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

fn premul(c: Rgba, cov: f32) -> [f32; 4] {
    let a = c.3 as f32 / 255.0 * cov;
    [
        c.0 as f32 / 255.0 * a,
        c.1 as f32 / 255.0 * a,
        c.2 as f32 / 255.0 * a,
        a,
    ]
}

/// Trägt die vorzeichenbehaftete Fläche einer Kante in den Akkumulator ein.
/// Die laufende Summe je Zeile ergibt danach die Abdeckung jedes Pixels.
fn accumulate_line(acc: &mut [f32], stride: usize, w: usize, h: usize, p0: Pt, p1: Pt) {
    if p0.y == p1.y {
        return;
    }
    let (dir, p0, p1) = if p0.y < p1.y {
        (1.0f32, p0, p1)
    } else {
        (-1.0f32, p1, p0)
    };
    if p1.y <= 0.0 || p0.y >= h as f32 {
        return;
    }
    let dxdy = (p1.x - p0.x) / (p1.y - p0.y);
    let xmax = w as f32 - 0.0001;
    let mut x = p0.x;
    if p0.y < 0.0 {
        x -= p0.y * dxdy;
    }
    let y_start = p0.y.max(0.0) as usize;
    let y_end = (p1.y.ceil() as usize).min(h);
    for y in y_start..y_end {
        let row = y * stride;
        let dy = ((y + 1) as f32).min(p1.y) - (y as f32).max(p0.y);
        let xnext = x + dxdy * dy;
        let d = dy * dir;
        // Waagerecht auf die Bildfläche begrenzen; die Windungszahl bleibt erhalten.
        let (xa, xb) = (x.clamp(0.0, xmax), xnext.clamp(0.0, xmax));
        let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
        let x0f = x0.floor();
        let x0i = x0f as usize;
        let x1c = x1.ceil();
        let x1i = x1c as usize;
        if x1i <= x0i + 1 {
            let xmf = 0.5 * (xa + xb) - x0f;
            acc[row + x0i] += d - d * xmf;
            acc[row + x0i + 1] += d * xmf;
        } else {
            let s = 1.0 / (x1 - x0);
            let x0r = x0 - x0f;
            let a0 = 0.5 * s * (1.0 - x0r) * (1.0 - x0r);
            let x1r = x1 - x1c + 1.0;
            let am = 0.5 * s * x1r * x1r;
            acc[row + x0i] += d * a0;
            if x1i == x0i + 2 {
                acc[row + x0i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - x0r);
                acc[row + x0i + 1] += d * (a1 - a0);
                for xi in x0i + 2..x1i - 1 {
                    acc[row + xi] += d * s;
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                acc[row + x1i - 1] += d * (1.0 - a2 - am);
            }
            acc[row + x1i] += d * am;
        }
        x = xnext;
    }
}

#[cfg(test)]
mod umrechnung {
    use super::unit_to_u8;

    #[test]
    fn rundet_wie_round() {
        let reference = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        // Alle f32-Werte zwischen -0,1 und 1,1 in feinen Schritten, dazu Grenzfälle
        let mut v = -0.1f32;
        while v < 1.1 {
            assert_eq!(unit_to_u8(v), reference(v), "{v}");
            let b = v.to_bits();
            v = f32::from_bits(if v < 0.0 { b.wrapping_sub(97) } else { b + 97 });
            if v.is_nan() || (v < 0.0 && v > -1e-30) {
                v = 0.0;
            }
        }
        // Um jede Rundungsgrenze k + 0,5 herum
        for k in 0..256 {
            let t = (k as f32 + 0.5) / 255.0;
            for d in -8i32..=8 {
                let v = f32::from_bits((t.to_bits() as i32 + d) as u32);
                assert_eq!(unit_to_u8(v), reference(v), "{v}");
            }
        }
        for v in [
            0.0,
            -0.0,
            1.0,
            0.5 / 255.0,
            1.5 / 255.0,
            f32::NAN,
            f32::INFINITY,
            -1.0,
        ] {
            assert_eq!(unit_to_u8(v), reference(v), "{v}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein Ausschnitt mit verschobenem Ursprung gleicht Pixel für Pixel den
    /// Zeilen des ganzen Bildes (Mengenfenster zeichnet nur geänderte Zeilen).
    #[test]
    fn ausschnitt_gleicht_dem_ganzen_bild() {
        let draw = |c: &mut Canvas| {
            let mut p = Path::new();
            p.rounded_rect(3.5, 7.25, 40.0, 30.0, 6.0);
            c.fill(&p, Rgba(200, 120, 30, 180));
            c.fill_rect(0.0, 20.0, 50.0, 2.5, Rgba(10, 20, 30, 255));
        };
        let mut full = Canvas::new(50, 50);
        full.clear(Rgba(240, 240, 240, 255));
        draw(&mut full);
        let mut part = Canvas::new(50, 12);
        part.clear(Rgba(240, 240, 240, 255));
        part.set_origin(0.0, 15.0);
        draw(&mut part);
        let (a, b) = (full.to_premul_rgba8(), part.to_premul_rgba8());
        assert_eq!(&a[15 * 50 * 4..27 * 50 * 4], &b[..]);
    }

    #[test]
    fn rechteck_deckt_genau_ab() {
        let mut c = Canvas::new(10, 10);
        c.fill_rect(2.0, 2.0, 4.5, 3.0, Rgba::rgb(255, 255, 255));
        let px = c.to_rgba8();
        let a = |x: usize, y: usize| px[(y * 10 + x) * 4 + 3];
        assert_eq!(a(3, 3), 255);
        assert_eq!(a(6, 3), 128); // halb abgedeckt
        assert_eq!(a(7, 3), 0);
        assert_eq!(a(1, 1), 0);
    }

    #[test]
    fn loch_wird_ausgestanzt() {
        let mut p = Path::new();
        p.rounded_rect(0.0, 0.0, 20.0, 20.0, 0.0);
        p.rounded_rect_hole(5.0, 5.0, 10.0, 10.0, 0.0);
        let mut c = Canvas::new(20, 20);
        c.fill(&p, Rgba::rgb(0, 0, 0));
        let px = c.to_rgba8();
        assert_eq!(px[(10 * 20 + 10) * 4 + 3], 0);
        assert_eq!(px[(2 * 20 + 2) * 4 + 3], 255);
    }

    #[test]
    fn flaeche_eines_kreises() {
        let mut p = Path::new();
        p.rounded_rect(10.0, 10.0, 80.0, 80.0, 40.0);
        let mut c = Canvas::new(100, 100);
        c.fill(&p, Rgba::rgb(0, 0, 0));
        let sum: f32 = c.to_rgba8().chunks(4).map(|p| p[3] as f32 / 255.0).sum();
        let soll = std::f32::consts::PI * 40.0 * 40.0;
        assert!((sum - soll).abs() / soll < 0.002, "{sum} vs {soll}");
    }

    #[test]
    fn hsv_hin_und_zurueck() {
        for c in [
            Rgba::rgb(192, 57, 43),
            Rgba::rgb(0, 0, 0),
            Rgba::rgb(255, 255, 255),
            Rgba::rgb(40, 120, 220),
            Rgba::rgb(242, 179, 61),
            Rgba::rgb(10, 200, 30),
        ] {
            let (h, s, v) = rgb_to_hsv(c);
            assert_eq!(hsv_to_rgb(h, s, v), c);
        }
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), Rgba::rgb(255, 0, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), Rgba::rgb(0, 0, 255));
    }

    #[test]
    fn bild_ueber_bild() {
        let mut a = Canvas::new(4, 4);
        let b = Canvas::from_fn(2, 2, |_, _| Rgba::rgb(255, 0, 0));
        a.blit(&b, 3, -1);
        let px = a.to_rgba8();
        assert_eq!(&px[3 * 4..4 * 4], &[255, 0, 0, 255]);
        assert_eq!(&px[(4 + 3) * 4..(4 + 4) * 4], &[0, 0, 0, 0]);
    }
}
