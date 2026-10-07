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
    /// Je Zeile und Block von [`BLOCK`] Pixeln: hat eine Kante dort in den
    /// Akkumulator geschrieben? Unberührte Blöcke haben eine feste Abdeckung.
    marks: Vec<u8>,
    /// Lage der Leinwand im Gesamtbild: gefüllte Pfade werden um diesen
    /// Betrag verschoben (Ausschnitt eines größeren Bildes zeichnen).
    pub(crate) origin: (f32, f32),
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Canvas {
        Canvas {
            width,
            height,
            px: vec![[0.0; 4]; width * height],
            acc: Vec::new(),
            marks: Vec::new(),
            origin: (0.0, 0.0),
        }
    }

    /// Dieselbe Leinwand für ein neues Bild der Größe `width` × `height`,
    /// durchsichtig wie [`Canvas::new`]. Behält den Speicher: ein ganzes
    /// Fensterbild braucht dann keine frischen Seiten vom System (unter
    /// Windows kommt jeder große Block neu von VirtualAlloc).
    /// Wird zur Kopie von `other` und behält dabei den eigenen Speicher
    /// (wie [`Canvas::reuse`]): pixelgleich zu `other.clone()`.
    pub fn copy_from(&mut self, other: &Canvas) {
        self.width = other.width;
        self.height = other.height;
        self.px.clone_from(&other.px);
        self.acc.clone_from(&other.acc);
        self.marks.clone_from(&other.marks);
        self.origin = other.origin;
    }

    /// Übernimmt die Zeilen `y0..y1` von `other` (gleiche Breite), etwa um
    /// Rand und Kopf eines Paneels über gerolltem Inhalt wiederherzustellen.
    pub fn copy_rows(&mut self, other: &Canvas, y0: usize, y1: usize) {
        if other.width != self.width {
            return;
        }
        let (a, b) = (
            y0.min(self.height).min(other.height) * self.width,
            y1.min(self.height).min(other.height) * self.width,
        );
        if a < b {
            self.px[a..b].copy_from_slice(&other.px[a..b]);
        }
    }

    pub fn reuse(&mut self, width: usize, height: usize) {
        if (width, height) != (self.width, self.height) {
            // Akkumulator und Marken passen nur zur alten Größe ([`Canvas::fill`])
            self.acc.clear();
            self.marks.clear();
        }
        self.width = width;
        self.height = height;
        self.px.clear();
        self.px.resize(width * height, [0.0; 4]);
        self.origin = (0.0, 0.0);
    }

    /// Die Leinwand zeigt ab jetzt den Ausschnitt ab `(x, y)` eines größeren
    /// Bildes: Pfade in dessen Koordinaten landen an der richtigen Stelle.
    /// Ganzzahlig gewählt, gleicht der Ausschnitt dem ganzen Bild bis auf
    /// Rundung (höchstens eine Stufe bei Kurven mit Bruchteilkoordinaten).
    pub fn set_origin(&mut self, x: f32, y: f32) {
        self.origin = (x, y);
    }

    /// Lage der Leinwand im Gesamtbild ([`Canvas::set_origin`]).
    pub fn origin(&self) -> (f32, f32) {
        self.origin
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
        let nb = stride.div_ceil(BLOCK);
        // Akkumulator und Blockmarken bleiben zwischen Aufrufen genullt;
        // bearbeitet wird nur der umschließende Bereich des Pfads
        if self.acc.len() != stride * h {
            self.acc.clear();
            self.acc.resize(stride * h, 0.0);
            self.marks.clear();
            self.marks.resize(nb * h, 0);
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
                accumulate_line(
                    &mut self.acc,
                    stride,
                    w,
                    h,
                    a,
                    b,
                    Some((&mut self.marks, nb)),
                );
            }
        }
        // Volle Abdeckung ist der häufigste Fall: Farbe einmal vorausrechnen,
        // deckende Farbe einfach schreiben
        let full = premul(c, 1.0);
        let opaque = full[3] >= 1.0;
        for y in cy0..cy1 {
            let mut sum = 0.0f32;
            let mut x = cx0;
            while x < cx1 {
                let bi = x / BLOCK;
                let end = ((bi + 1) * BLOCK).min(cx1);
                if std::mem::take(&mut self.marks[y * nb + bi]) == 0 {
                    // Keine Kante im Block: Akkumulator ist null, die Abdeckung
                    // fest (Rundungsreste der Laufsumme zählen nicht). Innen in
                    // Rahmen und großen Flächen spart das die Schleife je Pixel.
                    let cov = sum.abs().min(1.0);
                    let row = &mut self.px[y * w + x.min(w)..y * w + end.min(w)];
                    if cov >= 1.0 - FLAT {
                        if opaque {
                            row.fill(full);
                        } else {
                            let ia = 1.0 - full[3];
                            for d in row {
                                for k in 0..4 {
                                    d[k] = full[k] + d[k] * ia;
                                }
                            }
                        }
                    } else if cov > FLAT {
                        let s = premul(c, cov);
                        let ia = 1.0 - s[3];
                        for d in row {
                            for k in 0..4 {
                                d[k] = s[k] + d[k] * ia;
                            }
                        }
                    }
                    x = end;
                    continue;
                }
                for x in x..end {
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
                x = end;
            }
        }
    }

    /// Mischt eine Fläche je Pixel (vorzeichenbehaftet, wie die Laufsumme in
    /// [`Canvas::fill`]) ab `(x0, y0)` in Farbe `c` ein; dieselbe Rechnung
    /// wie dort. Der Bereich liegt ganz in der Leinwand.
    pub(crate) fn blend_area(
        &mut self,
        x0: usize,
        y0: usize,
        w: usize,
        h: usize,
        area: &[f32],
        c: Rgba,
    ) {
        let full = premul(c, 1.0);
        let opaque = full[3] >= 1.0;
        for r in 0..h {
            let row = &mut self.px[(y0 + r) * self.width + x0..][..w];
            for (d, &v) in row.iter_mut().zip(&area[r * w..(r + 1) * w]) {
                let cov = v.abs().min(1.0);
                if cov <= 0.0 {
                    continue;
                }
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
        let mut out = Vec::new();
        self.premul_rgba8_into(&mut out);
        out
    }

    /// Wie [`Canvas::to_premul_rgba8`] in einen vorhandenen Puffer, der seinen
    /// Speicher behält.
    pub fn premul_rgba8_into(&self, out: &mut Vec<u8>) {
        out.clear();
        out.extend(self.px.as_flattened().iter().map(|&v| unit_to_u8(v)));
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
            let src = self.px[(y + row) * self.width + x..][..w].as_flattened();
            for (o, &v) in o.iter_mut().zip(src) {
                *o = unit_to_u8(v);
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

    /// Ersetzt die Bildpunkte ab `(x, y)` (Bildpunkte dieser Leinwand) durch
    /// `src`, ohne Überblenden; was übersteht, fällt weg. Für Teilbilder, die
    /// ein ganzes Bild an einer Stelle erneuern.
    pub fn put(&mut self, src: &Canvas, x: usize, y: usize) {
        if x >= self.width || y >= self.height {
            return;
        }
        let w = src.width.min(self.width - x);
        for row in 0..src.height.min(self.height - y) {
            let d = (y + row) * self.width + x;
            self.px[d..d + w].copy_from_slice(&src.px[row * src.width..][..w]);
        }
    }

    /// Zeichnet `src` mit der linken oberen Ecke bei `(x, y)` darüber
    /// (Quelle über Ziel); was außerhalb liegt, fällt weg. Mit Ursprung
    /// ([`Canvas::set_origin`]) in dessen Koordinaten.
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32) {
        let (x, y) = (x - self.origin.0 as i32, y - self.origin.1 as i32);
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

    /// Zeichnet `src` um `scale` vergrößert (bilinear) und mit Deckkraft
    /// `alpha` darüber, linke obere Ecke bei `(x, y)` (Koordinaten mit
    /// Ursprung wie bei Pfaden). Für fliegende Kopien und Überblendungen.
    pub fn blit_scaled(&mut self, src: &Canvas, x: f32, y: f32, scale: f32, alpha: f32) {
        if scale <= 0.0 || alpha <= 0.0 || src.width == 0 || src.height == 0 {
            return;
        }
        let (x, y) = (x - self.origin.0, y - self.origin.1);
        let (w, h) = (src.width as f32 * scale, src.height as f32 * scale);
        let x0 = x.floor().max(0.0) as usize;
        let y0 = y.floor().max(0.0) as usize;
        let x1 = ((x + w).ceil().max(0.0) as usize).min(self.width);
        let y1 = ((y + h).ceil().max(0.0) as usize).min(self.height);
        let at = |sx: isize, sy: isize| -> [f32; 4] {
            if sx < 0 || sy < 0 || sx >= src.width as isize || sy >= src.height as isize {
                [0.0; 4]
            } else {
                src.px[sy as usize * src.width + sx as usize]
            }
        };
        for dy in y0..y1 {
            let fy = (dy as f32 + 0.5 - y) / scale - 0.5;
            let (iy, ty) = (fy.floor() as isize, fy - fy.floor());
            for dx in x0..x1 {
                let fx = (dx as f32 + 0.5 - x) / scale - 0.5;
                let (ix, tx) = (fx.floor() as isize, fx - fx.floor());
                let (a, b) = (at(ix, iy), at(ix + 1, iy));
                let (c, d) = (at(ix, iy + 1), at(ix + 1, iy + 1));
                let mut p = [0.0f32; 4];
                for k in 0..4 {
                    let top = a[k] + (b[k] - a[k]) * tx;
                    let bot = c[k] + (d[k] - c[k]) * tx;
                    p[k] = (top + (bot - top) * ty) * alpha;
                }
                let q = &mut self.px[dy * self.width + dx];
                let ia = 1.0 - p[3];
                for k in 0..4 {
                    q[k] = p[k] + q[k] * ia;
                }
            }
        }
    }

    /// Füllt die ganzen Bildpunkte im Bereich `x0..x1`, `y0..y1` (Koordinaten
    /// des Gesamtbildes wie bei Pfaden) deckend mit `f(x, y)`; `f` bekommt die
    /// Mitte des Bildpunkts. Für Schraffuren in Vorschaubildern.
    pub fn shade_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, f: impl Fn(f32, f32) -> Rgba) {
        let (ox, oy) = self.origin;
        let col = |v: f32, o: f32, n: usize| ((v - o).round().max(0.0) as usize).min(n);
        let (cx0, cx1) = (col(x0, ox, self.width), col(x1, ox, self.width));
        let (cy0, cy1) = (col(y0, oy, self.height), col(y1, oy, self.height));
        for y in cy0..cy1 {
            for x in cx0..cx1 {
                let c = f(x as f32 + ox + 0.5, y as f32 + oy + 0.5);
                self.px[y * self.width + x] = premul(c, 1.0);
            }
        }
    }

    /// Übernimmt die ganzen Bildpunkte im Bereich `x0..x1`, `y0..y1`
    /// (Koordinaten des Gesamtbildes, gerundet wie [`Canvas::shade_rect`])
    /// deckend aus `src`, das mit eigenem Ursprung im selben Gesamtbild liegt.
    /// Was `src` nicht zeigt, bleibt unverändert.
    pub fn copy_rect_from(&mut self, src: &Canvas, x0: f32, y0: f32, x1: f32, y1: f32) {
        let (ox, oy) = self.origin;
        let col = |v: f32, o: f32, n: usize| ((v - o).round().max(0.0) as usize).min(n);
        let (cx0, cx1) = (col(x0, ox, self.width), col(x1, ox, self.width));
        let (cy0, cy1) = (col(y0, oy, self.height), col(y1, oy, self.height));
        let dx = (ox - src.origin.0).round() as isize;
        let dy = (oy - src.origin.1).round() as isize;
        let sx0 = (cx0 as isize + dx).max(0);
        let sx1 = (cx1 as isize + dx).min(src.width as isize);
        if sx1 <= sx0 {
            return;
        }
        let (sx0, sx1) = (sx0 as usize, sx1 as usize);
        let d0 = (sx0 as isize - dx) as usize;
        for y in cy0..cy1 {
            let sy = y as isize + dy;
            if sy < 0 || sy >= src.height as isize {
                continue;
            }
            let from = &src.px[sy as usize * src.width + sx0..sy as usize * src.width + sx1];
            let at = y * self.width + d0;
            self.px[at..at + from.len()].copy_from_slice(from);
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
/// und ohne die sättigende Umwandlung `as i32`: Beides verhindert, dass der
/// Übersetzer die Schleife über ein Bild bündelt (SIMD). Ergebnis identisch.
#[inline]
fn unit_to_u8(v: f32) -> u8 {
    let x = v * 255.0;
    // Begrenzen; NaN wird zu 0 wie beim Vorbild
    let x = if x > 0.0 { x } else { 0.0 };
    let x = if x < 255.0 { x } else { 255.0 };
    // + 2^23 rundet auf die nächste ganze Zahl n (bei ,5 auf die gerade), die
    // dann in den unteren Bits steht; ,5 wird wie bei `round` aufgerundet
    const SHIFT: f32 = 8_388_608.0;
    let t = x + SHIFT;
    let n = t - SHIFT;
    (t.to_bits() as u8).wrapping_add((x - n >= 0.5) as u8)
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

/// Breite der Blöcke, deren Abdeckung [`Canvas::fill`] am Stück setzt.
const BLOCK: usize = 16;
/// Abweichung der Laufsumme von 0 bzw. 1, die als Rundungsrest gilt.
const FLAT: f32 = 1e-3;

/// Trägt die vorzeichenbehaftete Fläche einer Kante in den Akkumulator ein.
/// Die laufende Summe je Zeile ergibt danach die Abdeckung jedes Pixels.
/// `marks` (Blockmarken, Blöcke je Zeile): jeder beschriebene Block wird markiert.
pub(crate) fn accumulate_line(
    acc: &mut [f32],
    stride: usize,
    w: usize,
    h: usize,
    p0: Pt,
    p1: Pt,
    mut marks: Option<(&mut [u8], usize)>,
) {
    if p0.y == p1.y {
        return;
    }
    // An den Seitenrändern teilen: links davon zählt das Stück als senkrecht
    // am Rand (volle Windung), rechts davon ist es unsichtbar. Ein Stück, das
    // nur gestaucht würde, gäbe am Rand eine falsche Abdeckung (Teilbilder).
    for edge in [0.0, w as f32] {
        if (p0.x - edge) * (p1.x - edge) < 0.0 {
            let t = (edge - p0.x) / (p1.x - p0.x);
            let m = Pt {
                x: edge,
                y: p0.y + (p1.y - p0.y) * t,
            };
            let mut again = |a: Pt, b: Pt, marks: &mut Option<(&mut [u8], usize)>| {
                let mk = marks.as_mut().map(|(m, nb)| (&mut **m, *nb));
                accumulate_line(acc, stride, w, h, a, b, mk);
            };
            again(p0, m, &mut marks);
            again(m, p1, &mut marks);
            return;
        }
    }
    if p0.x.min(p1.x) >= w as f32 {
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
        if let Some((m, nb)) = marks.as_mut() {
            m[y * *nb + x0i / BLOCK..=y * *nb + (x1i.max(x0i + 1)) / BLOCK].fill(1);
        }
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

    /// Eine wiederverwendete Leinwand malt bytegleich wie eine neue, auch
    /// nach anderer Größe und mit Ursprung (Akkumulator und Marken).
    #[test]
    fn wiederverwendet_wie_neu() {
        let draw = |c: &mut Canvas, o: f32| {
            c.set_origin(o, o);
            let mut p = Path::new();
            p.rounded_rect(o + 3.3, o + 2.7, 41.5, 23.2, 6.0);
            c.fill(&p, Rgba(200, 40, 90, 255));
            c.fill_rect(o + 10.5, o + 5.25, 20.0, 9.0, Rgba(20, 140, 190, 128));
        };
        let mut c = Canvas::new(0, 0);
        for (w, h, o) in [(64, 40, 0.0), (33, 70, 5.0), (64, 40, 1.5), (64, 40, 0.0)] {
            let mut fresh = Canvas::new(w, h);
            draw(&mut fresh, o);
            c.reuse(w, h);
            draw(&mut c, o);
            assert_eq!(c.to_premul_rgba8(), fresh.to_premul_rgba8(), "{w}x{h}");
            let mut buf = vec![7u8; 3];
            c.premul_rgba8_into(&mut buf);
            assert_eq!(buf, fresh.to_premul_rgba8());
        }
    }

    /// `copy_from` ist eine Kopie wie `clone`, auch für das, was danach
    /// darauf gemalt wird, und aus jeder Größe heraus.
    #[test]
    fn kopie_in_vorhandene_leinwand() {
        let mut p = Path::new();
        p.rounded_rect(3.3, 2.7, 41.5, 23.2, 6.0);
        let mut q = Path::new();
        q.rounded_rect(10.1, 8.6, 30.0, 20.0, 4.0);
        let mut src = Canvas::new(64, 40);
        src.fill(&p, Rgba(200, 40, 90, 255));
        for mut c in [Canvas::new(0, 0), Canvas::new(64, 40), Canvas::new(90, 12)] {
            c.copy_from(&src);
            let mut want = src.clone();
            c.fill(&q, Rgba(20, 140, 190, 128));
            want.fill(&q, Rgba(20, 140, 190, 128));
            assert_eq!(c.to_premul_rgba8(), want.to_premul_rgba8());
        }
    }

    /// Ein Ausschnitt (Teilbild) zeigt einen Pfad wie das ganze Bild, auch
    /// wo der Pfad über den linken oder rechten Rand des Ausschnitts läuft.
    #[test]
    fn pfad_am_rand_des_ausschnitts() {
        let col = Rgba::from_f32([0.2, 0.8, 0.4, 1.0]);
        let mut p = Path::new();
        p.rounded_rect(40.3, 10.2, 40.0, 30.0, 6.0);
        p.rounded_rect_hole(42.3, 12.2, 36.0, 26.0, 4.0);
        let mut big = Canvas::new(100, 60);
        big.fill(&p, col);
        for (ox, w) in [(0usize, 44usize), (20, 24), (20, 22), (30, 13), (41, 10)] {
            let mut sub = Canvas::new(w, 50);
            sub.set_origin(ox as f32, 5.0);
            sub.fill(&p, col);
            for y in 0..50 {
                for x in 0..w {
                    let a = big.px[(y + 5) * 100 + x + ox];
                    let b = sub.px[y * w + x];
                    assert!(
                        (0..4).all(|k| (a[k] - b[k]).abs() < 1e-4),
                        "ox {ox} w {w}: {x},{y}: {a:?} {b:?}"
                    );
                }
            }
        }
    }

    /// Unverkleinert und deckend wie `blit`; halb so groß bleibt eine
    /// einfarbige Fläche einfarbig; halbe Deckkraft halbiert.
    #[test]
    fn verkleinert_mit_deckkraft() {
        let red = Rgba::from_f32([1.0, 0.0, 0.0, 1.0]);
        let mut src = Canvas::new(20, 10);
        src.fill_rect(0.0, 0.0, 20.0, 10.0, red);
        src.fill_rect(5.0, 2.0, 4.0, 4.0, Rgba::from_f32([0.0, 0.0, 1.0, 1.0]));
        let (mut a, mut b) = (Canvas::new(40, 30), Canvas::new(40, 30));
        a.blit(&src, 7, 5);
        b.blit_scaled(&src, 7.0, 5.0, 1.0, 1.0);
        assert!(a
            .px
            .iter()
            .zip(&b.px)
            .all(|(p, q)| (0..4).all(|k| (p[k] - q[k]).abs() < 1e-5)));
        let mut uni = Canvas::new(20, 10);
        uni.fill_rect(0.0, 0.0, 20.0, 10.0, red);
        let mut c = Canvas::new(40, 30);
        c.blit_scaled(&uni, 4.0, 4.0, 0.5, 0.5);
        let p = c.px[6 * 40 + 8];
        assert!(
            (p[0] - 0.5).abs() < 1e-5 && (p[3] - 0.5).abs() < 1e-5,
            "{p:?}"
        );
        assert_eq!(c.px[2 * 40 + 2], [0.0; 4]);
        assert_eq!(
            c.px[6 * 40 + 20],
            [0.0; 4],
            "rechts der 10 px breiten Kopie leer"
        );
    }

    /// Füllen wie vor den Blockmarken: jede Zeile Pixel für Pixel.
    fn fill_per_pixel(c: &mut Canvas, path: &Path, col: Rgba) {
        let (w, h) = (c.width, c.height);
        let stride = w + 2;
        let mut acc = vec![0.0f32; stride * h];
        for poly in &path.flatten(0.2) {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                accumulate_line(&mut acc, stride, w, h, a, b, None);
            }
        }
        for y in 0..h {
            let mut sum = 0.0f32;
            for x in 0..w {
                sum += acc[y * stride + x];
                let cov = sum.abs().min(1.0);
                if cov <= 0.0 {
                    continue;
                }
                let s = premul(col, cov);
                let d = &mut c.px[y * w + x];
                let ia = 1.0 - s[3];
                for k in 0..4 {
                    d[k] = s[k] + d[k] * ia;
                }
            }
        }
    }

    /// Ganze Blöcke ohne Kante (innen in Rahmen und Flächen) ergeben dasselbe
    /// Bild wie die Schleife je Pixel: Fensterrahmen mit Schatten, Kreis,
    /// halbdurchsichtige Fläche, teils außerhalb der Leinwand.
    #[test]
    fn bloecke_gleichen_der_pixelschleife() {
        let shadow = Rgba(0, 0, 0, 20);
        let mut paths = Vec::new();
        for i in 1..=4 {
            let d = i as f32 * 3.0;
            let mut p = Path::new();
            p.rounded_rect(
                20.3 - d * 0.5,
                15.7 - d * 0.25,
                300.0 + d,
                200.0 + d,
                9.0 + d,
            );
            p.rounded_rect_hole(22.3, 17.7, 296.0, 196.0, 7.0);
            paths.push((p, shadow));
        }
        let mut p = Path::new();
        p.rounded_rect(20.3, 15.7, 300.0, 200.0, 9.0);
        paths.push((p, Rgba(230, 230, 235, 255)));
        let mut p = Path::new();
        p.rounded_rect(88.6, 58.6, 122.8, 122.8, 61.4);
        paths.push((p, Rgba(50, 100, 200, 153)));
        let mut p = Path::new();
        p.rounded_rect(-40.0, 180.0, 500.0, 90.0, 12.0);
        paths.push((p, Rgba(200, 80, 25, 255)));
        let (mut a, mut b) = (Canvas::new(347, 251), Canvas::new(347, 251));
        for (p, col) in &paths {
            a.fill(p, *col);
            fill_per_pixel(&mut b, p, *col);
        }
        let (a, b) = (a.to_premul_rgba8(), b.to_premul_rgba8());
        let diff = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        assert!(diff <= 1, "größte Abweichung {diff}");
    }

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
