//! Geführter Ablauf als Blatt über der Verwaltung (KA-3b5, soll-ka-3d):
//! links die Seiten mit Haken und Antwort, rechts die Frage mit passender
//! Feldart, unten Abbrechen, Zurück und Weiter. Erst alle Fragen, dann ein
//! einziger Schreibvorgang mit dem letzten Knopf (Regel 104); Esc oder
//! Abbrechen lassen nichts zurück. Seiten, Antworten und Operationen
//! bildet `sk_cost::ablauf`, hier ist nur das Fenster.

use super::*;
use sk_cost::ablauf::{self, Ablauf, Antworten, Art, Feldart, Zugang};

/// Blatt (dip).
pub(super) const A_W: f32 = 820.0;
pub(super) const A_H: f32 = 540.0;
const KOPF: f32 = 60.0;
const FUSS: f32 = 66.0;
const SPALTE: f32 = 250.0;
const SCHRITT_H: f32 = 54.0;
/// Rechte Seite: linker Rand.
const RX: f32 = SPALTE + 28.0;
/// Zeilen der Auswahlliste.
const OPT_Y: f32 = 184.0;
const OPT_H: f32 = 28.0;
const OPT_N: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AZiel {
    Schliessen,
    Abbrechen,
    Zurueck,
    Weiter,
    /// Feld Nummer n der Seite.
    Feld(usize),
    /// Preiseinheit n (Regel 108).
    Einheit(usize),
    /// Zeile n der Auswahl (ab der obersten sichtbaren).
    Option(usize),
    /// Seite n in der Liste links (nur erreichte).
    Seite(usize),
}

pub(super) struct Assistent {
    pub a: Ablauf,
    pub seiten: Vec<Vec<usize>>,
    pub seite: usize,
    /// Höchste erreichte Seite: bis dahin springt ein Klick links zurück.
    pub weit: usize,
    /// Eingabe je Schritt (bei `pick:` der Suchtext).
    pub te: Vec<TextEdit>,
    /// Gewählte Preiseinheit je Schritt (Index in `per`).
    pub per: Vec<usize>,
    /// Gewählter Eintrag je Schritt: Guid und Name.
    pub wahl: Vec<Option<(Guid, String)>>,
    /// Zuletzt eingesetzte Vorbelegung je Schritt: Hat der Nutzer sie nicht
    /// geändert, folgt sie geänderten früheren Antworten.
    pub preset: Vec<Option<String>>,
    pub antworten: Antworten,
    /// Feld mit der Schreibmarke (Nummer auf der Seite).
    pub fokus: usize,
    /// Fehler: Schritt (Index) und Satz; `usize::MAX`: beim Schreiben.
    pub fehler: Option<(usize, String)>,
    /// Oberste sichtbare Zeile der Auswahl.
    pub oben: usize,
    pub hover: Option<AZiel>,
    pub pressed: Option<AZiel>,
}

/// Fertiger Ablauf für dieses Haus (Reiter Kosten): die App schreibt die
/// Operationen als einen Rückgängig-Schritt mit dem Namen des Ablaufs.
#[derive(Clone, Debug, PartialEq)]
pub struct FuerHaus {
    pub name: String,
    pub ops: Vec<Op>,
    pub schluss: String,
    /// Regeln der `check`-Schritte: ihre Befunde sperren auch im Haus
    /// (BIM §3.15 `rule`, Review 3ax).
    pub regeln: Vec<u16>,
}

/// „Baustoff“ aus `baustoff`.
fn gross(key: &str) -> String {
    let k = key.replace('_', " ");
    let mut c = k.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Knopf auf der letzten Seite nach der ersten Operation.
fn verb(a: &Ablauf) -> &'static str {
    match a
        .schritte
        .iter()
        .find(|s| s.art == Art::Operation)
        .map(|s| s.op.as_str())
    {
        Some("artikel_anlegen") => "Anlegen",
        Some("preis_setzen") => "Eintragen",
        _ => "Setzen",
    }
}

/// „MM/JJJJ“ für neue Preise.
fn stand_heute() -> String {
    let d = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Manual).datum;
    let mut t = d.split('-');
    match (t.next(), t.next()) {
        (Some(j), Some(m)) => format!("{m}/{j}"),
        _ => String::new(),
    }
}

impl Verwaltung {
    /// Abläufe für die Verwaltung: `kind=admin`, nicht ausgemustert.
    pub(super) fn verwaltungs_ablaeufe(&self) -> impl Iterator<Item = &Ablauf> {
        self.ablaeufe
            .iter()
            .filter(|a| a.zugang == Zugang::Admin && !a.retired)
    }

    pub(super) fn ablauf(&self, g: Guid) -> Option<&Ablauf> {
        self.ablaeufe.iter().find(|a| a.guid == g)
    }

    /// Name einer Seite: der Knopf bei der Abschlussseite mit mehreren
    /// Textfeldern („Anlegen“), sonst der Name der Frage („Preis je m²“).
    /// Die Einheit folgt dem gewählten Artikel („Preis je kg“).
    pub(super) fn seiten_name(
        &self,
        a: &Ablauf,
        seiten: &[Vec<usize>],
        i: usize,
        antworten: &Antworten,
    ) -> String {
        let seite = &seiten[i];
        if seite.len() > 1 {
            return verb(a).to_string();
        }
        let s = &a.schritte[seite[0]];
        match (&s.typ, ablauf::grundeinheit(s, antworten, &self.jetzt)) {
            (Some(Feldart::Geld), Some(e)) => format!("{} je {}", gross(&s.key), e.zeichen()),
            _ => gross(&s.key),
        }
    }

    /// Abläufe für den Reiter Kosten: `kind=user`, gültig, nicht
    /// ausgemustert (paket-ka3b §3).
    pub fn haus_ablaeufe(&self) -> Vec<(Guid, String)> {
        self.ablaeufe
            .iter()
            .filter(|a| a.zugang == Zugang::User && !a.retired && a.befund.is_none())
            .map(|a| (a.guid, a.name.clone()))
            .collect()
    }

