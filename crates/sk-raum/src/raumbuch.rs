//! Raumbuch eines Raums (RB3, `raumbuch/plan.md` §5 und §6): die Mengen
//! je Fläche und Schicht nach VOB/C, jede mit ihrem Rechenweg. Die Terme
//! ergeben in Summe genau die Menge.
//!
//! Gemessen wird nach der VOB-Prüfung Raumbuch (11.10. 18:10) und Jörns
//! Nachtregel (18:41, nach Empfehlung entschieden, änderbar):
//! - Estrich und Bodenbelag: die hergestellte bzw. belegte Fläche bis zur
//!   Putzoberfläche (Putz kommt vor dem Estrich, Wandfliesen stehen auf dem
//!   Bodenbelag), dazu der Streifen vor jeder Öffnung am Boden und die
//!   Nische: bei der Innentür auf der Gegenseite des Aufschlags, ohne
//!   Aufschlag hälftig, bei Fenstertür und Haustür bis zum Element.
//!   Aussparungen bis 0,5 m² übermessen.
//! - Putz: Länge im Rohbaumaß × OK Rohdecke bis UK Rohdecke; Tapete und
//!   Anstrich ebenso, aber erst über den Wandfliesen. Öffnungen bis 2,5 m²
//!   übermessen, Laibungen gesondert in m, nur wo sie behandelt werden.
//! - Wandfliesen: Länge an der Putzoberfläche × Fliesenhöhe ab OK
//!   Fertigfußboden; Öffnungen bis 2,5 m² übermessen; Laibungen in der
//!   Fliesenzone samt geflieser Fensterbank in m.
//! - Decke: Rohbaumaß, Aussparungen bis 2,5 m² übermessen.
//! - Sockelleiste: Länge an der Putzoberfläche, Öffnungen am Boden bis
//!   1,00 m übermessen.
//! - Eckschienen an vorspringenden Ecken über die Rohbauhöhe und an den
//!   Laibungskanten; Randdämmstreifen am Umfang des Estrichs ohne Abzug;
//!   Silikon an Boden/Wand und in den Innenecken der Fliesenzone;
//!   Dichtband unter Fliesen mit Verbundabdichtung.
//!
//! Die Grenzen stehen in [`Regeln`], bis die Tabelle `Abrechnung` des
//! VOB-Threads sie liefert. [`Term`], [`Art`] und [`Bezug`] sind die aus
//! `sk_model::rechenweg` (VOB, PR #77).

use crate::ausbau::{self, laibung_behandelt, RaumAusbau};
use crate::erkennung::{Begrenzung, Kante};
use crate::oeffnungen::{OeffnungAmRaum, OpeningKind, Rohbau};
use crate::ring::{self, Versatz};
use sk_math::{vec3, Vec3};
use sk_model::{LayerFunction, LayerSetId, Model};

/// Bezug, Rolle und Term einer Menge wie in der Mengenliste (VOB, PR #77).
pub use sk_model::rechenweg::{Art, Bezug, Term};

/// Grenzen der Übermessung (VOB-Prüfung Raumbuch, Vorgabe für Skizzeo).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regeln {
    /// Aussparungen in Estrich und Bodenbelag (mm²).
    pub boden: f64,
    /// Öffnungen in Putz, Tapete, Anstrich, Wandfliesen; Aussparungen in der
    /// Decke (mm²).
    pub wand: f64,
    /// Unterbrechungen der Sockelleiste (mm, Rohbaumaß der Öffnung).
    pub sockel: f64,
}

impl Default for Regeln {
    fn default() -> Regeln {
        Regeln {
            boden: 0.5e6,
            wand: 2.5e6,
            sockel: 1000.0,
        }
    }
}

/// Fläche, zu der eine Zeile gehört; Wandseiten nach der Kante in der
/// Zählung von [`Rohbau::kanten`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flaeche {
    Boden,
    Estrich,
    Decke,
    Wand(usize),
    Laibung(usize),
    Fliesen(usize),
    FliesenLaibung(usize),
    Sockel,
    Eckschienen,
    Randstreifen,
    Silikon,
    Dichtband,
}

/// Automatik-Schlüssel der Mengen in m (plan.md §6, mit Kosten abgestimmt).
pub const SOCKEL: &str = "room.skirting";
pub const ECKSCHIENE: &str = "room.cornerbead";
pub const RANDSTREIFEN: &str = "room.edgestrip";
pub const SILIKON: &str = "room.joint.silicone";

