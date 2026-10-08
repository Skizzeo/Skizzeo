//! Verwaltung mit Verwaltungskennwort: Entwurf, Vorschau, Freigabe (KA-3b2
//! und KA-3b3, paket-ka3b §1 und §3, soll-ka-3b-freigabe).
//!
//! Der Administrator ändert einen Entwurf (`firmenkatalog.entwurf.szk`).
//! Jede Eingabe wird gleich dorthin geschrieben (`Out::entwurf`); OK und
//! Abbrechen gibt es dann nicht. Oben rechts steht die Pille „Entwurf · n
//! Änderungen“, unten die Leiste „Entwurf: n Änderungen seit Stand 3 · die
//! anderen Plätze sehen weiter Stand 3“, im Baum ein Punkt an geänderten
//! Einträgen. Die Vorschau ist ein Blatt über der Verwaltung: Änderungen alt
//! → neu mit Herkunft, je Zeile „Änderung verwerfen“, die Referenzhäuser
//! mit Stand, Entwurf, Änderung und €/m² Grundfläche, und „Freigeben als
//! Stand n+1“ als einzige Akzentfläche.

use super::*;
use sk_cost::Cent;

/// Der Entwurf lässt sich nicht lesen.
pub(super) const KAPUTT: &str =
    "Der Entwurf ist nicht lesbar; die Verwaltung zeigt den freigegebenen Stand und speichert nichts.";
/// Hinweis unter der Vorschau.
const FREIGEBEN_SATZ: &str = "Projekte bleiben auf ihrem Stand, bis sie übernehmen.";

#[cfg(test)]
pub(super) const SAETZE: [&str; 2] = [KAPUTT, FREIGEBEN_SATZ];

/// Blatt „Vorschau“ (dip, soll-ka-3b).
const V_W: f32 = 860.0;
const V_H: f32 = 560.0;
const V_KOPF: f32 = 54.0;
const V_FUSS: f32 = 58.0;
const PAD: f32 = 22.0;
const ZEILE_H: f32 = 30.0;
const HAUS_H: f32 = 44.0;
/// Spalten der Änderungen (rechte Kanten für Zahlen).
const NAME_X: f32 = 140.0;
const ALT_R: f32 = 560.0;
const PFEIL_X: f32 = 576.0;
const NEU_R: f32 = 700.0;
/// Spalten der Referenzhäuser (rechte Kanten).
const STAND_R: f32 = 420.0;
const ENTWURF_R: f32 = 540.0;
const AEND_R: f32 = 650.0;
/// Ab dieser Änderung (‰) ist der Balken voll.
const BALKEN_VOLL: f64 = 50.0;

/// Der freigegebene Stand neben dem Entwurf.
pub(super) struct Freigabe {
    /// Datei beim Lesen (erkennt eine Freigabe an einem anderen Platz).
    pub text: String,
    pub lib: Library,
    pub kat: Katalog,
    pub stand: u32,
    /// Sätze, in denen der Entwurf samt gesammelten Eingaben abweicht.
    pub saetze: Vec<SatzId>,
    /// „Stand 4 freigegeben …“ nach der Freigabe an diesem Platz.
    pub hinweis: Option<String>,
}

impl Freigabe {
    pub fn neu(m: &Model, c: &Company) -> Freigabe {
        let lib = c.library().clone();
        let kat = sk_cost::lesen::firma_oder_werk(m, Some(&lib));
        let stand = kat.firma_stand.unwrap_or(0);
        Freigabe {
            text: c.geladen().to_string(),
            lib,
            kat,
            stand,
            saetze: Vec::new(),
            hinweis: None,
        }
    }
}

/// Grundlage der Verwaltung: Text, auf den die Eingaben gehen, und seine
/// Bibliothek. Mit Kennwort (`entwurf`) der offene Entwurf, gelesen wie ein
/// Firmenkatalog; ohne offenen Entwurf der freigegebene Stand. Das dritte
/// Feld: Der Entwurf ist nicht lesbar (die Bibliothek ist dann der
/// freigegebene Stand, und nichts wird gespeichert).
pub(super) fn grundlage(company: Option<&Company>, entwurf: bool) -> (String, Library, bool) {
    let Some(c) = company else {
        // Ohne Datei braucht die Vorschau einen lesbaren leeren Katalog
        let lib = Library::standard();
        return (sk_model::write_szk(&lib), lib, false);
    };
    if let Some(t) = c.entwurf().filter(|_| entwurf) {
        return match sk_model::read_szk_with(t, &sk_cost::lesen::ABSCHNITTE_SZK) {
            Ok(l) => (
                t.to_string(),
                sk_cost::verwaltung::wie_freigegeben(&l),
                false,
            ),
            Err(_) => (t.to_string(), c.library().clone(), true),
        };
    }
    let lib = c.library().clone();
    // Ohne Datei schreibt die erste Änderung sie neu
    let basis = if c.geladen().trim().is_empty() {
        sk_model::write_szk(&lib)
    } else {
        c.geladen().to_string()
    };
    (basis, lib, false)
}

/// Eine Zeile der Änderungen.
#[derive(Clone, Debug)]
pub(super) struct VZeile {
    pub satz: SatzId,
    pub art: String,
    pub name: String,
    pub alt: String,
    pub neu: String,
    pub herkunft: String,
}

