//! PDF lesen, so weit der Lageplan es braucht (plan-gelaendemodul.md §5
//! G1c, Technik): Objekte, Querverweise als Tabelle und als Strom,
//! Objektströme, Nachträge über `/Prev`, Suche nach `n 0 obj`, wenn die
//! Querverweise kaputt sind, Stromfilter, der Standard-Schutz ohne
//! Öffnungskennwort und der Seitenbaum mit geerbten Ressourcen.
//!
//! Skizzeo ändert die Datei nie; es liest Bytes und gibt Objekte zurück.
//! Was nicht passt, wird ein Satz und nie ein Abbruch.

pub mod filter;
pub mod objekt;
pub mod schutz;

use objekt::{Leser, Objekt, Strom, Verz, Wort};
use schutz::Schutz;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

/// So viele Bytes darf eine Datei insgesamt kosten: entpackte und kopierte
/// Ströme, je Filterstufe, und gelesene Objekte, auch wiederholt (Schutz vor
/// Bomben, Review W37). Ein Lageplan braucht wenige MB.
pub const BUDGET: usize = 512 << 20;

/// Wo ein Objekt steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ort {
    /// Lage in der Datei und Generation.
    Datei(usize, u16),
    /// In einem Objektstrom (Nummer des Stroms, Stelle darin).
    InStrom(u32, u32),
    Frei,
}

/// Eine geöffnete PDF-Datei.
pub struct Pdf {
    d: Vec<u8>,
    orte: BTreeMap<u32, Ort>,
    /// Objekte aus Objektströmen, beim Öffnen gelesen, mit ihrer Länge
    /// (jede Kopie kostet sie).
    aus_stroemen: BTreeMap<u32, (Objekt, usize)>,
    /// Wo „endstream“ steht, aufsteigend (einmal gesucht).
    endstreams: Vec<usize>,
    /// Was vom [`BUDGET`] übrig ist; `voll`, wenn eine Datei mehr wollte.
    rest: Cell<usize>,
    voll: Cell<bool>,
    pub trailer: Verz,
    schutz: Option<Schutz>,
    /// Nummer des `/Encrypt`-Verzeichnisses (wird nicht entschlüsselt).
    encrypt_nr: Option<u32>,
    /// Was beim Öffnen auffiel (Querverweise neu aufgebaut …), als Sätze.
    pub hinweise: Vec<String>,
}

/// Eine Seite aus dem Seitenbaum, mit geerbten Angaben.
#[derive(Clone, Debug, PartialEq)]
pub struct Blatt {
    pub verz: Verz,
    pub ressourcen: Verz,
    /// x0, y0, x1, y1 in pt.
    pub mediabox: [f64; 4],
    /// Drehung in Grad (0, 90, 180, 270).
    pub drehung: i64,
}

/// So oft folgt Skizzeo Bezügen auf Bezüge.
const KETTE: usize = 32;

/// Wo `muster` in `d` beginnt, aufsteigend.
fn suchen(d: &[u8], muster: &[u8]) -> Vec<usize> {
    let mut aus = Vec::new();
    let mut i = 0;
    while let Some(k) = ab(d, i).windows(muster.len()).position(|w| w == muster) {
        aus.push(i + k);
        i += k + 1;
    }
    aus
}

/// Rest des Puffers ab `p`, leer jenseits des Endes.
fn ab(d: &[u8], p: usize) -> &[u8] {
    d.get(p..).unwrap_or(&[])
}

/// Ganze Zahl ohne Vorzeichen ab `p`; Ende der Ziffern.
fn ziffern(d: &[u8], p: usize) -> Option<(u64, usize)> {
    let mut e = p;
    while d.get(e).is_some_and(u8::is_ascii_digit) && e - p < 19 {
        e += 1;
    }
    let v = std::str::from_utf8(d.get(p..e)?).ok()?.parse().ok()?;
    (e > p).then_some((v, e))
}