/// Eine Zeile des Raumbuchs.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    pub flaeche: Flaeche,
    /// Ausbautyp und Schicht darin; leer bei Mengen ohne Schicht.
    pub typ: Option<(LayerSetId, usize)>,
    /// Wirksame Dicke der Schicht (mm); im Estrich mit Ausgleich.
    pub dicke: f64,
    pub schluessel: Option<&'static str>,
    pub bezug: Bezug,
    pub terme: Vec<Term>,
}

impl Zeile {
    /// Menge: Summe der Terme (mm bzw. mm²).
    pub fn menge(&self) -> f64 {
        self.terme.iter().map(|t| t.wert).sum()
    }
}

/// „4,240“ aus mm.
fn m(x: f64) -> String {
    format!("{:.3}", x / 1e3).replace('.', ",")
}

/// „2,156 m²“ aus mm².
fn qm(x: f64) -> String {
    format!("{} m²", format!("{:.3}", x / 1e6).replace('.', ","))
}

fn term(text: String, wert: f64, art: Art) -> Term {
    Term { text, wert, art }
}

fn hinweis(text: String) -> Term {
    term(text, 0.0, Art::Hinweis)
}

/// Überdeckung der Bereiche `a0..a1` und `b0..b1` (mm, nie negativ).
fn ueber(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Alles, was die Zeilen eines Raums gemeinsam brauchen.
struct Raum<'a> {
    model: &'a Model,
    form: &'a Rohbau,
    aus: &'a RaumAusbau,
    regeln: &'a Regeln,
    kanten: Vec<&'a Kante>,
    /// Je Ring die Nummer seiner ersten Kante und seine Punkte.
    ringe: Vec<(usize, Vec<Vec3>)>,
    /// Je Kante: Putzdicke, Rohbaulänge, Länge an der Putzoberfläche.
    putz: Vec<f64>,
    roh: Vec<f64>,
    an_putz: Vec<f64>,
    /// Je Ring nach innen versetzt bis zur Putzoberfläche.
    ringe_putz: Vec<Versatz>,
    ffb: f64,
    estrich: Vec<f64>,
}

impl<'a> Raum<'a> {
    fn new(model: &'a Model, form: &'a Rohbau, aus: &'a RaumAusbau, regeln: &'a Regeln) -> Self {
        let kanten: Vec<&Kante> = form.kanten().collect();
        let mut ringe = Vec::new();
        let mut erste = 0;
        for r in form.ringe() {
            ringe.push((erste, r.iter().map(|k| k.a).collect::<Vec<Vec3>>()));
            erste += r.len();
        }
        let leer = ausbau::SeitenAusbau::default();
        let putz: Vec<f64> = kanten
            .iter()
            .enumerate()
            .map(|(i, k)| ausbau::putz(model, k, aus.seiten.get(i).unwrap_or(&leer)))
            .collect();
        let roh = kanten.iter().map(|k| ring::laenge(k.a, k.b)).collect();
        let ringe_putz: Vec<Versatz> = ringe
            .iter()
            .map(|(e, p)| ring::versetzt(p, &putz[*e..*e + p.len()]))
            .collect();
        let an_putz = ringe_putz
            .iter()
            .flat_map(|v| v.laengen.iter().copied())
            .collect();
        let (belag, estrich) = ausbau::boden(model, aus);
        let ffb = form.z_boden + estrich.iter().sum::<f64>() + belag;
        Raum {
            model,
            form,
            aus,
            regeln,
            kanten,
            ringe,
            putz,
            roh,
            an_putz,
            ringe_putz,
            ffb,
            estrich,
        }
    }

    fn hoehe(&self) -> f64 {
        self.form.z_decke - self.form.z_boden
    }

    fn seite(&self, k: usize) -> ausbau::SeitenAusbau {
        self.aus.seiten.get(k).copied().unwrap_or_default()
    }

    /// Ring und Stelle der Kante `k`.
    fn im_ring(&self, k: usize) -> (usize, usize) {
        let r = self.ringe.iter().rposition(|(e, _)| *e <= k).unwrap_or(0);
        (r, k - self.ringe[r].0)
    }

    fn vorige(&self, k: usize) -> usize {
        let (r, i) = self.im_ring(k);
        let (e, p) = &self.ringe[r];
        e + (i + p.len() - 1) % p.len()
    }