/// Ein Referenzhaus in der Vorschau.
#[derive(Clone, Debug)]
pub(super) struct VHaus {
    pub name: String,
    pub stand: Cent,
    pub entwurf: Cent,
    /// Grundfläche in mm².
    pub flaeche: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VZiel {
    Schliessen,
    Freigeben,
    /// „Entwurf verwerfen …“ (fragt erst).
    Weg,
    Zurueck,
    Verwerfen,
    /// „Änderung verwerfen“ an Zeile i.
    Zeile(usize),
}

/// Blatt „Vorschau · Entwurf → Stand n+1“.
pub(super) struct Vorschau {
    pub(super) zeilen: Vec<VZeile>,
    pub(super) haeuser: Vec<VHaus>,
    /// „Den größten Teil macht …“.
    pub(super) satz: Option<String>,
    /// Was die Freigabe sperrt.
    pub(super) befunde: Vec<String>,
    hover: Option<VZiel>,
    /// Zeile unter der Maus („Änderung verwerfen“ erscheint dort).
    hover_zeile: Option<usize>,
    pressed: Option<VZiel>,
    scroll: f32,
    /// Rückfrage „Den ganzen Entwurf verwerfen?“.
    frage: bool,
    meldung: Option<String>,
}

/// €/m² Grundfläche auf ganze €.
fn je_m2(c: Cent, flaeche: f64) -> Option<i64> {
    (flaeche > 0.0).then(|| (c.0 as f64 / 100.0 / (flaeche / 1e6)).round() as i64)
}

impl Verwaltung {
    /// Der freigegebene Katalog (Protokoll, Kopfzeile); ohne Kennwort der
    /// beim Öffnen.
    pub(super) fn freigegeben(&self) -> &Katalog {
        self.freigabe.as_ref().map_or(&self.vorher, |f| &f.kat)
    }

    /// Änderungen im Entwurf gegenüber dem freigegebenen Stand.
    pub(super) fn entwurf_anzahl(&self) -> usize {
        self.freigabe.as_ref().map_or(0, |f| f.saetze.len())
    }

    pub(super) fn entwurf_zaehlen(&mut self) {
        if let Some(f) = self.freigabe.as_mut() {
            f.saetze = sk_cost::verwaltung::entwurf_saetze(&f.lib, &self.lib);
        }
    }

    /// Ändert der Entwurf diesen Eintrag (Punkt im Baum)?
    pub(super) fn entwurf_geaendert(&self, k: &Knoten) -> bool {
        let Some(f) = &self.freigabe else {
            return false;
        };
        f.saetze.iter().any(|s| {
            let g = Guid::from_ifc(&s.kennung);
            match (s.abschnitt, k) {
                ("article", Knoten::ArtikelSatz(x)) | ("service", Knoten::Leistung(x)) => {
                    g == Some(*x)
                }
                ("lot", Knoten::Los(x) | Knoten::Titel(x) | Knoten::LosSatz(x)) => g == Some(*x),
                ("rate", Knoten::Firmenwerte) | ("catalog", Knoten::Kennwort) => true,
                _ => false,
            }
        })
    }

    /// Mit Kennwort: Sind gesammelte Eingaben in den Entwurf zu schreiben?
    /// Dieselben Operationen nach einem Fehler nicht noch einmal.
    pub fn entwurf_faellig(&mut self) -> bool {
        if self.freigabe.is_none()
            || self.ohne_firma
            || self.ops.is_empty()
            || self.gesperrt()
            || self.ops == self.versucht
        {
            return false;
        }
        self.versucht = self.ops.clone();
        true
    }

    /// Der Entwurf ist geschrieben (oder die Freigabe, ein Verwerfen):
    /// gesammelte Eingaben sind jetzt Grundlage. Ist das Kennwort mit der
    /// Freigabe entfernt, arbeitet die Verwaltung wieder als Einzelplatz.
    pub fn entwurf_gespeichert(&mut self, company: &Company) {
        self.ops.clear();
        self.versucht.clear();
        self.zurueck = None;
        self.umkehr_ops.clear();
        self.nach_grundlage = None;
        if !sk_cost::verwaltung::hat_kennwort(company.library()) {
            self.freigabe = None;
        } else if self
            .freigabe
            .as_ref()
            .is_none_or(|f| f.text != company.geladen())
        {
            self.freigabe = Some(Freigabe::neu(&self.m, company));
        }
        let (basis, lib0, kaputt) = grundlage(Some(company), self.freigabe.is_some());
        self.ohne_firma |= kaputt;
        self.basis_setzen(basis, lib0, false);
        self.neu_rechnen();
        if self.vorschau.is_some() {
            if self.entwurf_anzahl() == 0 {
                self.vorschau = None;
            } else {
                self.vorschau_oeffnen();
            }
        }
    }

    /// Nach „Freigeben“: Hinweis in der Leiste.
    pub fn freigegeben_als(&mut self, stand: u32) {
        if let Some(f) = self.freigabe.as_mut() {
            f.hinweis = Some(format!(
                "Stand {stand} freigegeben · die anderen Plätze sehen ihn nach dem Neuladen"
            ));
        }
    }

    /// Vorschau gescheitert (Freigeben, Verwerfen): Meldung im Blatt.
    pub fn vorschau_fehler(&mut self, text: String) {
        match self.vorschau.as_mut() {
            Some(v) => {
                v.meldung = Some(text);
                v.frage = false;
            }
            None => self.fehler(text),
        }
    }

    // --- Pille und Leiste -------------------------------------------------------