/// Steht an `p` „nr gen obj“? Gibt nr, gen und die Lage hinter `obj`.
fn kopf_an(d: &[u8], p: usize) -> Option<(u32, u16, usize)> {
    let leer = |q: usize| {
        let mut e = q;
        while d.get(e).is_some_and(|&b| objekt::leer(b)) {
            e += 1;
        }
        (e > q).then_some(e)
    };
    let (nr, q) = ziffern(d, p)?;
    let (gen, q) = ziffern(d, leer(q)?)?;
    let q = leer(q)?;
    ab(d, q).starts_with(b"obj").then_some((
        u32::try_from(nr).ok()?,
        u16::try_from(gen).ok()?,
        q + 3,
    ))
}

impl Pdf {
    /// Datei öffnen. Fehler sind Sätze für den Dialog, z. B. „Die Datei ist
    /// mit Kennwort geschützt.“
    pub fn lesen(roh: &[u8]) -> Result<Pdf, String> {
        Pdf::lesen_mit(roh, BUDGET)
    }

    /// Wie [`Pdf::lesen`] mit eigenem Budget (Tests mit kleinen Bomben).
    pub fn lesen_mit(roh: &[u8], budget: usize) -> Result<Pdf, String> {
        let kopf = roh
            .windows(5)
            .take(1024)
            .position(|w| w == b"%PDF-")
            .ok_or("Das ist keine PDF-Datei.")?;
        let mut pdf = Pdf {
            d: roh[kopf..].to_vec(),
            orte: BTreeMap::new(),
            aus_stroemen: BTreeMap::new(),
            endstreams: Vec::new(),
            rest: Cell::new(budget),
            voll: Cell::new(false),
            trailer: Verz::default(),
            schutz: None,
            encrypt_nr: None,
            hinweise: Vec::new(),
        };
        pdf.endstreams = suchen(&pdf.d, b"endstream");
        let gut = pdf.querverweise().is_some()
            && pdf.trailer.get("Root").is_some()
            && pdf.orte.iter().all(|(&nr, o)| match *o {
                Ort::Datei(p, _) => kopf_an(&pdf.d, p).is_some_and(|k| k.0 == nr),
                _ => true,
            });
        if !gut {
            pdf.neu_aufbauen();
            pdf.hinweise.push(
                "Die Querverweise der Datei sind beschädigt; Skizzeo hat die Objekte selbst gesucht."
                    .into(),
            );
        }
        if let Some(e) = pdf.trailer.get("Encrypt").cloned() {
            pdf.encrypt_nr = match e {
                Objekt::Bezug(n, _) => Some(n),
                _ => None,
            };
            let v = pdf.aufloesen(&e);
            let v = v
                .verz()
                .ok_or("Die Verschlüsselung der Datei ist unlesbar.")?;
            let id0 = match pdf.trailer.get("ID").map(|o| pdf.aufloesen(o)) {
                Some(Objekt::Feld(f)) => f.first().and_then(Objekt::text).unwrap_or(&[]).to_vec(),
                _ => Vec::new(),
            };
            pdf.schutz = Some(Schutz::neu(v, &id0)?);
        }
        pdf.objektstroeme(!gut);
        if pdf.katalog().is_none() {
            return Err("In der Datei steht kein lesbares Dokument (Katalog fehlt).".into());
        }
        Ok(pdf)
    }

    /// Alle Querverweise ab `startxref` lesen, `/Prev` folgen.
    fn querverweise(&mut self) -> Option<()> {
        let ende = &self.d[self.d.len().saturating_sub(2048)..];
        let s = ende.windows(9).rposition(|w| w == b"startxref")?;
        let mut l = Leser::neu(ende, s + 9);
        let mut pos = l.objekt()?.ganz()?;
        let mut gesehen = BTreeSet::new();
        let mut erster = true;
        while pos >= 0 && gesehen.insert(pos) {
            let trailer = self.abschnitt(pos as usize)?;
            if erster {
                self.trailer = trailer.clone();
                erster = false;
            }
            match trailer.ganz("Prev") {
                Some(p) => pos = p,
                None => break,
            }
        }
        Some(())
    }