    fn naechste(&self, k: usize) -> usize {
        let (r, i) = self.im_ring(k);
        let (e, p) = &self.ringe[r];
        e + (i + 1) % p.len()
    }

    /// Name einer Wandseite: Nummer der Wand.
    fn name_seite(&self, k: usize) -> String {
        match self.kanten.get(k).map(|k| &k.quelle) {
            Some(Begrenzung::Wand { wand, .. }) => self
                .model
                .element(*wand)
                .map_or_else(|| "Wand".into(), |e| e.number.clone()),
            None => "Wand".into(),
        }
    }

    fn name(&self, o: &OeffnungAmRaum) -> String {
        self.model
            .element(o.oeffnung)
            .map_or_else(|| "Öffnung".into(), |e| e.number.clone())
    }

    fn an(&self, k: usize) -> impl Iterator<Item = &'a OeffnungAmRaum> + '_ {
        self.form.oeffnungen.iter().filter(move |o| o.kante == k)
    }

    /// Reicht die Öffnung bis zum Boden (Tür, Fenstertür)?
    fn am_boden(&self, o: &OeffnungAmRaum) -> bool {
        o.uk <= self.ffb + 0.5
    }

    /// Setzt sich die Öffnung über die Ecke am Anfang bzw. Ende ihrer Kante
    /// fort (Ecköffnung)? Dort hat sie keine seitliche Laibung.
    fn ueber_ecke(&self, o: &OeffnungAmRaum) -> (bool, bool) {
        let fort = |k: usize, ende: bool| {
            self.an(k).any(|p| {
                p.oeffnung == o.oeffnung
                    && if ende {
                        p.bis >= self.roh[k] - 0.5
                    } else {
                        p.von <= 0.5
                    }
            })
        };
        let anfang = o.von <= 0.5 && fort(self.vorige(o.kante), true);
        let ende = o.bis >= self.roh[o.kante] - 0.5 && fort(self.naechste(o.kante), false);
        (anfang, ende)
    }

    /// Öffnet eine Öffnung die Ecke vor Kante `k`?
    fn ecke_offen(&self, k: usize) -> bool {
        self.an(k).any(|o| self.ueber_ecke(o).0)
    }

    /// Seitliche Laibungen der Öffnung (0, 1 oder 2).
    fn seiten(&self, o: &OeffnungAmRaum) -> f64 {
        let (a, e) = self.ueber_ecke(o);
        2.0 - a as u8 as f64 - e as u8 as f64
    }

    /// „4,970 × 3,970“ für ein Rechteck, sonst „Umriss mit n Ecken“.
    fn mass(&self, laengen: &[f64]) -> String {
        let p = &self.ringe[0].1;
        let recht = p.len() == 4
            && (0..4).all(|i| {
                let a = p[i];
                let b = p[(i + 1) % 4];
                let c = p[(i + 2) % 4];
                let u = vec3(b.x - a.x, b.y - a.y, 0.0);
                let v = vec3(c.x - b.x, c.y - b.y, 0.0);
                u.dot(v).abs() < 1e-6 * u.length() * v.length()
            });
        if recht {
            format!("{} × {}", m(laengen[0]), m(laengen[1]))
        } else {
            format!("Umriss mit {} Ecken", p.len())
        }
    }

    // --- Boden ----------------------------------------------------------------