    /// Pille „Entwurf · n Änderungen“ oben rechts (px); ohne Änderung keine.
    pub(super) fn pille_rect(&self, w: &Win, fonts: &Fonts) -> Option<Rect> {
        let n = self.entwurf_anzahl();
        if n == 0 || self.abfrage.is_some() {
            return None;
        }
        let text = pille_text(n);
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(150.0, |f| f.width(&text, 12.0 * w.scale) / w.scale);
        let (ww, _) = self.dip(w);
        let bw = tw + 28.0;
        Some(self.r(w, ww - 56.0 - bw, 15.0, bw, 26.0))
    }

    pub(super) fn pille_malen(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some(r) = self.pille_rect(w, fonts) else {
            return;
        };
        let (s, u) = (w.scale, &t.ui);
        let hover = self.hover == Some(Ziel::Vorschau);
        rounded(c, r, r.h * 0.5, u.accent);
        let innen = Rect {
            x: r.x + s,
            y: r.y + s,
            w: r.w - 2.0 * s,
            h: r.h - 2.0 * s,
        };
        rounded(c, innen, innen.h * 0.5, if hover { u.hover } else { u.bg });
        let font = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let text = pille_text(self.entwurf_anzahl());
        label(
            c,
            font,
            &text,
            12.0 * s,
            r.x + 14.0 * s,
            r.y + r.h * 0.5 + 4.0 * s,
            u.accent,
        );
    }

    /// Fuß im Entwurf: Punkt, „Entwurf: n Änderungen seit Stand 3“ und „die
    /// anderen Plätze sehen weiter Stand 3“. Mit Meldung, Befund oder
    /// Rückfrage steht die Leiste darunter und `false` kommt zurück.
    pub(super) fn entwurf_fuss(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        fy: f32,
        rand: f32,
    ) -> bool {
        let Some(fg) = &self.freigabe else {
            return false;
        };
        let (s, u) = (w.scale, &t.ui);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let px = t.size.font_small * s;
        let allein = !self.frage && self.meldung.is_none() && self.befunde.is_empty();
        let base = if allein { fy + 37.0 * s } else { fy + 47.0 * s };
        let f = self.frame(w);
        let mut x = f.x + 20.0 * s;
        let n = self.entwurf_anzahl();
        let (vorn, hinten) = match (n, &fg.hinweis) {
            (0, Some(h)) => (String::new(), h.clone()),
            (0, None) => (
                String::new(),
                "Jede Änderung kommt in einen Entwurf; die anderen Plätze sehen sie erst nach „Freigeben“.".to_string(),
            ),
            (n, _) => (
                format!("Entwurf: {} seit Stand {}", aenderungen_text(n), fg.stand),
                format!(" · die anderen Plätze sehen weiter Stand {}", fg.stand),
            ),
        };
        if !vorn.is_empty() {
            let d = 3.5 * s;
            rounded(
                c,
                Rect {
                    x: x.round(),
                    y: (base - 4.0 * s - d).round(),
                    w: 2.0 * d,
                    h: 2.0 * d,
                },
                d,
                u.accent,
            );
            x += 2.0 * d + 8.0 * s;
            let v = widgets::ellipsize(bold, &vorn, px, rand - x);
            label(c, bold, &v, px, x, base, u.text);
            x += bold.map_or(0.0, |b| b.width(&v, px));
        }
        let h = widgets::ellipsize(regular, &hinten, px, rand - x);
        label(c, regular, &h, px, x, base, u.text_dim);
        allein
    }

    // --- Vorschau ---------------------------------------------------------------

    /// Vorschau öffnen bzw. neu rechnen (nach „Änderung verwerfen“).
    pub(super) fn vorschau_oeffnen(&mut self) {
        self.ende_edit(true);
        if self.freigabe.is_none() {
            return;
        }
        let zeilen = self.vorschau_zeilen();
        let befunde = self.vorschau_befunde();
        let (haeuser, satz) = self.vorschau_haeuser(&zeilen);
        let alt = self.vorschau.take();
        self.vorschau = Some(Vorschau {
            zeilen,
            haeuser,
            satz,
            befunde,
            hover: None,
            hover_zeile: None,
            pressed: None,
            scroll: alt.map_or(0.0, |v| v.scroll),
            frage: false,
            meldung: None,
        });
        self.hover = None;
    }

    /// Je geändertem Satz die Zeilen alt → neu: die Felder, in denen der
    /// Entwurf vom freigegebenen Stand abweicht.
    fn vorschau_zeilen(&self) -> Vec<VZeile> {
        let Some(fg) = &self.freigabe else {
            return Vec::new();
        };
        let k = &self.jetzt;
        let mut out = Vec::new();
        for s in &fg.saetze {
            let g = Guid::from_ifc(&s.kennung);
            let name = match s.abschnitt {
                "catalog" => "Verwaltungskennwort".to_string(),
                a => {
                    // Ein entfernter Satz steht nur noch im freigegebenen
                    let da = g.is_some_and(|g| {
                        k.artikel(g).is_some() || k.leistung(g).is_some() || k.los(g).is_some()
                    });
                    let kat = if da || a == "rate" { k } else { &fg.kat };
                    sk_cost::verwaltung::satz_name(kat, a, &s.kennung)
                }
            };
            let felder = sk_cost::verwaltung::entwurf_felder(&fg.lib, &self.lib, s);
            // Quelle und Preisstand gehören zur Herkunft, solange sich mehr
            // geändert hat
            let herkunftsfeld = |x: &str| matches!(x, "date" | "source" | "supplier");
            let nur_herkunft = felder.iter().all(|f| herkunftsfeld(&f.0));
            let herkunft = self.vorschau_herkunft(s, &felder);
            for (key, alt, neu) in felder {
                if herkunftsfeld(&key) && !nur_herkunft {
                    continue;
                }
                let (art, alt, neu) = match key.as_str() {
                    "+" => (wort_art(s.abschnitt, ""), String::new(), "neu".to_string()),
                    "-" => (
                        wort_art(s.abschnitt, ""),
                        String::new(),
                        "entfernt".to_string(),
                    ),
                    "svcpart" => (
                        "Stoffanteile".to_string(),
                        String::new(),
                        "geändert".to_string(),
                    ),
                    "svcfollow" => (
                        "Folgepositionen".to_string(),
                        String::new(),
                        "geändert".to_string(),
                    ),
                    "pw" => (
                        "Kennwort".to_string(),
                        String::new(),
                        match neu {
                            Some(_) if alt.is_some() => "geändert",
                            Some(_) => "gesetzt",
                            None => "entfernt",
                        }
                        .to_string(),
                    ),
                    "retired" => (
                        wort_art(s.abschnitt, ""),
                        String::new(),
                        if neu.is_some() {
                            "in den Papierkorb"
                        } else {
                            "wiederhergestellt"
                        }
                        .to_string(),
                    ),
                    _ => {
                        let wert = |v: Option<String>| {
                            v.map_or(String::new(), |v| self.vorschau_wert(s, &key, &v))
                        };
                        (wort_art(s.abschnitt, &key), wert(alt), wert(neu))
                    }
                };
                out.push(VZeile {
                    satz: s.clone(),
                    art,
                    name: name.clone(),
                    alt,
                    neu,
                    herkunft: herkunft.clone(),
                });
            }
        }
        out
    }