    /// Ablauf `kind=user` aus dem Reiter Kosten: nur das Blatt, ohne die
    /// Verwaltung dahinter und ohne Kennwort; „Eintragen“ gibt die
    /// Operationen an die App ([`Out::haus`]). `false`: kein solcher Ablauf.
    pub fn ablauf_im_haus(&mut self, g: Guid) -> bool {
        if !self.haus_ablaeufe().iter().any(|(x, _)| *x == g) {
            return false;
        }
        self.projekt = true;
        self.abfrage = None;
        // Auswahl, Einheiten und Vergleich aus dem Katalog dieses Hauses:
        // mit seinen eigenen Artikeln und Preisen, auf dem freigegebenen
        // Firmenstand (Review 3ax)
        let firma = self.freigabe.as_ref().map_or(&self.lib0, |f| &f.lib);
        self.jetzt = sk_cost::lesen::katalog(&self.m, Some(firma));
        self.ablauf_starten(g);
        self.assistent.is_some()
    }

    /// Schreiben ins Haus gescheitert: das Blatt bleibt mit dem Grund offen.
    pub fn ablauf_fehler(&mut self, e: String) {
        if let Some(x) = self.assistent.as_mut() {
            x.fehler = Some((usize::MAX, e));
        }
    }

    /// Startet den Ablauf `g` als Blatt; ein ungültiger startet nicht.
    pub(super) fn ablauf_starten(&mut self, g: Guid) {
        let Some(a) = self.ablauf(g).filter(|a| a.befund.is_none()).cloned() else {
            return;
        };
        self.ende_edit(true);
        self.schluss = None;
        let n = a.schritte.len();
        let seiten = ablauf::seiten(&a);
        self.assistent = Some(Assistent {
            a,
            seiten,
            seite: 0,
            weit: 0,
            te: (0..n).map(|_| TextEdit::new("")).collect(),
            per: vec![0; n],
            wahl: vec![None; n],
            preset: vec![None; n],
            antworten: Antworten::new(),
            fokus: 0,
            fehler: None,
            oben: 0,
            hover: None,
            pressed: None,
        });
        self.seite_oeffnen(0);
    }

    /// Seite `i` zeigen: Vorbelegung einsetzen, wo der Nutzer sie nicht
    /// geändert hat.
    pub(super) fn seite_oeffnen(&mut self, i: usize) {
        let Some(x) = self.assistent.as_mut() else {
            return;
        };
        x.seite = i;
        x.weit = x.weit.max(i);
        x.fokus = 0;
        x.fehler = None;
        x.oben = 0;
        for &j in &x.seiten[i] {
            let s = &x.a.schritte[j];
            if s.preset.is_empty() {
                continue;
            }
            let neu = ablauf::einsetzen(&s.preset, &x.antworten);
            let unberuehrt = match &x.preset[j] {
                Some(p) => x.te[j].text == *p,
                None => x.te[j].text.is_empty(),
            };
            if unberuehrt {
                x.te[j] = TextEdit::new(&neu);
                x.preset[j] = Some(neu);
            }
        }
    }

    /// Auswahl für `pick:<abschnitt>`, nach Namen, gefiltert mit `such`.
    fn optionen(&self, abschnitt: &str, such: &str) -> Vec<(Guid, String)> {
        let k = &self.jetzt;
        let mut v: Vec<(Guid, String)> = match abschnitt {
            "material" => self
                .m
                .materials()
                .iter()
                .map(|(_, m)| (m.guid, m.name.clone()))
                .collect(),
            "article" => k
                .artikel
                .iter()
                .filter(|a| !a.retired)
                .map(|a| (a.guid, a.name.clone()))
                .collect(),
            "service" => k
                .leistungen
                .iter()
                .filter(|l| !l.retired)
                .map(|l| (l.guid, l.kurz.clone()))
                .collect(),
            "lot" => k
                .lose
                .iter()
                .filter(|l| !l.retired)
                .map(|l| (l.guid, format!("{} {}", l.nr, l.name)))
                .collect(),
            "layerset" => self.typen(),
            "trade" => self
                .lib
                .trades
                .iter()
                .map(|t| (t.guid, format!("{} {}", t.code, t.name)))
                .collect(),
            _ => Vec::new(),
        };
        v.sort_by(|a, b| a.1.cmp(&b.1));
        v.dedup_by(|a, b| a.0 == b.0);
        let q = such.trim().to_lowercase();
        if !q.is_empty() {
            v.retain(|(_, n)| n.to_lowercase().contains(&q));
        }
        v
    }

    /// „Weiter“: prüft alle Felder der Seite (Pflicht, Zahl, Grenzen) und
    /// geht weiter; auf der letzten Seite schreibt es.
    pub(super) fn ablauf_weiter(&mut self) {
        let Some(x) = self.assistent.as_ref() else {
            return;
        };
        let seite = x.seiten[x.seite].clone();
        let mut antworten = x.antworten.clone();
        for (pos, &j) in seite.iter().enumerate() {
            let s = &x.a.schritte[j];
            let (text, name) = match (&s.typ, &x.wahl[j]) {
                (Some(Feldart::Wahl(_)), Some((g, n))) => (g.to_ifc(), n.clone()),
                (Some(Feldart::Wahl(_)), None) => (String::new(), String::new()),
                _ => (x.te[j].text.clone(), String::new()),
            };
            match ablauf::antwort(s, &text, &name, x.per[j], &antworten, &self.jetzt) {
                Ok(a) => {
                    antworten.insert(s.key.clone(), a);
                }
                Err(e) => {
                    if let Some(x) = self.assistent.as_mut() {
                        x.fehler = Some((j, e));
                        x.fokus = pos;
                    }
                    return;
                }
            }
        }
        let letzte = x.seite + 1 == x.seiten.len();
        let naechste = x.seite + 1;
        if let Some(x) = self.assistent.as_mut() {
            x.antworten = antworten;
        }
        if letzte {
            self.ablauf_fertig();
        } else {
            self.seite_oeffnen(naechste);
        }
    }