    /// Estrich und Bodenbelag bis zur Putzoberfläche, mit Aussparungen,
    /// Streifen vor den Öffnungen und Nischen.
    fn boden_terme(&self) -> Vec<Term> {
        let mut t = vec![term(
            format!("bis Putz {}", self.mass(&self.ringe_putz[0].laengen)),
            self.ringe_putz[0].flaeche(),
            Art::Basis,
        )];
        for v in &self.ringe_putz[1..] {
            let a = -v.flaeche();
            if a <= self.regeln.boden {
                t.push(hinweis(format!(
                    "Aussparung {} übermessen (bis {})",
                    qm(a),
                    qm(self.regeln.boden)
                )));
            } else {
                t.push(term(format!("Aussparung {}", qm(a)), -a, Art::Abzug));
            }
        }
        for o in self.form.oeffnungen.iter().filter(|o| self.am_boden(o)) {
            let w = o.bis - o.von;
            let p = self.putz[o.kante];
            let n = self.name(o);
            if p > 0.0 {
                t.push(term(
                    format!("vor {n}: {} × {}", m(w), m(p)),
                    w * p,
                    Art::Zuschlag,
                ));
            }
            let d = o.tiefe;
            match (o.art, o.aufschlag_hier) {
                (OpeningKind::InnerDoor, Some(true)) => t.push(hinweis(format!(
                    "Türnische {n} gehört zum Nachbarraum (Tür schlägt hierher auf)"
                ))),
                (OpeningKind::InnerDoor, None) => t.push(term(
                    format!(
                        "Türnische {n}: {} × {} / 2 (ohne Aufschlag hälftig)",
                        m(w),
                        m(d)
                    ),
                    w * d * 0.5,
                    Art::Zuschlag,
                )),
                (OpeningKind::InnerDoor, Some(false)) => t.push(term(
                    format!("Türnische {n}: {} × {}", m(w), m(d)),
                    w * d,
                    Art::Zuschlag,
                )),
                _ => t.push(term(
                    format!("Nische {n} bis zum Element: {} × {}", m(w), m(d)),
                    w * d,
                    Art::Zuschlag,
                )),
            }
            // Ecköffnung: das Eckfeld der Nische fehlt, der Streifen vor der
            // Öffnung zählt im Eck doppelt
            if self.ueber_ecke(o).1 && o.art != OpeningKind::InnerDoor {
                let k = self.naechste(o.kante);
                if let Some(q) = self
                    .an(k)
                    .find(|q| q.oeffnung == o.oeffnung && q.von <= 0.5)
                {
                    t.push(term(
                        format!("Eckfeld {n}: {} × {}", m(d), m(q.tiefe)),
                        d * q.tiefe,
                        Art::Zuschlag,
                    ));
                    let pq = self.putz[k];
                    if p > 0.0 && pq > 0.0 {
                        t.push(term(
                            format!("vor {n} im Eck doppelt: {} × {}", m(p), m(pq)),
                            -p * pq,
                            Art::Abzug,
                        ));
                    }
                }
            }
        }
        t
    }

    fn boden(&self, out: &mut Vec<Zeile>) {
        let terme = self.boden_terme();
        let schichten = |id: Option<LayerSetId>| {
            id.and_then(|id| Some((id, self.model.layer_set(id)?)))
                .map_or_else(Vec::new, |(id, t)| {
                    t.layers
                        .iter()
                        .enumerate()
                        .map(|(j, l)| (id, j, l.thickness))
                        .collect()
                })
        };
        for (id, j, d) in schichten(self.aus.boden) {
            out.push(Zeile {
                flaeche: Flaeche::Boden,
                typ: Some((id, j)),
                dicke: d,
                schluessel: None,
                bezug: Bezug::Flaeche,
                terme: terme.clone(),
            });
        }
        for (id, j, _) in schichten(self.aus.estrich) {
            out.push(Zeile {
                flaeche: Flaeche::Estrich,
                typ: Some((id, j)),
                dicke: self.estrich.get(j).copied().unwrap_or(0.0),
                schluessel: None,
                bezug: Bezug::Flaeche,
                terme: terme.clone(),
            });
        }
    }

    // --- Decke ----------------------------------------------------------------

    fn decke(&self, out: &mut Vec<Zeile>) {
        let Some((id, t)) = self
            .aus
            .decke
            .and_then(|id| Some((id, self.model.layer_set(id)?)))
        else {
            return;
        };
        let roh: Vec<f64> = self.roh[..self.ringe[0].1.len()].to_vec();
        let mut terme = vec![term(
            format!("Rohbaumaß {}", self.mass(&roh)),
            sk_math::polygon::signed_area(&self.ringe[0].1),
            Art::Basis,
        )];
        for (_, p) in &self.ringe[1..] {
            let a = -sk_math::polygon::signed_area(p);
            if a <= self.regeln.wand {
                terme.push(hinweis(format!(
                    "Aussparung {} übermessen (bis {})",
                    qm(a),
                    qm(self.regeln.wand)
                )));
            } else {
                terme.push(term(format!("Aussparung {}", qm(a)), -a, Art::Abzug));
            }
        }
        for (j, l) in t.layers.iter().enumerate() {
            out.push(Zeile {
                flaeche: Flaeche::Decke,
                typ: Some((id, j)),
                dicke: l.thickness,
                schluessel: None,
                bezug: Bezug::Flaeche,
                terme: terme.clone(),
            });
        }
    }