    /// Herkunft einer Änderung: Quelle und Preisstand aus dem Satz
    /// („Händler 10/2026“), sonst die Art der Herkunftszeile („manuell“).
    fn vorschau_herkunft(
        &self,
        s: &SatzId,
        felder: &[(String, Option<String>, Option<String>)],
    ) -> String {
        if s.abschnitt == "catalog" {
            return String::new();
        }
        let feld = |n: &str| {
            felder
                .iter()
                .find(|f| f.0 == n)
                .and_then(|f| f.2.as_deref())
                .map(|v| v.trim_matches('"').to_string())
        };
        if let Some(q) = feld("source") {
            return match feld("date")
                .and_then(|d| d.split_once('-').map(|(j, m)| format!("{m}/{j}")))
            {
                Some(d) => format!("{q} {d}"),
                None => q,
            };
        }
        let u = self.jetzt.herkunft_von(s.abschnitt, &s.kennung);
        match u.map(|u| u.kind.as_str()) {
            Some("import") => "importiert".into(),
            Some("ai") => "Vorschlag".into(),
            _ => "manuell".into(),
        }
    }

    /// Ein Wert der Vorschau mit Einheit: Geld mit zwei Stellen („29,40
    /// €/m²“), Zeit „0,50 h/m²“, sonst wie in der Datei mit Komma.
    fn vorschau_wert(&self, s: &SatzId, key: &str, v: &str) -> String {
        let v = v.trim_matches('"');
        let Some(d) = Dez::lesen(v, 6) else {
            return v.to_string();
        };
        let g = Guid::from_ifc(&s.kennung);
        let k = &self.jetzt;
        let einheit = |e: Option<Einheit>| e.map_or("", |e| e.zeichen());
        match (s.abschnitt, key) {
            ("article", "price") => {
                let e = einheit(g.and_then(|g| k.artikel(g)).map(|a| a.einheit));
                format!("{} €/{e}", geld(d))
            }
            ("service", "hours") => {
                let e = einheit(g.and_then(|g| k.leistung(g)).map(|l| l.einheit));
                format!("{} h/{e}", geld(d))
            }
            ("service", "equip" | "other" | "nu") => {
                let e = einheit(g.and_then(|g| k.leistung(g)).map(|l| l.einheit));
                format!("{} €/{e}", geld(d))
            }
            ("rate", _) => match s.kennung.as_str() {
                "wage" => format!("{} €/h", geld(d)),
                "surcharge" | "vat" => format!("{} %", geld(d)),
                _ => komma(d),
            },
            _ => komma(d),
        }
    }

    /// Was die Freigabe sperrt: neue Fehler im Entwurf (Regeln 71–92), neue
    /// Lücken im Standardhaus (Regel 97) und offene Eingaben.
    fn vorschau_befunde(&self) -> Vec<String> {
        let Some(fg) = &self.freigabe else {
            return Vec::new();
        };
        let mut b: Vec<String> = self
            .jetzt
            .befunde
            .iter()
            .filter(|x| x.schwere == sk_cost::Schwere::Fehler && !fg.kat.befunde.contains(x))
            .map(|x| x.satz.clone())
            .collect();
        let alt: Vec<String> = self
            .wirkung
            .luecken(&fg.kat)
            .into_iter()
            .map(|x| x.satz)
            .collect();
        b.extend(
            self.wirkung
                .luecken(&self.jetzt)
                .into_iter()
                .filter(|x| !alt.contains(&x.satz))
                .map(|x| x.satz),
        );
        if !self.ops.is_empty() {
            let grund = self
                .befunde
                .first()
                .map_or(String::new(), |x| format!(": {}", x.satz));
            b.insert(0, format!("Eine Eingabe ist noch nicht im Entwurf{grund}"));
        }
        b.dedup();
        b
    }