    /// Ein Schreibvorgang (Regel 104): die Operationen zu den gesammelten
    /// dazu, vorher geprüft wie beim OK; `check`-Regeln sperren wie die
    /// Freigabe. Scheitert es, bleibt das Blatt mit dem Grund offen.
    pub(super) fn ablauf_fertig(&mut self) {
        let Some(x) = self.assistent.as_ref() else {
            return;
        };
        let fehler = |v: &mut Self, e: String| {
            if let Some(x) = v.assistent.as_mut() {
                x.fehler = Some((usize::MAX, e));
            }
        };
        let ops = match ablauf::ops(&x.a, &x.antworten, &stand_heute()) {
            Ok(o) => o,
            Err(e) => return fehler(self, format!("Nicht geschrieben: {e}.")),
        };
        let regeln: Vec<u16> =
            x.a.schritte
                .iter()
                .filter(|s| s.art == Art::Pruefung)
                .filter_map(|s| s.regel)
                .collect();
        let schluss = ablauf::schluss(&x.a, &x.antworten);
        if self.projekt {
            // Ins Haus schreibt die App, geprüft wie jede Projektänderung
            self.fuer_haus = Some(FuerHaus {
                name: x.a.name.clone(),
                ops,
                schluss,
                regeln,
            });
            return;
        }
        let mut alle = self.ops.clone();
        alle.extend(ops.iter().cloned());
        let entwurf = self.freigabe.is_some();
        match sk_cost::verwaltung::mit_ops_in(&self.basis, &alle, entwurf) {
            Err(b) => {
                let satz = b.first().map_or(String::new(), |b| b.satz.clone());
                return fehler(self, satz);
            }
            Ok((lib, _)) if !regeln.is_empty() => {
                let lib = match entwurf {
                    true => sk_cost::verwaltung::wie_freigegeben(&lib),
                    false => lib,
                };
                let k = sk_cost::lesen::firma_oder_werk(&self.m, Some(&lib));
                if let Some(b) = k.befunde.iter().find(|b| regeln.contains(&b.regel)) {
                    return fehler(self, b.satz.clone());
                }
            }
            Ok(_) => {}
        }
        self.assistent = None;
        for op in ops.iter().cloned() {
            self.setzen(op);
        }
        // Das Ergebnis zeigen
        let ziel = ops.iter().find_map(|o| match o {
            Op::ArtikelAnlegen { name, .. } => self
                .jetzt
                .artikel
                .iter()
                .find(|a| a.name == *name && !a.retired)
                .map(|a| Knoten::ArtikelSatz(a.guid)),
            Op::PreisSetzen { artikel, .. } | Op::UmrechnungSetzen { artikel, .. } => {
                Some(Knoten::ArtikelSatz(*artikel))
            }
            Op::FirmenwertSetzen { .. } => Some(Knoten::Firmenwerte),
            _ => None,
        });
        if let Some(k) = ziel {
            self.waehlen(k);
        }
        self.neu_angelegt = ops.iter().find_map(|o| match o {
            Op::ArtikelAnlegen { name, .. } => Some(name.clone()),
            _ => None,
        });
        self.schluss = Some(schluss);
        self.meldung = self.schluss.clone();
    }

    /// Nach dem Schreiben den angelegten Artikel wieder wählen (seine Guid
    /// vergibt das Schreiben neu).
    pub(super) fn neu_angelegt_waehlen(&mut self) {
        let Some(name) = self.neu_angelegt.take() else {
            return;
        };
        let g = self
            .jetzt
            .artikel
            .iter()
            .find(|a| a.name == name && !a.retired)
            .map(|a| a.guid);
        if let Some(g) = g {
            self.waehlen(Knoten::ArtikelSatz(g));
        }
    }

    /// Antwort für Tests und Sprachsteuerung: tippt `text` ins Feld `pos`
    /// der offenen Seite.
    #[cfg(test)]
    pub(super) fn a_tippen(&mut self, pos: usize, text: &str) {
        if let Some(x) = self.assistent.as_mut() {
            let j = x.seiten[x.seite][pos];
            x.te[j] = TextEdit::new(text);
            // Wie getippt: Schreibmarke am Ende, nichts markiert
            x.te[j].end(false);
        }
    }

    pub(super) fn ablauf_zurueck(&mut self) {
        let Some(x) = self.assistent.as_ref() else {
            return;
        };
        if x.seite > 0 {
            let i = x.seite - 1;
            self.seite_oeffnen(i);
        }
    }

    // --- Lage ---------------------------------------------------------------

    fn a_rect(&self, w: &Win) -> Rect {
        let (ww, hh) = self.dip(w);
        if self.projekt {
            return self.r(w, 0.0, 0.0, ww, hh);
        }
        let (bw, bh) = (A_W.min(ww - 20.0), A_H.min(hh - 20.0));
        self.r(w, (ww - bw) * 0.5, (hh - bh) * 0.5, bw, bh)
    }

    /// Rechteck im Blatt (dip ab der linken oberen Ecke) in px.
    fn a_at(&self, w: &Win, x: f32, y: f32, ww: f32, hh: f32) -> Rect {
        let r = self.a_rect(w);
        let s = w.scale;
        Rect::new(
            (r.x + x * s).round(),
            (r.y + y * s).round(),
            (ww * s).round(),
            (hh * s).round(),
        )
    }

    /// Breite und Höhe des Blatts (dip).
    fn a_dip(&self, w: &Win) -> (f32, f32) {
        let r = self.a_rect(w);
        (r.w / w.scale, r.h / w.scale)
    }