    // --- Wand -----------------------------------------------------------------

    /// Oberkante der Wandfliesen an Seite `k` (absolut).
    fn fliesen_oben(&self, k: usize) -> Option<f64> {
        self.seite(k).fliesen.map(|f| self.ffb + f.1)
    }

    /// Öffnungen einer Zone `lo..hi` der Seite `k` (Länge im Rohbaumaß),
    /// übermessen bis `grenze`.
    fn oeffnungen_in(&self, k: usize, lo: f64, hi: f64, terme: &mut Vec<Term>) {
        for o in self.an(k) {
            let w = o.bis - o.von;
            let h = ueber(o.uk, o.ok, lo, hi);
            if h <= 0.0 {
                continue;
            }
            let a = w * h;
            let n = self.name(o);
            if a <= self.regeln.wand {
                terme.push(hinweis(format!(
                    "{n} {} × {} = {} übermessen (bis {})",
                    m(w),
                    m(h),
                    qm(a),
                    qm(self.regeln.wand)
                )));
            } else {
                terme.push(term(format!("{n} {} × {}", m(w), m(h)), -a, Art::Abzug));
            }
        }
    }

    /// Laibungen der Seite `k` in der Zone `lo..hi`: seitlich, oben, und
    /// mit `sohlbank` unten (geflieste Fensterbank).
    fn laibungen_in(&self, k: usize, lo: f64, hi: f64, sohlbank: bool) -> Vec<Term> {
        let mut t = Vec::new();
        for o in self.an(k).filter(|o| laibung_behandelt(o.art)) {
            let w = o.bis - o.von;
            let h = ueber(o.uk, o.ok, lo, hi);
            let n = self.seiten(o);
            let oben = o.ok > lo + 0.5 && o.ok <= hi + 0.5;
            let unten = sohlbank && !self.am_boden(o) && o.uk >= lo - 0.5 && o.uk < hi - 0.5;
            let wert = n * h + (oben as u8 + unten as u8) as f64 * w;
            if wert <= 0.0 {
                continue;
            }
            let mut text = format!("{}: ", self.name(o));
            let mut teile = Vec::new();
            if n * h > 0.0 {
                teile.push(format!("{} × {}", n, m(h)));
            }
            if oben {
                teile.push(format!("{} oben", m(w)));
            }
            if unten {
                teile.push(format!("{} unten", m(w)));
            }
            text.push_str(&teile.join(" + "));
            t.push(term(text, wert, Art::Basis));
        }
        t
    }

    fn wand(&self, k: usize, out: &mut Vec<Zeile>) {
        let Some((id, t)) = self
            .seite(k)
            .wand
            .and_then(|id| Some((id, self.model.layer_set(id)?)))
        else {
            return;
        };
        let (z0, z1) = (self.form.z_boden, self.form.z_decke);
        let putz_im_typ = !self.kanten[k].ausbau_typ.is_empty();
        for (j, l) in t.layers.iter().enumerate() {
            if putz_im_typ && l.thickness > 0.0 {
                continue; // der Putz kommt mit dem Wandtyp (§5.6)
            }
            // Putz bis zur Rohdecke, Tapete und Anstrich erst über den Fliesen
            let lo = match self.fliesen_oben(k) {
                Some(f) if l.thickness <= 0.0 => f,
                _ => z0,
            };
            if z1 - lo <= 0.5 {
                continue;
            }
            let mut terme = vec![term(
                format!(
                    "Länge × Höhe {} × {}{}",
                    m(self.roh[k]),
                    m(z1 - lo),
                    if lo > z0 { " über den Fliesen" } else { "" }
                ),
                self.roh[k] * (z1 - lo),
                Art::Basis,
            )];
            self.oeffnungen_in(k, lo, z1, &mut terme);
            out.push(Zeile {
                flaeche: Flaeche::Wand(k),
                typ: Some((id, j)),
                dicke: l.thickness,
                schluessel: None,
                bezug: Bezug::Flaeche,
                terme,
            });
            let terme = self.laibungen_in(k, lo, z1, false);
            if !terme.is_empty() {
                out.push(Zeile {
                    flaeche: Flaeche::Laibung(k),
                    typ: Some((id, j)),
                    dicke: l.thickness,
                    schluessel: None,
                    bezug: Bezug::Laenge,
                    terme,
                });
            }
        }
    }