    /// Referenzhäuser mit Stand und Entwurf, dazu der Satz, welche Änderung
    /// am meisten ausmacht.
    fn vorschau_haeuser(&mut self, zeilen: &[VZeile]) -> (Vec<VHaus>, Option<String>) {
        let Some(fg) = &self.freigabe else {
            return (Vec::new(), None);
        };
        let (alt, neu) = (fg.lib.clone(), self.lib.clone());
        let firma_text = fg.text.clone();
        let saetze = fg.saetze.clone();
        let haeuser: Vec<VHaus> = self
            .wirkung
            .vergleich(&alt, &neu)
            .into_iter()
            .map(|(name, stand, entwurf, flaeche)| VHaus {
                name,
                stand,
                entwurf,
                flaeche,
            })
            .collect();
        let satz = self.vorschau_satz(&firma_text, &saetze, haeuser.first(), zeilen);
        // Die Wirkzeile wieder mit den gesammelten Eingaben
        self.wirkung.rechnen(&self.lib, &self.saetze);
        (haeuser, satz)
    }

    /// „Den größten Teil macht der Verrechnungslohn aus (+3,3 % im
    /// Standardhaus).“: je Änderung die Summe des ersten Referenzhauses ohne
    /// sie. Nur ohne offene Eingaben und bis 30 Änderungen.
    fn vorschau_satz(
        &mut self,
        firma: &str,
        saetze: &[SatzId],
        haus: Option<&VHaus>,
        zeilen: &[VZeile],
    ) -> Option<String> {
        let haus = haus?;
        if !self.ops.is_empty() || saetze.is_empty() || saetze.len() > 30 {
            return None;
        }
        if haus.stand == haus.entwurf {
            return Some(format!(
                "Keine dieser Änderungen ändert die Summe von {}.",
                haus.name
            ));
        }
        let mut best: Option<(&SatzId, i64)> = None;
        for s in saetze {
            let Ok(t) = sk_cost::verwaltung::entwurf_ohne(firma, &self.basis, s) else {
                continue;
            };
            let Ok(l) = sk_model::read_szk_with(&t, &sk_cost::lesen::ABSCHNITTE_SZK) else {
                continue;
            };
            let ohne = self
                .wirkung
                .erstes(&sk_cost::verwaltung::wie_freigegeben(&l))?;
            let d = haus.entwurf.0 - ohne.0;
            if best.is_none_or(|(_, b)| d.abs() > b.abs()) {
                best = Some((s, d));
            }
        }
        let (s, d) = best?;
        if d == 0 {
            return None;
        }
        let name = zeilen
            .iter()
            .find(|z| z.satz == *s)
            .map_or_else(String::new, |z| z.name.clone());
        let p = wirkung::prozent(haus.stand, Cent(haus.stand.0 + d))?;
        Some(if saetze.len() == 1 {
            format!("Die Änderung an „{name}“ macht {p} im {} aus.", haus.name)
        } else {
            format!(
                "Den größten Teil macht „{name}“ aus ({p} im {}).",
                haus.name
            )
        })
    }

    fn v_rect(&self, w: &Win) -> Rect {
        let (ww, hh) = self.dip(w);
        let h = V_H.min(hh - 40.0);
        let vw = V_W.min(ww - 40.0);
        self.r(w, (ww - vw) * 0.5, (hh - h) * 0.5 + 10.0, vw, h)
    }

    /// Knöpfe im Fuß des Blatts (px).
    fn v_knoepfe(&self, w: &Win) -> Vec<(VZiel, Rect, &'static str, bool)> {
        let Some(v) = &self.vorschau else {
            return Vec::new();
        };
        let r = self.v_rect(w);
        let s = w.scale;
        let y = r.y + r.h - (V_FUSS - 13.0) * s;
        let rechts = r.x + r.w - PAD * s;
        if v.frage {
            let b2 = Rect {
                x: rechts - 110.0 * s,
                y,
                w: 110.0 * s,
                h: 32.0 * s,
            };
            let b1 = Rect {
                x: b2.x - 10.0 * s - 100.0 * s,
                y,
                w: 100.0 * s,
                h: 32.0 * s,
            };
            return vec![
                (VZiel::Zurueck, b1, "Zurück", false),
                (VZiel::Verwerfen, b2, "Verwerfen", false),
            ];
        }
        let fw = 190.0 * s;
        let b2 = Rect {
            x: rechts - fw,
            y,
            w: fw,
            h: 32.0 * s,
        };
        let b1 = Rect {
            x: b2.x - 10.0 * s - 170.0 * s,
            y,
            w: 170.0 * s,
            h: 32.0 * s,
        };
        vec![
            (VZiel::Weg, b1, "Entwurf verwerfen …", false),
            (VZiel::Freigeben, b2, "", true),
        ]
    }

    fn v_schliessen_rect(&self, w: &Win) -> Rect {
        let r = self.v_rect(w);
        let s = w.scale;
        Rect {
            x: r.x + r.w - 44.0 * s,
            y: r.y + 12.0 * s,
            w: 30.0 * s,
            h: 30.0 * s,
        }
    }

    /// Oberkante der Zeile `i` der Änderungen (px, mit Bildlauf).
    fn v_zeile_y(&self, w: &Win, i: usize) -> f32 {
        let r = self.v_rect(w);
        let scroll = self.vorschau.as_ref().map_or(0.0, |v| v.scroll);
        r.y + (V_KOPF + 44.0 + i as f32 * ZEILE_H - scroll) * w.scale
    }

    /// Fläche „Änderung verwerfen“ an Zeile `i` (px).
    fn v_verwerfen_rect(&self, w: &Win, fonts: &Fonts, i: usize) -> Rect {
        let r = self.v_rect(w);
        let s = w.scale;
        let tw = fonts
            .regular
            .as_ref()
            .map_or(120.0 * s, |f| f.width("Änderung verwerfen", 11.5 * s));
        Rect {
            x: r.x + r.w - PAD * s - tw - 8.0 * s,
            y: self.v_zeile_y(w, i) + 3.0 * s,
            w: tw + 8.0 * s,
            h: ZEILE_H * s - 6.0 * s,
        }
    }