    /// Wo die Felder der Seite stehen (dip): eine Frage oben mit großem
    /// Feld, die Abschlussseite als Zeilen mit Bezeichnung.
    fn a_feld(&self, w: &Win, pos: usize) -> Rect {
        let Some(x) = self.assistent.as_ref() else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let (bw, _) = self.a_dip(w);
        let seite = &x.seiten[x.seite];
        let j = seite[pos.min(seite.len() - 1)];
        let s = &x.a.schritte[j];
        if seite.len() > 1 {
            return self.a_at(
                w,
                RX + 110.0,
                150.0 + pos as f32 * 46.0,
                (bw - RX - 110.0 - 120.0).min(340.0),
                34.0,
            );
        }
        let breite = match s.typ {
            Some(Feldart::Wahl(_)) | Some(Feldart::Text) => (bw - RX - 28.0).min(420.0),
            _ => 190.0,
        };
        self.a_at(w, RX, 142.0, breite, 34.0)
    }

    fn a_teile(&self, w: &Win) -> Vec<(AZiel, Rect)> {
        let Some(x) = self.assistent.as_ref() else {
            return Vec::new();
        };
        let (bw, bh) = self.a_dip(w);
        let mut v = vec![
            (AZiel::Schliessen, self.a_at(w, bw - 44.0, 16.0, 28.0, 28.0)),
            (AZiel::Abbrechen, self.a_at(w, 22.0, bh - 50.0, 120.0, 34.0)),
            (
                AZiel::Weiter,
                self.a_at(w, bw - 166.0, bh - 50.0, 144.0, 34.0),
            ),
        ];
        if x.seite > 0 {
            v.push((
                AZiel::Zurueck,
                self.a_at(w, bw - 296.0, bh - 50.0, 120.0, 34.0),
            ));
        }
        for i in 0..=x.weit.min(x.seiten.len() - 1) {
            if i != x.seite {
                v.push((
                    AZiel::Seite(i),
                    self.a_at(
                        w,
                        10.0,
                        KOPF + 12.0 + i as f32 * SCHRITT_H,
                        SPALTE - 20.0,
                        SCHRITT_H - 6.0,
                    ),
                ));
            }
        }
        let seite = &x.seiten[x.seite];
        for pos in 0..seite.len() {
            v.push((AZiel::Feld(pos), self.a_feld(w, pos)));
        }
        if seite.len() == 1 {
            let j = seite[0];
            let s = &x.a.schritte[j];
            match &s.typ {
                Some(Feldart::Geld) if s.per.len() > 1 => {
                    let f = self.a_feld(w, 0);
                    let s0 = w.scale;
                    let sichtbar = ablauf::einheiten_sichtbar(s, &x.antworten, &self.jetzt);
                    for (i, &n) in sichtbar.iter().enumerate() {
                        v.push((
                            AZiel::Einheit(n),
                            Rect::new(
                                f.x + i as f32 * 76.0 * s0,
                                f.y + f.h + 10.0 * s0,
                                (72.0 * s0).round(),
                                (26.0 * s0).round(),
                            ),
                        ));
                    }
                }
                Some(Feldart::Wahl(a)) => {
                    let n = self.optionen(a, &x.te[j].text).len();
                    let breite = (bw - RX - 28.0).min(420.0);
                    for r in 0..n.saturating_sub(x.oben).min(OPT_N) {
                        v.push((
                            AZiel::Option(r),
                            self.a_at(w, RX, OPT_Y + r as f32 * OPT_H, breite, OPT_H - 2.0),
                        ));
                    }
                }
                _ => {}
            }
        }
        v
    }

    fn a_hit(&self, w: &Win, x: f64, y: f64) -> Option<AZiel> {
        self.a_teile(w)
            .into_iter()
            .find(|(_, r)| r.contains(x, y))
            .map(|(z, _)| z)
    }

    // --- Ereignisse -----------------------------------------------------------

    /// Schritt-Index des Feldes mit der Schreibmarke.
    fn a_fokus_schritt(&self) -> Option<usize> {
        let x = self.assistent.as_ref()?;
        x.seiten[x.seite].get(x.fokus).copied()
    }