    /// Ein Abschnitt (Tabelle oder Strom) an `pos`; gibt seinen Trailer.
    fn abschnitt(&mut self, pos: usize) -> Option<Verz> {
        let mut l = Leser::neu(&self.d, pos);
        l.leerraum();
        if ab(&self.d, l.pos).starts_with(b"xref") {
            l.pos += 4;
            let mut neu = Vec::new();
            loop {
                match l.wort()? {
                    Wort::Befehl(b"trailer") => break,
                    Wort::Objekt(Objekt::Ganz(start)) => {
                        let n = l.objekt()?.ganz()?;
                        for i in 0..n.max(0) {
                            let ofs = l.objekt()?.ganz()?;
                            let gen = l.objekt()?.ganz()?;
                            let art = match l.wort()? {
                                Wort::Befehl(a) => a,
                                _ => return None,
                            };
                            let nr = u32::try_from(start.saturating_add(i)).ok()?;
                            let ort = if art == b"n" && ofs > 0 {
                                Ort::Datei(ofs as usize, gen.clamp(0, 65535) as u16)
                            } else {
                                Ort::Frei
                            };
                            neu.push((nr, ort));
                        }
                    }
                    _ => return None,
                }
            }
            let trailer = l.objekt()?.verz()?.clone();
            // Mischform: Querverweisstrom neben der Tabelle zuerst
            if let Some(x) = trailer.ganz("XRefStm") {
                if x >= 0 {
                    let _ = self.xref_strom(x as usize);
                }
            }
            for (nr, ort) in neu {
                self.orte.entry(nr).or_insert(ort);
            }
            Some(trailer)
        } else {
            self.xref_strom(pos)
        }
    }

    /// Querverweisstrom an `pos` (ISO 32000-1 §7.5.8).
    fn xref_strom(&mut self, pos: usize) -> Option<Verz> {
        let (_, _, o) = self.lesen_an(pos, None)?;
        let s = o.strom()?;
        if s.verz.name("Type") != Some("XRef") {
            return None;
        }
        let daten = self.daten(s)?;
        let w: Vec<usize> = s
            .verz
            .get("W")?
            .feld()?
            .iter()
            .map(|o| o.ganz().unwrap_or(0).clamp(0, 8) as usize)
            .collect();
        if w.len() < 3 {
            return None;
        }
        let groesse = s.verz.ganz("Size").unwrap_or(0);
        let index: Vec<i64> = match s.verz.get("Index").and_then(Objekt::feld) {
            Some(f) => f.iter().filter_map(Objekt::ganz).collect(),
            None => vec![0, groesse],
        };
        let breite = w[0] + w[1] + w[2];
        let mut zeilen = daten.chunks_exact(breite.max(1));
        let feld =
            |z: &[u8], a: usize, n: usize| z[a..a + n].iter().fold(0u64, |v, &b| v << 8 | b as u64);
        for paar in index.chunks_exact(2) {
            for i in 0..paar[1].max(0) {
                let Some(z) = zeilen.next() else { break };
                let art = if w[0] == 0 { 1 } else { feld(z, 0, w[0]) };
                let (f2, f3) = (feld(z, w[0], w[1]), feld(z, w[0] + w[1], w[2]));
                let Ok(nr) = u32::try_from(paar[0].saturating_add(i)) else {
                    continue;
                };
                let ort = match art {
                    1 if f2 > 0 => Ort::Datei(f2 as usize, f3.min(65535) as u16),
                    2 => Ort::InStrom(
                        f2.min(u32::MAX as u64) as u32,
                        f3.min(u32::MAX as u64) as u32,
                    ),
                    _ => Ort::Frei,
                };
                self.orte.entry(nr).or_insert(ort);
            }
        }
        Some(s.verz.clone())
    }