    fn fliesen(&self, k: usize, out: &mut Vec<Zeile>) {
        let Some((id, h)) = self.seite(k).fliesen else {
            return;
        };
        let Some(t) = self.model.layer_set(id) else {
            return;
        };
        let (lo, hi) = (self.ffb, self.ffb + h);
        let mut terme = vec![term(
            format!("Länge × Höhe {} × {}", m(self.an_putz[k]), m(h)),
            self.an_putz[k] * h,
            Art::Basis,
        )];
        self.oeffnungen_in(k, lo, hi, &mut terme);
        let laibung = self.laibungen_in(k, lo, hi, true);
        for (j, l) in t.layers.iter().enumerate() {
            out.push(Zeile {
                flaeche: Flaeche::Fliesen(k),
                typ: Some((id, j)),
                dicke: l.thickness,
                schluessel: None,
                bezug: Bezug::Flaeche,
                terme: terme.clone(),
            });
            if !laibung.is_empty() {
                out.push(Zeile {
                    flaeche: Flaeche::FliesenLaibung(k),
                    typ: Some((id, j)),
                    dicke: l.thickness,
                    schluessel: None,
                    bezug: Bezug::Laenge,
                    terme: laibung.clone(),
                });
            }
        }
    }

    // --- Längen ---------------------------------------------------------------

    fn sockel(&self, out: &mut Vec<Zeile>) {
        let mut typen: Vec<LayerSetId> = Vec::new();
        for k in 0..self.kanten.len() {
            if let Some(s) = self.seite(k).sockel {
                if !typen.contains(&s) {
                    typen.push(s);
                }
            }
        }
        for id in typen {
            let mut terme = Vec::new();
            for k in (0..self.kanten.len()).filter(|&k| self.seite(k).sockel == Some(id)) {
                terme.push(term(
                    format!("{}: {}", self.name_seite(k), m(self.an_putz[k])),
                    self.an_putz[k],
                    Art::Basis,
                ));
                for o in self.an(k).filter(|o| self.am_boden(o)) {
                    let w = o.bis - o.von;
                    let n = self.name(o);
                    if w <= self.regeln.sockel {
                        terme.push(hinweis(format!(
                            "{n} {} übermessen (bis {} m)",
                            m(w),
                            m(self.regeln.sockel)
                        )));
                    } else {
                        terme.push(term(format!("{n} {}", m(w)), -w, Art::Abzug));
                    }
                }
            }
            let dicke = self
                .model
                .layer_set(id)
                .and_then(|t| t.layers.first())
                .map_or(0.0, |l| l.thickness);
            out.push(Zeile {
                flaeche: Flaeche::Sockel,
                typ: Some((id, 0)),
                dicke,
                schluessel: Some(SOCKEL),
                bezug: Bezug::Laenge,
                terme,
            });
        }
    }

    fn eckschienen(&self, out: &mut Vec<Zeile>) {
        let mut terme = Vec::new();
        let h = self.hoehe();
        for (e, p) in &self.ringe {
            for i in 0..p.len() {
                let k = e + i;
                let v = self.vorige(k);
                let geputzt = self.putz[k] > 0.0 || self.putz[v] > 0.0;
                if ring::springt_vor(p, i) && geputzt && !self.ecke_offen(k) {
                    terme.push(term(
                        format!(
                            "Ecke {}/{}: {}",
                            self.name_seite(v),
                            self.name_seite(k),
                            m(h)
                        ),
                        h,
                        Art::Basis,
                    ));
                }
            }
        }
        for o in &self.form.oeffnungen {
            if !laibung_behandelt(o.art) || self.putz[o.kante] <= 0.0 {
                continue;
            }
            let w = o.bis - o.von;
            let n = self.seiten(o);
            let hoch = o.ok - o.uk;
            terme.push(term(
                format!(
                    "Laibungskanten {}: {} × {} + {}",
                    self.name(o),
                    n,
                    m(hoch),
                    m(w)
                ),
                n * hoch + w,
                Art::Basis,
            ));
        }
        if !terme.is_empty() {
            out.push(Zeile {
                flaeche: Flaeche::Eckschienen,
                typ: None,
                dicke: 0.0,
                schluessel: Some(ECKSCHIENE),
                bezug: Bezug::Laenge,
                terme,
            });
        }
    }