    pub(super) fn assistent_handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out {
            repaint: true,
            ..Default::default()
        };
        match *e {
            Event::MouseMove { x, y, .. } => {
                let z = self.a_hit(&cx.win, x, y);
                if let Some(a) = self.assistent.as_mut() {
                    out.repaint = a.hover != z;
                    a.hover = z;
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let z = self.a_hit(&cx.win, x, y);
                self.a_klick(z, true);
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let z = self.a_hit(&cx.win, x, y);
                let p = self.assistent.as_mut().and_then(|a| a.pressed.take());
                if p.is_some() && p == z {
                    self.a_klick(z, false);
                }
            }
            Event::Wheel { delta, .. } => {
                let n = self.a_optionen_jetzt().len();
                if let Some(x) = self.assistent.as_mut() {
                    let schritt = if delta < 0.0 { 3 } else { 0 };
                    let zurueck = if delta > 0.0 { 3 } else { 0 };
                    let max = n.saturating_sub(OPT_N);
                    x.oben = (x.oben + schritt).saturating_sub(zurueck).min(max);
                }
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => self.a_taste(key, mods),
            Event::Text(c) if !c.is_control() => {
                if let (Some(j), Some(x)) = (self.a_fokus_schritt(), self.assistent.as_mut()) {
                    x.te[j].insert(&c.to_string());
                    x.fehler = None;
                    if matches!(x.a.schritte[j].typ, Some(Feldart::Wahl(_))) {
                        x.oben = 0;
                    }
                }
            }
            _ => out.repaint = false,
        }
        out
    }

    /// Optionen der Auswahl auf der offenen Seite.
    pub(super) fn a_optionen_jetzt(&self) -> Vec<(Guid, String)> {
        let Some(x) = self.assistent.as_ref() else {
            return Vec::new();
        };
        let seite = &x.seiten[x.seite];
        if seite.len() != 1 {
            return Vec::new();
        }
        let j = seite[0];
        match &x.a.schritte[j].typ {
            Some(Feldart::Wahl(a)) => self.optionen(a, &x.te[j].text),
            _ => Vec::new(),
        }
    }

    /// Klick: `unten` beim Drücken (Felder, Einheit, Auswahl wirken
    /// gleich), sonst beim Loslassen (Knöpfe).
    pub(super) fn a_klick(&mut self, z: Option<AZiel>, unten: bool) {
        let opts = self.a_optionen_jetzt();
        let k = self.jetzt.clone();
        let Some(x) = self.assistent.as_mut() else {
            return;
        };
        match (z, unten) {
            (Some(AZiel::Feld(p)), true) => x.fokus = p,
            (Some(AZiel::Einheit(n)), true) => {
                let j = x.seiten[x.seite][0];
                match ablauf::einheit_waehlbar(&x.a.schritte[j], n, &x.antworten, &k) {
                    Ok(()) => {
                        x.per[j] = n;
                        x.fehler = None;
                    }
                    Err(e) => x.fehler = Some((j, e)),
                }
            }
            (Some(AZiel::Option(r)), true) => {
                let j = x.seiten[x.seite][0];
                if let Some((g, n)) = opts.get(x.oben + r) {
                    x.wahl[j] = Some((*g, n.clone()));
                    x.fehler = None;
                }
            }
            (Some(z), true) => x.pressed = Some(z),
            (Some(AZiel::Weiter), false) => self.ablauf_weiter(),
            (Some(AZiel::Zurueck), false) => self.ablauf_zurueck(),
            (Some(AZiel::Seite(i)), false) => self.seite_oeffnen(i),
            (Some(AZiel::Abbrechen | AZiel::Schliessen), false) => self.assistent = None,
            _ => {}
        }
    }

    pub(super) fn a_taste(&mut self, key: Key, mods: Modifiers) {
        let n_opt = self.a_optionen_jetzt().len();
        let Some(x) = self.assistent.as_mut() else {
            return;
        };
        let felder = x.seiten[x.seite].len();
        let j = x.seiten[x.seite][x.fokus.min(felder - 1)];
        match key {
            Key::Escape => {
                self.assistent = None;
                return;
            }
            Key::Enter => {
                // Enter in der Auswahl nimmt den einzigen Treffer
                let wahl = matches!(x.a.schritte[j].typ, Some(Feldart::Wahl(_)));
                if wahl && x.wahl[j].is_none() && n_opt == 1 {
                    let opts = self.a_optionen_jetzt();
                    if let Some(x) = self.assistent.as_mut() {
                        x.wahl[j] = opts.first().cloned();
                    }
                }
                let letzt = self
                    .assistent
                    .as_ref()
                    .is_some_and(|x| x.fokus + 1 >= felder);
                if letzt {
                    self.ablauf_weiter();
                } else if let Some(x) = self.assistent.as_mut() {
                    x.fokus += 1;
                }
                return;
            }
            Key::Tab => {
                x.fokus = if mods.shift {
                    (x.fokus + felder - 1) % felder
                } else {
                    (x.fokus + 1) % felder
                };
                return;
            }
            _ => {}
        }
        let te = &mut x.te[j];
        let sel = mods.shift;
        match key {
            Key::Backspace => te.backspace(),
            Key::Delete => te.delete(),
            Key::Left => te.left(sel),
            Key::Right => te.right(sel),
            Key::Home => te.home(sel),
            Key::End => te.end(sel),
            Key::Char('A') if mods.ctrl => te.select_all(),
            _ => return,
        }
        x.fehler = None;
    }

    // --- Zeichnen -------------------------------------------------------------

    /// „in den Entwurf“ bzw. „in den Firmenkatalog“ (soll-ka-3d).
    fn a_ziel(&self) -> &'static str {
        if self.projekt {
            "für dieses Haus"
        } else if self.freigabe.is_some() {
            "in den Entwurf"
        } else {
            "in den Firmenkatalog"
        }
    }

    /// Antwort für die Liste links: „200 mm“, „25,30 €/m²“.
    fn a_antwort_text(&self, x: &Assistent, j: usize) -> String {
        let s = &x.a.schritte[j];
        let Some(a) = x.antworten.get(&s.key) else {
            return String::new();
        };
        if a.anzeige.is_empty() {
            return String::new();
        }
        match (&s.typ, ablauf::grundeinheit(s, &x.antworten, &self.jetzt)) {
            (Some(Feldart::Mm), _) => format!("{} mm", a.anzeige),
            (Some(Feldart::Stunden), _) => format!("{} h", a.anzeige),
            (Some(Feldart::Geld), Some(e)) => format!("{} €/{}", a.anzeige, e.zeichen()),
            (Some(Feldart::Geld), None) => format!("{} €", a.anzeige),
            _ => a.anzeige.clone(),
        }
    }