    /// Querverweise durch Suche nach „n g obj“ neu aufbauen; der Trailer
    /// wird aus allen Trailern und Querverweisströmen gemischt.
    fn neu_aufbauen(&mut self) {
        let d = &self.d;
        let mut orte = BTreeMap::new();
        let mut i = 0;
        while let Some(k) = d[i..].windows(3).position(|w| w == b"obj") {
            let o = i + k;
            i = o + 3;
            // rückwärts: Leerraum, gen, Leerraum, nr, davor Leerraum oder Anfang
            let mut p = o;
            let zurueck = |p: &mut usize, f: &dyn Fn(u8) -> bool| {
                let a = *p;
                while *p > 0 && f(d[*p - 1]) {
                    *p -= 1;
                }
                *p < a
            };
            if !(zurueck(&mut p, &|b| objekt::leer(b))
                && zurueck(&mut p, &|b| b.is_ascii_digit())
                && zurueck(&mut p, &|b| objekt::leer(b))
                && zurueck(&mut p, &|b| b.is_ascii_digit()))
            {
                continue;
            }
            if p > 0 && !objekt::leer(d[p - 1]) {
                continue;
            }
            if let Some((nr, gen, _)) = kopf_an(d, p) {
                orte.insert(nr, Ort::Datei(p, gen));
            }
        }
        // Objektströme behalten, was die alten Querverweise über sie wussten
        let alt: Vec<(u32, Ort)> = self
            .orte
            .iter()
            .filter(|(_, o)| matches!(o, Ort::InStrom(..)))
            .map(|(&n, &o)| (n, o))
            .collect();
        self.orte = orte;
        for (n, o) in alt {
            self.orte.entry(n).or_insert(o);
        }
        let mut trailer = Verz::default();
        let mut p = 0;
        while let Some(k) = self.d[p..].windows(7).position(|w| w == b"trailer") {
            p += k + 7;
            if let Some(Objekt::Verz(v)) = Leser::neu(&self.d, p).objekt() {
                for (k, o) in v.0 {
                    trailer.setzen(&k, o);
                }
            }
        }
        let nummern: Vec<u32> = self.orte.keys().copied().collect();
        for nr in &nummern {
            if let Some(Ort::Datei(p, _)) = self.orte.get(nr).copied() {
                if let Some((_, _, Objekt::Strom(s))) = self.lesen_an(p, None) {
                    if s.verz.name("Type") == Some("XRef") {
                        for k in ["Root", "Info", "ID", "Encrypt"] {
                            if let Some(o) = s.verz.get(k) {
                                trailer.setzen(k, o.clone());
                            }
                        }
                    }
                }
            }
        }
        if trailer.get("Root").is_none() {
            for nr in nummern {
                if let Some(o) = self.objekt(nr) {
                    if o.verz().and_then(|v| v.name("Type")) == Some("Catalog") {
                        trailer.setzen("Root", Objekt::Bezug(nr, 0));
                    }
                }
            }
        }
        self.trailer = trailer;
    }