    /// Höhe des Inhalts zwischen Kopf und Fuß (dip).
    fn v_inhalt_h(&self) -> f32 {
        let Some(v) = &self.vorschau else {
            return 0.0;
        };
        44.0 + v.zeilen.len() as f32 * ZEILE_H
            + v.befunde.len() as f32 * 22.0
            + 30.0
            + 72.0
            + v.haeuser.len() as f32 * HAUS_H
            + 60.0
    }

    fn v_hit(&self, w: &Win, fonts: &Fonts, x: f64, y: f64) -> (Option<VZiel>, Option<usize>) {
        if self.v_schliessen_rect(w).contains(x, y) {
            return (Some(VZiel::Schliessen), None);
        }
        for (z, r, _, _) in self.v_knoepfe(w) {
            if r.contains(x, y) {
                return (Some(z), None);
            }
        }
        let r = self.v_rect(w);
        let s = w.scale;
        let oben = r.y + V_KOPF * s;
        let unten = r.y + r.h - V_FUSS * s;
        let (yf, xf) = (y as f32, x as f32);
        if yf < oben || yf > unten || xf < r.x || xf > r.x + r.w {
            return (None, None);
        }
        let Some(v) = &self.vorschau else {
            return (None, None);
        };
        if v.frage {
            return (None, None);
        }
        for i in 0..v.zeilen.len() {
            let zy = self.v_zeile_y(w, i);
            if yf >= zy && yf < zy + ZEILE_H * s {
                let z = self
                    .v_verwerfen_rect(w, fonts, i)
                    .contains(x, y)
                    .then_some(VZiel::Zeile(i));
                return (z, Some(i));
            }
        }
        (None, None)
    }

    /// Freigeben gesperrt?
    fn v_gesperrt(&self) -> bool {
        self.vorschau
            .as_ref()
            .is_none_or(|v| !v.befunde.is_empty() || v.zeilen.is_empty())
    }