    pub(super) fn assistent_malen(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some(x) = self.assistent.as_ref() else {
            return;
        };
        let (s, u) = (w.scale, &t.ui);
        let r = self.a_rect(w);
        // Über der Verwaltung abgedunkelt; allein (Reiter Kosten) ist das
        // Blatt das Fenster
        if !self.projekt {
            let f = self.frame(w);
            c.fill_rect(f.x, f.y, f.w, f.h, sk_paint::Rgba(0, 0, 0, 110));
            widgets::panel(c, r, s, t);
        }
        let (bw, bh) = self.a_dip(w);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let at = |xx: f32, yy: f32| (r.x + xx * s, r.y + yy * s);
        let line = s.round().max(1.0);
        // Kopf: Name, Ziel, ×
        let (tx, ty) = at(24.0, 38.0);
        let titel = widgets::ellipsize(bold, &x.a.name, 16.0 * s, (bw - 260.0) * s);
        label(c, bold, &titel, 16.0 * s, tx, ty, u.text);
        let ziel = self.a_ziel();
        let zw = regular.map_or(0.0, |f| f.width(ziel, 12.0 * s));
        let (zx, zy) = at(bw - 56.0, 36.0);
        label(c, regular, ziel, 12.0 * s, zx - zw, zy, u.text_dim);
        let teile = self.a_teile(w);
        let rect = |z: AZiel| teile.iter().find(|(x, _)| *x == z).map(|(_, r)| *r);
        if let Some(xr) = rect(AZiel::Schliessen) {
            let col = if x.hover == Some(AZiel::Schliessen) {
                u.text
            } else {
                u.text_dim
            };
            let (mx, my, d) = (xr.x + xr.w * 0.5, xr.y + xr.h * 0.5, 4.5 * s);
            let mut p = Path::new();
            p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
            p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
            c.fill(&p, col);
        }
        c.fill_rect(r.x, r.y + KOPF * s, r.w, line, u.border);
        c.fill_rect(r.x, r.y + (bh - FUSS) * s, r.w, line, u.border);
        c.fill_rect(
            (r.x + SPALTE * s).round(),
            r.y + KOPF * s,
            line,
            (bh - KOPF - FUSS) * s,
            u.border,
        );
        // Links: die Seiten
        for i in 0..x.seiten.len() {
            let y0 = KOPF + 12.0 + i as f32 * SCHRITT_H;
            let jetzt = i == x.seite;
            let fertig = i < x.seite || (i <= x.weit && i != x.seite);
            let zr = self.a_at(w, 10.0, y0, SPALTE - 20.0, SCHRITT_H - 6.0);
            if jetzt {
                rounded(c, zr, 6.0 * s, u.pressed);
                c.fill_rect(
                    zr.x - 4.0 * s,
                    zr.y + 4.0 * s,
                    3.0 * s,
                    zr.h - 8.0 * s,
                    u.accent,
                );
            } else if x.hover == Some(AZiel::Seite(i)) {
                rounded(c, zr, 6.0 * s, u.hover);
            }
            let (cx0, cy0) = at(34.0, y0 + 18.0);
            let rad = 10.0 * s;
            let kreis = Rect::new(cx0 - rad, cy0 - rad, 2.0 * rad, 2.0 * rad);
            if jetzt {
                rounded(c, kreis, rad, u.accent);
            } else if fertig {
                // Erledigt: grüner Kreis (spaeter-darstellung 17)
                rounded(c, kreis, rad, u.text_same);
            } else {
                widgets::ring(c, cx0, cy0, rad, line, u.border);
            }
            if fertig && !jetzt {
                let mut p = Path::new();
                p.segment(
                    (cx0 - 4.5 * s, cy0),
                    (cx0 - 1.0 * s, cy0 + 3.5 * s),
                    1.6 * s,
                );
                p.segment(
                    (cx0 - 1.0 * s, cy0 + 3.5 * s),
                    (cx0 + 5.0 * s, cy0 - 3.5 * s),
                    1.6 * s,
                );
                c.fill(&p, u.bg);
            } else {
                let n = (i + 1).to_string();
                let col = if jetzt { u.on_accent } else { u.text_dim };
                let nw = bold.map_or(0.0, |f| f.width(&n, 11.0 * s));
                label(c, bold, &n, 11.0 * s, cx0 - nw * 0.5, cy0 + 4.0 * s, col);
            }
            let name = self.seiten_name(&x.a, &x.seiten, i, &x.antworten);
            let (lx, ly) = at(56.0, y0 + 22.0);
            let font = if jetzt { bold } else { regular };
            let name = widgets::ellipsize(font, &name, 13.0 * s, (SPALTE - 70.0) * s);
            label(
                c,
                font,
                &name,
                13.0 * s,
                lx,
                ly,
                if fertig || jetzt { u.text } else { u.text_dim },
            );
            let unter = if fertig {
                x.seiten[i]
                    .iter()
                    .map(|&j| self.a_antwort_text(x, j))
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
            } else if x.seiten[i].len() > 1 {
                let namen: Vec<String> = x.seiten[i]
                    .iter()
                    .map(|&j| gross(&x.a.schritte[j].key))
                    .collect();
                match namen.split_last() {
                    Some((l, rest)) if !rest.is_empty() => format!("{} und {l}", rest.join(", ")),
                    _ => namen.join(""),
                }
            } else {
                String::new()
            };
            if !unter.is_empty() {
                let (ux, uy) = at(56.0, y0 + 40.0);
                let unter = widgets::ellipsize(regular, &unter, 12.0 * s, (SPALTE - 70.0) * s);
                label(c, regular, &unter, 12.0 * s, ux, uy, u.text_dim);
            }
        }
        // Rechts: Schritt n von m, Frage, Felder
        let seite = &x.seiten[x.seite];
        let (sx, sy) = at(RX, 92.0);
        let n_von = format!("Schritt {} von {}", x.seite + 1, x.seiten.len());
        label(c, regular, &n_von, 12.0 * s, sx, sy, u.text_dim);
        let rbreite = (bw - RX - 28.0) * s;
        let frage = if seite.len() > 1 {
            self.seiten_name(&x.a, &x.seiten, x.seite, &x.antworten)
        } else {
            ablauf::einsetzen(&x.a.schritte[seite[0]].text, &x.antworten)
        };
        let (qx, qy) = at(RX, 124.0);
        let frage = widgets::ellipsize(bold, &frage, 16.0 * s, rbreite);
        label(c, bold, &frage, 16.0 * s, qx, qy, u.text);
        for (pos, &j) in seite.iter().enumerate() {
            self.a_feld_malen(c, t, fonts, w, x, pos, j);
        }
        // Unten rechts: was geschrieben wird
        let letzte = x.seite + 1 == x.seiten.len();
        let hinweis = if letzte {
            let mut teile: Vec<String> = x
                .seiten
                .iter()
                .take(x.seite)
                .flatten()
                .map(|&j| self.a_antwort_text(x, j))
                .filter(|t| !t.is_empty())
                .collect();
            teile.push(ziel.to_string());
            teile.join(" · ")
        } else {
            let wo = if self.projekt {
                "als ein Schritt, den Strg+Z zurücknimmt"
            } else if self.freigabe.is_some() {
                "als eine Änderung im Entwurf"
            } else {
                "mit OK im Firmenkatalog"
            };
            format!(
                "Geschrieben wird erst mit „{}“ in Schritt {}, {wo}.",
                verb(&x.a),
                x.seiten.len()
            )
        };
        let (hx, hy) = at(RX, bh - FUSS - 22.0);
        let hinweis = widgets::ellipsize(regular, &hinweis, 12.0 * s, rbreite);
        label(c, regular, &hinweis, 12.0 * s, hx, hy, u.text_dim);
        if let Some((usize::MAX, e)) = &x.fehler {
            let (ex, ey) = at(RX, bh - FUSS - 44.0);
            let e = widgets::ellipsize(regular, e, 12.0 * s, rbreite);
            label(c, regular, &e, 12.0 * s, ex, ey, u.field_invalid);
        }
        // Knöpfe
        let weiter = if letzte { verb(&x.a) } else { "Weiter" };
        for (z, text, aktiv) in [
            (AZiel::Abbrechen, "Abbrechen", false),
            (AZiel::Zurueck, "Zurück", false),
            (AZiel::Weiter, weiter, true),
        ] {
            let Some(br) = rect(z) else {
                continue;
            };
            let st = ButtonState {
                hover: x.hover == Some(z),
                pressed: x.pressed == Some(z),
                active: aktiv,
                disabled: false,
            };
            widgets::button(c, fonts, br, text, st, s, t);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn a_feld_malen(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        x: &Assistent,
        pos: usize,
        j: usize,
    ) {
        let (s, u) = (w.scale, &t.ui);
        let regular = fonts.regular.as_ref();
        let st = &x.a.schritte[j];
        let fr = self.a_feld(w, pos);
        let fokus = x.fokus == pos;
        let falsch = matches!(&x.fehler, Some((f, _)) if *f == j);
        let mehrere = x.seiten[x.seite].len() > 1;
        let te = &x.te[j];
        // Bezeichnung links auf der Abschlussseite
        if mehrere {
            let base = fr.y + (fr.h + regular.map_or(0.0, |f| f.cap_height(13.0 * s))) * 0.5;
            let (lx, _) = (self.a_rect(w).x + RX * s, 0.0);
            label(c, regular, &st.text, 13.0 * s, lx, base.round(), u.text_dim);
        }
        let pe = ablauf::preiseinheit(st, x.per[j], &x.antworten, &self.jetzt);
        let einheit = match (&st.typ, pe) {
            (Some(Feldart::Mm), _) => "mm".to_string(),
            (Some(Feldart::Stunden), _) => "h".to_string(),
            (Some(Feldart::Geld), Some(e)) => format!("€/{}", e.zeichen()),
            (Some(Feldart::Geld), None) => "€".to_string(),
            _ => String::new(),
        };
        let zahl = matches!(st.typ, Some(Feldart::Mm | Feldart::Geld | Feldart::Stunden));
        let state = FieldState {
            text: &te.text,
            unit: &einheit,
            hover: x.hover == Some(AZiel::Feld(pos)),
            focus: fokus,
            invalid: falsch,
            caret: fokus.then_some(te.caret),
            select: fokus.then(|| te.selection()),
            disabled: false,
        };
        if zahl {
            widgets::field(c, fonts, fr, &state, s, t);
        } else {
            widgets::text_field(c, fonts, fr, &state, s, t);
        }
        // Leiser Platzhalter im leeren Feld (`hint`, nie Antwort); bei der
        // Auswahl „Suchen …“
        let platz = match &st.typ {
            Some(Feldart::Wahl(_)) => "Suchen …",
            _ => st.hint.as_str(),
        };
        if te.text.is_empty() && !platz.is_empty() {
            let px = 13.0 * s;
            label(
                c,
                regular,
                platz,
                px,
                widgets::text_field_x(fr, s, t),
                fr.y + fr.h * 0.5 + 4.5 * s,
                u.text_disabled,
            );
        }
        // Rechts daneben: Fehler, sonst „vorgeschlagen“ bzw. „optional“
        let rechts_x = fr.x + fr.w + 12.0 * s;
        let base = fr.y + fr.h * 0.5 + 4.5 * s;
        let notiz = match &x.fehler {
            Some((f, e)) if *f == j => Some((e.clone(), u.field_invalid)),
            _ if x.preset[j].as_deref() == Some(te.text.as_str()) && !te.text.is_empty() => {
                Some(("vorgeschlagen".to_string(), u.text_dim))
            }
            _ if st.optional => Some(("optional".to_string(), u.text_dim)),
            _ => None,
        };
        if let Some((n, col)) = notiz {
            let r = self.a_rect(w);
            let max = (r.x + r.w - 20.0 * s - rechts_x).max(40.0 * s);
            let n = widgets::ellipsize(regular, &n, 12.0 * s, max);
            label(c, regular, &n, 12.0 * s, rechts_x, base, col);
        }
        if mehrere {
            return;
        }
        match &st.typ {
            Some(Feldart::Geld) => self.a_geld_malen(c, t, fonts, w, x, j, fr),
            Some(Feldart::Wahl(a)) => {
                let opts = self.optionen(a, &te.text);
                let breite = fr.w;
                for (r, (g, name)) in opts.iter().skip(x.oben).take(OPT_N).enumerate() {
                    let zr = self.a_at(w, RX, OPT_Y + r as f32 * OPT_H, 0.0, OPT_H - 2.0);
                    let zr = Rect::new(zr.x, zr.y, breite, zr.h);
                    let gewaehlt = x.wahl[j].as_ref().is_some_and(|(w, _)| w == g);
                    if gewaehlt {
                        rounded(c, zr, 6.0 * s, u.pressed);
                        c.fill_rect(zr.x, zr.y + 4.0 * s, 3.0 * s, zr.h - 8.0 * s, u.accent);
                    } else if x.hover == Some(AZiel::Option(r)) {
                        rounded(c, zr, 6.0 * s, u.hover);
                    }
                    let n = widgets::ellipsize(regular, name, 13.0 * s, breite - 24.0 * s);
                    label(
                        c,
                        regular,
                        &n,
                        13.0 * s,
                        zr.x + 12.0 * s,
                        zr.y + zr.h * 0.5 + 4.5 * s,
                        u.text,
                    );
                }
                if opts.is_empty() {
                    let (lx, ly) = (fr.x, fr.y + fr.h + 26.0 * s);
                    label(c, regular, "Kein Treffer.", 12.0 * s, lx, ly, u.text_dim);
                } else if opts.len() > OPT_N {
                    let (lx, ly) = (
                        fr.x,
                        self.a_rect(w).y + (OPT_Y + OPT_N as f32 * OPT_H + 16.0) * s,
                    );
                    let mehr = format!(
                        "{} von {} · Mausrad blättert",
                        OPT_N.min(opts.len() - x.oben),
                        opts.len()
                    );
                    label(c, regular, &mehr, 12.0 * s, lx, ly, u.text_dim);
                }
            }
            _ => {}
        }
    }

    /// Preis: Einheiten (Regel 108), die Rechnung und zum Vergleich die
    /// Artikel desselben Baustoffs.
    #[allow(clippy::too_many_arguments)]
    fn a_geld_malen(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        x: &Assistent,
        j: usize,
        fr: Rect,
    ) {
        let (s, u) = (w.scale, &t.ui);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let st = &x.a.schritte[j];
        let mut y = fr.y + fr.h + 10.0 * s;
        if st.per.len() > 1 {
            let sichtbar = ablauf::einheiten_sichtbar(st, &x.antworten, &self.jetzt);
            for (i, &n) in sichtbar.iter().enumerate() {
                let e = ablauf::preiseinheit(st, n, &x.antworten, &self.jetzt);
                let r = Rect::new(
                    fr.x + i as f32 * 76.0 * s,
                    y,
                    (72.0 * s).round(),
                    (26.0 * s).round(),
                );
                let an = x.per[j] == n;
                let geht = ablauf::einheit_waehlbar(st, n, &x.antworten, &self.jetzt).is_ok();
                if an {
                    rounded(c, r, 6.0 * s, u.accent);
                } else if x.hover == Some(AZiel::Einheit(n)) && geht {
                    rounded(c, r, 6.0 * s, u.hover);
                } else {
                    rounded(c, r, 6.0 * s, u.pressed);
                }
                let text = e.map(sk_cost::einheit::je_text).unwrap_or_default();
                let col = match (an, geht) {
                    (true, _) => u.on_accent,
                    (false, true) => u.text,
                    (false, false) => u.text_disabled,
                };
                let tw = regular.map_or(0.0, |f| f.width(&text, 12.0 * s));
                label(
                    c,
                    regular,
                    &text,
                    12.0 * s,
                    r.x + (r.w - tw) * 0.5,
                    r.y + r.h * 0.5 + 4.0 * s,
                    col,
                );
            }
            y += 26.0 * s;
        }
        // Rechnung bei anderer Einheit: „110,00 €/m³ × 0,3 m = 33,00 €/m²“
        if x.per[j] > 0 && !x.te[j].text.trim().is_empty() {
            if let Ok(a) =
                ablauf::antwort(st, &x.te[j].text, "", x.per[j], &x.antworten, &self.jetzt)
            {
                y += 22.0 * s;
                label(c, regular, &a.rechnung, 12.0 * s, fr.x, y, u.text_dim);
            }
        }
        // Zum Vergleich: der gewählte Artikel mit seinem bisherigen Preis,
        // sonst Artikel desselben Baustoffs mit Preis
        let wahl = |art: &str| {
            x.a.schritte
                .iter()
                .filter(|q| matches!(&q.typ, Some(Feldart::Wahl(a)) if a == art))
                .find_map(|q| x.antworten.get(&q.key))
                .and_then(|a| Guid::from_ifc(&a.wert))
        };
        let (mut v, kopf): (Vec<_>, _) = if let Some(g) = wahl("article") {
            let v = self.jetzt.artikel.iter().filter(|a| a.guid == g).collect();
            let kopf = if self.projekt {
                "Bisher in diesem Haus"
            } else {
                "Bisher im Firmenkatalog"
            };
            (v, kopf)
        } else if let Some(mat) = wahl("material") {
            let v = self
                .jetzt
                .artikel
                .iter()
                .filter(|a| a.mat == Some(mat))
                .collect();
            let kopf = if self.projekt {
                "Zum Vergleich in diesem Haus"
            } else {
                "Zum Vergleich im Firmenkatalog"
            };
            (v, kopf)
        } else {
            return;
        };
        v.retain(|a| !a.retired && a.preis.is_some());
        v.sort_by_key(|a| a.t);
        if v.is_empty() {
            return;
        }
        y += 40.0 * s;
        label(c, bold, kopf, 12.0 * s, fr.x, y, u.text_dim);
        let r = self.a_rect(w);
        let rechts = r.x + r.w - 28.0 * s;
        for a in v.iter().take(4) {
            y += 22.0 * s;
            let preis = format!(
                "{} €/{}",
                sk_cost::einheit::zahl(a.preis.unwrap_or_default(), 2),
                a.einheit.zeichen()
            );
            let pw = regular.map_or(0.0, |f| f.width(&preis, 13.0 * s));
            let name =
                widgets::ellipsize(regular, &a.name, 13.0 * s, rechts - pw - 16.0 * s - fr.x);
            label(c, regular, &name, 13.0 * s, fr.x, y, u.text);
            label(c, regular, &preis, 13.0 * s, rechts - pw, y, u.text);
        }
    }
}