    /// Alle Objektströme lesen, auf die die Querverweise zeigen, und nach
    /// einer Suche (`gesucht`) auch die, die nur sie fand.
    fn objektstroeme(&mut self, gesucht: bool) {
        let mut stroeme: BTreeSet<u32> = self
            .orte
            .values()
            .filter_map(|o| match o {
                Ort::InStrom(s, _) => Some(*s),
                _ => None,
            })
            .collect();
        if gesucht {
            for (&nr, o) in &self.orte {
                if let Ort::Datei(p, _) = *o {
                    let kopf = ab(&self.d, p);
                    if kopf[..kopf.len().min(200)]
                        .windows(7)
                        .any(|w| w == b"/ObjStm")
                    {
                        stroeme.insert(nr);
                    }
                }
            }
        }
        for s in stroeme {
            let Some(Objekt::Strom(st)) = self.objekt(s) else {
                continue;
            };
            let Some(d) = self.daten(&st) else { continue };
            let n = st.verz.ganz("N").unwrap_or(0).clamp(0, 1 << 20) as usize;
            let erst = st.verz.ganz("First").unwrap_or(0).max(0) as usize;
            let mut l = Leser::neu(&d, 0);
            let mut paare = Vec::new();
            for _ in 0..n {
                match (
                    l.objekt().and_then(|o| o.ganz()),
                    l.objekt().and_then(|o| o.ganz()),
                ) {
                    (Some(nr), Some(ofs)) => paare.push((nr, ofs)),
                    _ => break,
                }
            }
            // Objekte liegen nacheinander; eines, das in ein früheres reicht,
            // gilt nicht (sonst liest eine Bombe dasselbe Feld je Nummer neu,
            // Review W37 B3)
            let mut paare: Vec<(usize, (i64, i64))> = paare.into_iter().enumerate().collect();
            paare.sort_by_key(|(_, (_, ofs))| *ofs);
            let mut ende = 0;
            for (i, (nr, ofs)) in paare {
                let Ok(nr) = u32::try_from(nr) else { continue };
                // nur, wenn die Querverweise das Objekt hier sehen (sonst ist es überholt)
                let gilt = match self.orte.get(&nr) {
                    Some(Ort::InStrom(st_nr, _)) => *st_nr == s,
                    None => true,
                    _ => false,
                };
                if !gilt || ofs < 0 {
                    continue;
                }
                let start = erst.saturating_add(ofs as usize);
                if start < ende {
                    continue;
                }
                let mut l = Leser::neu(&d, start);
                if let Some(o) = l.objekt() {
                    ende = l.pos;
                    let n = l.pos - start;
                    if self.kosten(n).is_none() {
                        break;
                    }
                    self.aus_stroemen.insert(nr, (o, n));
                    self.orte.entry(nr).or_insert(Ort::InStrom(s, i as u32));
                }
            }
        }
    }

    /// Objekt „nr gen obj … endobj“ an `pos` lesen, entschlüsselt, wenn
    /// `schutz` gegeben ist.
    fn lesen_an(&self, pos: usize, schutz: Option<&Schutz>) -> Option<(u32, u16, Objekt)> {
        let (nr, gen, nach) = kopf_an(&self.d, pos)?;
        let mut l = Leser::neu(&self.d, nach);
        let mut o = l.objekt()?;
        self.kosten(l.pos - nach)?;
        if let Objekt::Verz(v) = &o {
            l.leerraum();
            if ab(&self.d, l.pos).starts_with(b"stream") {
                let mut a = l.pos + 6;
                if self.d.get(a) == Some(&b'\r') {
                    a += 1;
                }
                if self.d.get(a) == Some(&b'\n') {
                    a += 1;
                }
                let laenge = match v.get("Length") {
                    Some(Objekt::Bezug(n, _)) => self.zahl_objekt(*n),
                    Some(o) => o.ganz(),
                    None => None,
                };
                let passt = |n: i64| {
                    let e = a.checked_add(usize::try_from(n).ok()?)?;
                    let mut q = Leser::neu(&self.d, e);
                    q.leerraum();
                    ab(&self.d, q.pos).starts_with(b"endstream").then_some(e)
                };
                let e = laenge.and_then(passt).or_else(|| {
                    // das nächste „endstream“ aus der Liste, ohne neu zu suchen
                    // (Review W37 B6)
                    let i = self.endstreams.partition_point(|&p| p < a);
                    let mut e = *self.endstreams.get(i)?;
                    if e > a && self.d[e - 1] == b'\n' {
                        e -= 1;
                    }
                    if e > a && self.d[e - 1] == b'\r' {
                        e -= 1;
                    }
                    Some(e)
                })?;
                self.kosten(e - a)?;
                o = Objekt::Strom(Strom {
                    verz: v.clone(),
                    daten: self.d[a..e].to_vec(),
                });
            }
        }
        if let Some(s) = schutz {
            s.texte_in(nr, gen, &mut o);
            if let Objekt::Strom(st) = &mut o {
                let roh = st.verz.name("Type") == Some("XRef")
                    || (!s.metadaten && st.verz.name("Type") == Some("Metadata"))
                    || matches!(st.verz.get("Filter"), Some(Objekt::Name(n)) if n == "Crypt")
                    || matches!(st.verz.get("Filter"), Some(Objekt::Feld(f)) if f.first().and_then(Objekt::name) == Some("Crypt"));
                if !roh {
                    st.daten = s.strom(nr, gen, &st.daten);
                }
            }
        }
        Some((nr, gen, o))
    }