    pub(super) fn vorschau_handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out {
            repaint: true,
            ..Default::default()
        };
        match *e {
            Event::MouseMove { x, y, .. } => {
                let (z, zeile) = self.v_hit(&cx.win, cx.fonts, x, y);
                if let Some(v) = self.vorschau.as_mut() {
                    out.repaint = v.hover != z || v.hover_zeile != zeile;
                    v.hover = z;
                    v.hover_zeile = zeile;
                }
            }
            Event::MouseLeave => {
                if let Some(v) = self.vorschau.as_mut() {
                    v.hover = None;
                    v.hover_zeile = None;
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let (z, _) = self.v_hit(&cx.win, cx.fonts, x, y);
                let gesperrt = z == Some(VZiel::Freigeben) && self.v_gesperrt();
                if let Some(v) = self.vorschau.as_mut() {
                    v.pressed = z.filter(|_| !gesperrt);
                    if z.is_none() && v.frage {
                        v.frage = false;
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let (z, _) = self.v_hit(&cx.win, cx.fonts, x, y);
                let p = self.vorschau.as_mut().and_then(|v| v.pressed.take());
                if p.is_some() && p == z {
                    self.v_klick(p, &mut out);
                }
            }
            Event::Wheel { delta, .. } => {
                let max = (self.v_inhalt_h()
                    - (self.v_rect(&cx.win).h / cx.win.scale - V_KOPF - V_FUSS))
                    .max(0.0);
                if let Some(v) = self.vorschau.as_mut() {
                    v.scroll = (v.scroll - delta as f32 * 3.0 * ZEILE_H).clamp(0.0, max);
                }
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => match self.vorschau.as_mut() {
                Some(v) if v.frage => v.frage = false,
                _ => self.vorschau = None,
            },
            _ => out.repaint = false,
        }
        out
    }

    fn v_klick(&mut self, z: Option<VZiel>, out: &mut Out) {
        let Some(v) = self.vorschau.as_mut() else {
            return;
        };
        v.meldung = None;
        match z {
            Some(VZiel::Schliessen) => self.vorschau = None,
            Some(VZiel::Freigeben) => out.freigeben = true,
            Some(VZiel::Weg) => v.frage = true,
            Some(VZiel::Zurueck) => v.frage = false,
            Some(VZiel::Verwerfen) => out.entwurf_verwerfen = true,
            Some(VZiel::Zeile(i)) => out.satz_verwerfen = v.zeilen.get(i).map(|z| z.satz.clone()),
            None => {}
        }
    }

    pub(super) fn vorschau_malen(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some(v) = &self.vorschau else {
            return;
        };
        let Some(fg) = &self.freigabe else {
            return;
        };
        let (s, u) = (w.scale, &t.ui);
        let f = self.frame(w);
        c.fill_rect(f.x, f.y, f.w, f.h, sk_paint::Rgba(0, 0, 0, 110));
        let r = self.v_rect(w);
        widgets::panel(c, r, s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let px = 13.0 * s;
        let klein = 11.5 * s;
        let x0 = r.x + PAD * s;
        let rechts = r.x + r.w - PAD * s;
        let breite = |font: Option<&sk_paint::font::Font>, t: &str, p: f32| {
            font.map_or(0.0, |f| f.width(t, p))
        };
        let rechtsbuendig =
            |c: &mut Canvas, font: Option<&sk_paint::font::Font>, t: &str, p, xr: f32, y, col| {
                let tw = breite(font, t, p);
                label(c, font, t, p, xr - tw, y, col);
                xr - tw
            };
        let line = s.round().max(1.0);
        // Inhalt (scrollt), Kopf und Fuß decken ab
        let oben = r.y + V_KOPF * s;
        let unten = r.y + r.h - V_FUSS * s;
        let sichtbar = |y: f32| y > oben + 4.0 * s && y < unten + 2.0 * s;
        let mut y = r.y + (V_KOPF + 30.0 - v.scroll) * s;
        if sichtbar(y) {
            label(c, bold, "ÄNDERUNGEN", 11.0 * s, x0, y, u.text_dim);
        }
        for (i, z) in v.zeilen.iter().enumerate() {
            let base = self.v_zeile_y(w, i) + 20.0 * s;
            if !sichtbar(base) {
                continue;
            }
            if v.hover_zeile == Some(i) {
                let zy = self.v_zeile_y(w, i);
                rounded(
                    c,
                    Rect {
                        x: x0 - 8.0 * s,
                        y: zy,
                        w: rechts - x0 + 16.0 * s,
                        h: ZEILE_H * s,
                    },
                    6.0 * s,
                    u.hover,
                );
            }
            let art = widgets::ellipsize(regular, &z.art, px, (NAME_X - 12.0) * s);
            label(c, regular, &art, px, x0, base, u.text_dim);
            let name_rand = r.x + (ALT_R - 120.0) * s;
            let name = widgets::ellipsize(regular, &z.name, px, name_rand - (r.x + NAME_X * s));
            label(c, regular, &name, px, r.x + NAME_X * s, base, u.text);
            if !z.alt.is_empty() {
                let alt = widgets::ellipsize(regular, &z.alt, px, 110.0 * s);
                let xa =
                    rechtsbuendig(c, regular, &alt, px, r.x + ALT_R * s, base, u.text_disabled);
                c.fill_rect(
                    xa,
                    (base - 4.5 * s).round(),
                    r.x + ALT_R * s - xa,
                    line,
                    u.text_disabled,
                );
                label(c, regular, "→", px, r.x + PFEIL_X * s, base, u.text_dim);
            }
            let neu = widgets::ellipsize(bold, &z.neu, px, (NEU_R - PFEIL_X - 24.0) * s);
            rechtsbuendig(c, bold, &neu, px, r.x + NEU_R * s, base, u.text);
            if v.hover_zeile == Some(i) {
                let vr = self.v_verwerfen_rect(w, fonts, i);
                if v.hover == Some(VZiel::Zeile(i)) {
                    rounded(c, vr, 5.0 * s, u.pressed);
                }
                rechtsbuendig(
                    c,
                    regular,
                    "Änderung verwerfen",
                    klein,
                    rechts - 4.0 * s,
                    base,
                    u.text,
                );
            } else {
                let hk = widgets::ellipsize(
                    regular,
                    &z.herkunft,
                    klein,
                    rechts - r.x - (NEU_R + 16.0) * s,
                );
                rechtsbuendig(c, regular, &hk, klein, rechts, base, u.text_dim);
            }
        }
        y = self.v_zeile_y(w, v.zeilen.len()) + 8.0 * s;
        for b in &v.befunde {
            if sichtbar(y + 14.0 * s) {
                let text = widgets::ellipsize(regular, b, klein, rechts - x0);
                label(c, regular, &text, klein, x0, y + 14.0 * s, u.field_invalid);
            }
            y += 22.0 * s;
        }
        y += 14.0 * s;
        if sichtbar(y) {
            c.fill_rect(x0, y, rechts - x0, line, u.border);
        }
        y += 34.0 * s;
        if sichtbar(y) {
            label(
                c,
                bold,
                "WIRKUNG AUF DIE REFERENZHÄUSER",
                11.0 * s,
                x0,
                y,
                u.text_dim,
            );
        }
        y += 26.0 * s;
        if sichtbar(y) {
            let p = klein;
            rechtsbuendig(
                c,
                regular,
                &format!("Stand {}", fg.stand),
                p,
                r.x + STAND_R * s,
                y,
                u.text_dim,
            );
            rechtsbuendig(c, regular, "Entwurf", p, r.x + ENTWURF_R * s, y, u.text_dim);
            rechtsbuendig(c, regular, "Änderung", p, r.x + AEND_R * s, y, u.text_dim);
            rechtsbuendig(
                c,
                regular,
                "außen an der tragenden Wand",
                p,
                rechts,
                y,
                u.text_dim,
            );
            rechtsbuendig(
                c,
                regular,
                "€/m² Grundfläche",
                p,
                rechts,
                y - 14.0 * s,
                u.text_dim,
            );
        }
        y += 12.0 * s;
        for h in &v.haeuser {
            let base = y + 20.0 * s;
            if sichtbar(base) {
                let name = widgets::ellipsize(regular, &h.name, px, (STAND_R - 120.0) * s);
                label(c, regular, &name, px, x0, base, u.text);
                rechtsbuendig(
                    c,
                    regular,
                    &wirkung::euro_ganz(h.stand),
                    px,
                    r.x + STAND_R * s,
                    base,
                    u.text,
                );
                rechtsbuendig(
                    c,
                    bold,
                    &wirkung::euro_ganz(h.entwurf),
                    px,
                    r.x + ENTWURF_R * s,
                    base,
                    u.text,
                );
                let p = wirkung::prozent(h.stand, h.entwurf).unwrap_or_default();
                rechtsbuendig(c, bold, &p, px, r.x + AEND_R * s, base, u.accent);
                if let (Some(a), Some(b)) = (je_m2(h.stand, h.flaeche), je_m2(h.entwurf, h.flaeche))
                {
                    rechtsbuendig(
                        c,
                        regular,
                        &format!("{a} → {b}"),
                        px,
                        rechts,
                        base,
                        u.text_dim,
                    );
                }
                // Balken: Änderung, voll ab 5 %
                let by = (base + 10.0 * s).round();
                let bw = r.x + AEND_R * s - x0;
                c.fill_rect(x0, by, bw, 4.0 * s, u.border);
                if h.stand.0 != 0 {
                    let pm = ((h.entwurf.0 - h.stand.0).abs() as f64 * 1000.0
                        / h.stand.0.abs() as f64)
                        .min(BALKEN_VOLL);
                    let fw = bw * (pm / BALKEN_VOLL) as f32;
                    c.fill_rect(x0, by, fw, 4.0 * s, u.accent);
                }
            }
            y += HAUS_H * s;
        }
        if let Some(satz) = &v.satz {
            y += 12.0 * s;
            if sichtbar(y) {
                let text = widgets::ellipsize(regular, satz, klein, rechts - x0);
                label(c, regular, &text, klein, x0, y, u.text_dim);
            }
        }
        // Kopf
        c.fill_rect(
            r.x + s,
            r.y + 12.0 * s,
            r.w - 2.0 * s,
            (V_KOPF - 12.0) * s,
            u.bg,
        );
        let titel = format!("Vorschau · Entwurf → Stand {}", fg.stand + 1);
        label(c, bold, &titel, 16.0 * s, x0, r.y + 36.0 * s, u.text);
        c.fill_rect(r.x, r.y + V_KOPF * s, r.w, line, u.border);
        let x = self.v_schliessen_rect(w);
        let col = if v.hover == Some(VZiel::Schliessen) {
            u.text
        } else {
            u.text_dim
        };
        let (mx, my, d) = (x.x + x.w * 0.5, x.y + x.h * 0.5, 4.5 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
        c.fill(&p, col);
        // Fuß
        c.fill_rect(r.x + s, unten, r.w - 2.0 * s, (V_FUSS - 12.0) * s, u.bg);
        c.fill_rect(r.x, unten, r.w, line, u.border);
        let knoepfe = self.v_knoepfe(w);
        let rand = knoepfe.first().map_or(rechts, |k| k.1.x - 16.0 * s);
        let fuss_y = unten + 26.0 * s;
        let (text, farbe) = match (&v.meldung, v.frage) {
            (Some(m), _) => (m.clone(), u.field_invalid),
            (None, true) => (
                format!(
                    "Den ganzen Entwurf verwerfen? Er kommt nach „firmenkatalog-staende“; freigegeben bleibt Stand {}.",
                    fg.stand
                ),
                u.text,
            ),
            (None, false) => (
                format!(
                    "Freigeben macht Stand {} für alle Plätze sichtbar. {FREIGEBEN_SATZ}",
                    fg.stand + 1
                ),
                u.text_dim,
            ),
        };
        let font = if v.frage { bold } else { regular };
        // Höchstens zwei Zeilen
        let zeilen = widgets::wrap(font, &text, klein, rand - x0);
        let y1 = if zeilen.len() > 1 {
            fuss_y - 8.0 * s
        } else {
            fuss_y + 1.0 * s
        };
        for (i, z) in zeilen.iter().take(2).enumerate() {
            let z = if i == 1 && zeilen.len() > 2 {
                widgets::ellipsize(
                    font,
                    &format!("{z} {}", zeilen[2..].join(" ")),
                    klein,
                    rand - x0,
                )
            } else {
                z.clone()
            };
            label(c, font, &z, klein, x0, y1 + i as f32 * 17.0 * s, farbe);
        }
        let freigeben = format!("Freigeben als Stand {}", fg.stand + 1);
        for (z, br, text, akzent) in knoepfe {
            let disabled = z == VZiel::Freigeben && self.v_gesperrt();
            let st = ButtonState {
                hover: v.hover == Some(z) && !disabled,
                pressed: v.pressed == Some(z),
                active: akzent && !disabled,
                disabled,
            };
            let text = if z == VZiel::Freigeben {
                freigeben.as_str()
            } else {
                text
            };
            widgets::button(c, fonts, br, text, st, s, t);
        }
    }
}

/// Art einer Zeile der Vorschau (soll-ka-3b: „Aufwandswert“, „Preis“,
/// „Firmenwert“).
fn wort_art(abschnitt: &str, key: &str) -> String {
    match (abschnitt, key) {
        ("rate", _) => "Firmenwert".into(),
        ("service", "hours") => "Aufwandswert".into(),
        ("service", "equip") => "Gerät".into(),
        ("service", "other") => "Sonstiges".into(),
        ("article", "price") => "Preis".into(),
        (a, "") => sk_cost::wort::abschnitt(a).into(),
        (a, k) => match sk_cost::wort::feld(Some(a), k) {
            "Angabe" => sk_cost::wort::abschnitt(a).into(),
            w => w.into(),
        },
    }
}

fn aenderungen_text(n: usize) -> String {
    match n {
        1 => "eine Änderung".to_string(),
        n => format!("{n} Änderungen"),
    }
}

fn pille_text(n: usize) -> String {
    match n {
        1 => "Entwurf · eine Änderung".to_string(),
        n => format!("Entwurf · {n} Änderungen"),
    }
}