    fn randstreifen(&self, out: &mut Vec<Zeile>) {
        if self.aus.estrich.is_none() {
            return;
        }
        let mut terme = vec![term(
            "Umfang bis Putz".into(),
            self.ringe_putz[0].umfang(),
            Art::Basis,
        )];
        for v in &self.ringe_putz[1..] {
            terme.push(term("um Aussparung".into(), v.umfang(), Art::Zuschlag));
        }
        if self.form.oeffnungen.iter().any(|o| self.am_boden(o)) {
            terme.push(hinweis("Türöffnungen nicht abgezogen".into()));
        }
        out.push(Zeile {
            flaeche: Flaeche::Randstreifen,
            typ: None,
            dicke: 0.0,
            schluessel: Some(RANDSTREIFEN),
            bezug: Bezug::Laenge,
            terme,
        });
    }

    /// Boden/Wand-Fuge an den Seiten `seiten`, Öffnungen am Boden abgezogen.
    fn fuge_unten(&self, seiten: impl Fn(usize) -> bool, terme: &mut Vec<Term>) {
        for k in (0..self.kanten.len()).filter(|&k| seiten(k)) {
            terme.push(term(
                format!("Boden/Wand {}: {}", self.name_seite(k), m(self.an_putz[k])),
                self.an_putz[k],
                Art::Basis,
            ));
            for o in self.an(k).filter(|o| self.am_boden(o)) {
                let w = o.bis - o.von;
                terme.push(term(format!("{} {}", self.name(o), m(w)), -w, Art::Abzug));
            }
        }
    }

    fn silikon(&self, out: &mut Vec<Zeile>) {
        let gefliest = |k: usize| self.seite(k).fliesen.map(|f| f.1);
        let mut terme = Vec::new();
        self.fuge_unten(|k| gefliest(k).is_some(), &mut terme);
        for (e, p) in &self.ringe {
            for i in 0..p.len() {
                let k = e + i;
                let v = self.vorige(k);
                let h = gefliest(v).unwrap_or(0.0).max(gefliest(k).unwrap_or(0.0));
                if h > 0.0 && ring::innenecke(p, i) && !self.ecke_offen(k) {
                    terme.push(term(
                        format!(
                            "Innenecke {}/{}: {}",
                            self.name_seite(v),
                            self.name_seite(k),
                            m(h)
                        ),
                        h,
                        Art::Basis,
                    ));
                }
            }
        }
        if !terme.is_empty() {
            out.push(Zeile {
                flaeche: Flaeche::Silikon,
                typ: None,
                dicke: 0.0,
                schluessel: Some(SILIKON),
                bezug: Bezug::Laenge,
                terme,
            });
        }
    }

    fn dichtband(&self, out: &mut Vec<Zeile>) {
        let abgedichtet = self
            .aus
            .boden
            .and_then(|id| self.model.layer_set(id))
            .is_some_and(|t| {
                t.layers
                    .iter()
                    .any(|l| l.function == LayerFunction::Membrane)
            });
        if !abgedichtet {
            return;
        }
        let mut terme = Vec::new();
        let wand = |k: usize| matches!(self.kanten[k].quelle, Begrenzung::Wand { .. });
        self.fuge_unten(wand, &mut terme);
        out.push(Zeile {
            flaeche: Flaeche::Dichtband,
            typ: None,
            dicke: 0.0,
            schluessel: None,
            bezug: Bezug::Laenge,
            terme,
        });
    }
}

/// Die Zeilen des Raumbuchs eines Raums: Boden, Estrich, Decke, je
/// Wandseite Bekleidung, Laibungen und Fliesen, dann Sockelleisten,
/// Eckschienen, Randdämmstreifen, Silikon und Dichtband.
pub fn rechnen(model: &Model, form: &Rohbau, aus: &RaumAusbau, regeln: &Regeln) -> Vec<Zeile> {
    let r = Raum::new(model, form, aus, regeln);
    let mut out = Vec::new();
    r.boden(&mut out);
    r.decke(&mut out);
    for k in 0..r.kanten.len() {
        r.wand(k, &mut out);
        r.fliesen(k, &mut out);
    }
    r.sockel(&mut out);
    r.eckschienen(&mut out);
    r.randstreifen(&mut out);
    r.silikon(&mut out);
    r.dichtband(&mut out);
    out
}

#[path = "koerper.rs"]
pub mod koerper;

#[cfg(test)]
#[path = "raumbuch_tests.rs"]
mod tests;