    /// Ganze Zahl als eigenes Objekt (die Länge eines Stroms), ohne Ströme
    /// zu lesen; so führt keine Länge im Kreis.
    fn zahl_objekt(&self, nr: u32) -> Option<i64> {
        match *self.orte.get(&nr)? {
            Ort::Datei(p, _) => {
                let (_, _, nach) = kopf_an(&self.d, p)?;
                Leser::neu(&self.d, nach).objekt()?.ganz()
            }
            Ort::InStrom(..) => self.aus_stroemen.get(&nr)?.0.ganz(),
            Ort::Frei => None,
        }
    }

    /// Objekt mit der Nummer `nr`, entschlüsselt.
    pub fn objekt(&self, nr: u32) -> Option<Objekt> {
        match *self.orte.get(&nr)? {
            Ort::Datei(p, _) => {
                let schutz = if Some(nr) == self.encrypt_nr {
                    None
                } else {
                    self.schutz.as_ref()
                };
                let (n, _, o) = self.lesen_an(p, schutz)?;
                (n == nr).then_some(o)
            }
            Ort::InStrom(..) => {
                let (o, n) = self.aus_stroemen.get(&nr)?;
                self.kosten(*n)?;
                Some(o.clone())
            }
            Ort::Frei => None,
        }
    }

    /// Bezüge auflösen (auch Ketten); anderes unverändert.
    pub fn aufloesen(&self, o: &Objekt) -> Objekt {
        let mut o = o.clone();
        for _ in 0..KETTE {
            match o {
                Objekt::Bezug(n, _) => o = self.objekt(n).unwrap_or(Objekt::Null),
                _ => return o,
            }
        }
        Objekt::Null
    }

    /// Verzeichnis zu einem Schlüssel, Bezüge aufgelöst.
    pub fn verz(&self, v: &Verz, k: &str) -> Option<Verz> {
        self.aufloesen(v.get(k)?).verz().cloned()
    }

    /// Daten eines Stroms durch seine Filter.
    pub fn daten(&self, s: &Strom) -> Option<Vec<u8>> {
        let f = s.verz.get("Filter").map(|o| self.aufloesen(o));
        let p = s
            .verz
            .get("DecodeParms")
            .or_else(|| s.verz.get("DP"))
            .map(|o| match self.aufloesen(o) {
                Objekt::Feld(f) => Objekt::Feld(f.iter().map(|x| self.aufloesen(x)).collect()),
                x => x,
            });
        self.kosten(s.daten.len())?;
        let mut d = s.daten.clone();
        for (name, parm) in filter::liste(f.as_ref(), p.as_ref()) {
            match filter::filter(&name, &d, parm.as_ref(), self.rest.get()) {
                Ok(x) => {
                    self.kosten(x.len())?;
                    d = x;
                }
                Err(filter::Fehler::Voll) => {
                    self.rest.set(0);
                    self.voll.set(true);
                    return None;
                }
                Err(filter::Fehler::Nichts) => return None,
            }
        }
        Some(d)
    }

    /// `n` Bytes vom [`BUDGET`]; `None`, wenn es nicht reicht (dann ist es
    /// aufgebraucht und [`Pdf::zu_gross`] gilt).
    fn kosten(&self, n: usize) -> Option<()> {
        match self.rest.get().checked_sub(n) {
            Some(r) => {
                self.rest.set(r);
                Some(())
            }
            None => {
                self.rest.set(0);
                self.voll.set(true);
                None
            }
        }
    }

    /// Wollte die Datei mehr als das [`BUDGET`]? Dann fehlt, was danach kam.
    pub fn zu_gross(&self) -> bool {
        self.voll.get()
    }

    /// Der Katalog (`/Root`).
    pub fn katalog(&self) -> Option<Verz> {
        self.verz(&self.trailer, "Root")
    }

    /// Erzeuger aus `/Info` (Producer und Creator), leer, wenn keiner da ist.
    pub fn erzeuger(&self) -> String {
        let Some(info) = self.verz(&self.trailer, "Info") else {
            return String::new();
        };
        ["Producer", "Creator"]
            .iter()
            .filter_map(|k| info.get(k).map(|o| self.aufloesen(o)))
            .filter_map(|o| o.text().map(objekt::text))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Ist die Datei verschlüsselt? Gibt V und R.
    pub fn schutz(&self) -> Option<(i64, i64)> {
        self.schutz.as_ref().map(|s| (s.v, s.r))
    }

    /// Alle Seiten in Reihenfolge, mit geerbten Ressourcen, MediaBox und
    /// Drehung.
    pub fn seiten(&self) -> Vec<Blatt> {
        let mut aus = Vec::new();
        let Some(k) = self.katalog() else {
            return aus;
        };
        let Some(wurzel) = k.get("Pages") else {
            return aus;
        };
        let mut gesehen = BTreeSet::new();
        self.knoten(wurzel, None, None, None, &mut gesehen, &mut aus, 0);
        aus
    }

    #[allow(clippy::too_many_arguments)]
    fn knoten(
        &self,
        o: &Objekt,
        res: Option<&Verz>,
        mb: Option<[f64; 4]>,
        dreh: Option<i64>,
        gesehen: &mut BTreeSet<u32>,
        aus: &mut Vec<Blatt>,
        tiefe: usize,
    ) {
        if tiefe > 64 {
            return;
        }
        if let Objekt::Bezug(n, _) = o {
            if !gesehen.insert(*n) {
                return;
            }
        }
        let Some(v) = self.aufloesen(o).verz().cloned() else {
            return;
        };
        let res = self.verz(&v, "Resources").or_else(|| res.cloned());
        let mb = self.rechteck(&v, "MediaBox").or(mb);
        let dreh = self
            .aufloesen(v.get("Rotate").unwrap_or(&Objekt::Null))
            .ganz()
            .or(dreh);
        match self.aufloesen(v.get("Kids").unwrap_or(&Objekt::Null)) {
            Objekt::Feld(kids) if v.name("Type") != Some("Page") => {
                for kid in &kids {
                    self.knoten(kid, res.as_ref(), mb, dreh, gesehen, aus, tiefe + 1);
                }
            }
            _ => aus.push(Blatt {
                verz: v,
                ressourcen: res.unwrap_or_default(),
                mediabox: mb.unwrap_or([0.0, 0.0, 595.276, 841.89]),
                drehung: dreh.unwrap_or(0).rem_euclid(360),
            }),
        }
    }

    fn rechteck(&self, v: &Verz, k: &str) -> Option<[f64; 4]> {
        let f = self.aufloesen(v.get(k)?);
        let z: Vec<f64> = f
            .feld()?
            .iter()
            .filter_map(|o| self.aufloesen(o).zahl())
            .collect();
        (z.len() == 4).then(|| {
            [
                z[0].min(z[2]),
                z[1].min(z[3]),
                z[0].max(z[2]),
                z[1].max(z[3]),
            ]
        })
    }

    /// Inhalt einer Seite: alle Inhaltsströme hintereinander.
    pub fn inhalt(&self, b: &Blatt) -> Vec<u8> {
        let mut aus = Vec::new();
        let c = match b.verz.get("Contents") {
            Some(o) => self.aufloesen(o),
            None => return aus,
        };
        let teile = match c {
            Objekt::Feld(f) => f.iter().map(|o| self.aufloesen(o)).collect(),
            o => vec![o],
        };
        for t in teile {
            if let Some(d) = t.strom().and_then(|s| self.daten(s)) {
                aus.extend_from_slice(&d);
                aus.push(b'\n');
            }
        }
        aus
    }
}

#[cfg(test)]
mod tests;
