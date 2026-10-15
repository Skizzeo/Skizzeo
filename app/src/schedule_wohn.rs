//! Wohnflächenberechnung als Blatt im Mengenfenster (G8; Jörn 13.10.
//! 02:14: „Wohnflächenberechnung mit Mengenansätzen als gesonderte Rubrik
//! in Mengen-Auswertung“): dieselbe Liste wie die Mengenermittlung, je
//! Gebäude und Geschoss die Räume mit Faktor, Grund- und Wohnfläche;
//! aufgeklappt stehen unter dem Raum die Teilflächen als Mengenansätze,
//! Zuschläge und Abzüge, die Hinweise und die Anrechnung. Gerechnet wird
//! in [`sk_raum::wohnflaeche`], hier entstehen nur die Zeilen und die
//! Tabelle. Ausgelagert, damit `schedule_view.rs` nicht wächst (R4).

use super::tabelle::{csv_field, csv_num};
use super::*;
use sk_model::qto::Umfang;
use sk_raum::wohnflaeche::{self as wf, Gebaeude, Geschoss, WohnRaum, Wohnflaeche};
use std::rc::Rc;

/// Titel des Blatts.
pub(super) const TITEL: &str = "Wohnflächenberechnung";

/// Grundlagen in der Zeile, in der die Mengenermittlung ihre Umschalter hat.
pub(super) const GRUNDLAGEN: &str =
    "nach WoFlV · lichte Maße an der fertigen Oberfläche · je Raum auf 0,01 m² gerundet";

/// Spaltenköpfe: Raum, Nr., Faktor, Grundfläche, Wohnfläche.
pub(super) const KOPF: [&str; CELLS] = ["Raum", "Nr.", "Faktor", "Grundfläche", "Wohnfläche"];

/// Gerechnete Wohnfläche und für welchen Stand (Szene, Modellstand).
#[derive(Default)]
pub(super) struct Stand {
    fuer: Option<(u64, u64)>,
    rechnung: Rc<Wohnflaeche>,
    laeufe: u64,
}

/// Wohnfläche des Projekts aus der Szene: die Räume aus ihrer Erkennung,
/// die Züge aus ihren berechneten Ketten, wie die Mengenliste.
pub fn rechnen(s: &Scene) -> Wohnflaeche {
    let k = s.ketten();
    // Räume aller Geschosse mit Räumen in einem Gang der Erkennung, nicht
    // je Geschoss über alle Züge (Review #195 H-195g)
    let mut gruppen = s.raumgruppen_mit_raeumen();
    wf::projekt_im_umfeld(s.model(), &k, &s.woflv_umfeld(), |g| {
        gruppen
            .remove(&g)
            .map(|r| r.flaechen().map(|f| f.form.clone()).collect())
            .unwrap_or_default()
    })
}

/// Gebäude und Geschosse im Umfang des Blatts.
fn im_umfang<'a>(w: &'a Wohnflaeche, u: &Umfang) -> Vec<(&'a Gebaeude, Vec<&'a Geschoss>)> {
    w.gebaeude
        .iter()
        .filter(|b| u.gebaeude.is_none() || b.id == u.gebaeude)
        .map(|b| {
            let gs = b
                .geschosse
                .iter()
                .filter(|g| !u.ohne.contains(&g.id))
                .collect::<Vec<_>>();
            (b, gs)
        })
        .filter(|(_, gs)| !gs.is_empty())
        .collect()
}

/// Wohn- und Grundfläche (0,01 m²) und Anzahl der Räume im Umfang.
pub fn summe(w: &Wohnflaeche, u: &Umfang) -> (i64, i64, usize) {
    let mut out = (0, 0, 0);
    for g in im_umfang(w, u).into_iter().flat_map(|(_, gs)| gs) {
        out.0 += g.wohn();
        out.1 += g.grund();
        out.2 += g.raeume.len();
    }
    out
}

/// Lebende Zahl der Karte: „142,37 m² · 9 Räume“.
pub fn karte(w: &Wohnflaeche, u: &Umfang) -> String {
    match summe(w, u) {
        (_, _, 0) => "keine Räume".into(),
        (h, _, 1) => format!("{} · 1 Raum", wf::qm(h)),
        (h, _, n) => format!("{} · {n} Räume", wf::qm(h)),
    }
}

/// „(2) 2,970 × 2,000“: die Nummer der Teilfläche wie in der Skizze.
fn ansatz(t: &wf::Teil) -> String {
    match t.nr {
        Some(n) => format!("({n}) {}", t.text),
        None => t.text.clone(),
    }
}

/// Unter dem aufgeklappten Raum: Teilflächen mit Vorzeichen und Wert in
/// der Spalte Grundfläche, die Hinweise, die Grundfläche und die
/// Anrechnung mit der Wohnfläche.
fn teil_zeilen(r: &WohnRaum, row: &Line, lines: &mut Vec<Line>) {
    let (i0, g0) = match row.key {
        Key::Element(i, g) => (i, g),
        _ => (u32::MAX, 0),
    };
    let mut push = |i: usize, text: String, grund: String, wohn: String| {
        let mut l = Line::new(
            Kind::Control,
            row.depth + 1,
            Key::Term(i0, g0, 0, 3, i.min(u16::MAX as usize) as u16),
        );
        l.storey = row.storey;
        l.parent = row.key;
        l.elements = row.elements.clone();
        l.cells[0] = text;
        l.cells[3] = grund;
        l.cells[4] = wohn;
        lines.push(l);
    };
    let mut erste = true;
    let mut i = 0;
    for t in &r.teile {
        let (text, wert) = match t.art {
            Art::Hinweis => (t.text.clone(), String::new()),
            _ if erste => (ansatz(t), wf::qm3(t.wert)),
            _ if t.wert < 0.0 => (
                format!("− {}", ansatz(t)),
                format!("− {}", wf::qm3(-t.wert)),
            ),
            _ => (format!("+ {}", ansatz(t)), format!("+ {}", wf::qm3(t.wert))),
        };
        if t.art != Art::Hinweis {
            erste = false;
        }
        push(i, text, wert, String::new());
        i += 1;
    }
    push(i, "= Grundfläche".into(), wf::qm(r.grund), String::new());
    let f = &r.faktor;
    push(
        i + 1,
        format!("× {} · {}", wf::prozent(f.prozent), f.grund),
        String::new(),
        wf::qm(r.wohn),
    );
}

/// Zeilen des Blatts im Umfang `u`; Teilflächen nur unter den in `offen`
/// aufgeklappten Räumen.
pub(super) fn zeilen(w: &Wohnflaeche, u: &Umfang, offen: &HashSet<Key>) -> Vec<Line> {
    let mut lines = Vec::new();
    let teil = im_umfang(w, u);
    let many = teil.len() > 1;
    for (b, gs) in &teil {
        let bkey = Key::Building(b.id.map_or(u32::MAX, |x| x.index()));
        if many {
            let mut l = Line::new(Kind::Building, 0, bkey);
            l.cells[0] = b.name.clone();
            lines.push(l);
        }
        let (mut grund, mut wohn) = (0, 0);
        for g in gs {
            let mut st = Line::new(Kind::Storey, 0, storey_key(g.id));
            st.cells[0] = g.name.clone();
            st.cells[3] = wf::qm(g.grund());
            st.cells[4] = wf::qm(g.wohn());
            st.elements = g.raeume.iter().filter_map(|r| r.raum).collect();
            lines.push(st);
            for r in &g.raeume {
                let key = r.raum.map_or(Key::None, elem_key);
                let mut row = Line::new(Kind::Row, 1, key);
                row.storey = storey_key(g.id);
                row.elements = r.raum.into_iter().collect();
                row.cells = [
                    r.name.clone(),
                    r.nummer.clone(),
                    wf::prozent(r.faktor.prozent),
                    wf::qm(r.grund),
                    wf::qm(r.wohn),
                ];
                row.weg = key != Key::None;
                let offen = row.weg && offen.contains(&key);
                lines.push(row.clone());
                if offen {
                    teil_zeilen(r, &row, &mut lines);
                }
            }
            grund += g.grund();
            wohn += g.wohn();
        }
        lines.push(Line::new(Kind::Rule, 0, Key::None));
        let mut head = Line::new(Kind::SumHead, 0, Key::None);
        head.cells[0] = format!("Wohnfläche · {}", b.name);
        head.cells[3] = wf::qm(grund);
        head.cells[4] = wf::qm(wohn);
        lines.push(head);
    }
    if lines.is_empty() {
        let mut l = Line::new(Kind::Control, 0, Key::None);
        l.cells[0] =
            "Keine Räume im Umfang. Räume entstehen, sobald Wände eine Fläche umschließen.".into();
        lines.push(l);
    }
    lines
}

impl ListView {
    /// Blatt Wohnfläche (statt der Mengenermittlung), gerechnet.
    #[cfg(test)]
    pub fn wohnflaeche(s: &mut Scene) -> ListView {
        let mut v = ListView::wohnflaeche_leer();
        v.sync(s, false);
        v
    }

    /// Blatt Wohnfläche, noch nicht gerechnet: Das Mengenfenster rechnet
    /// es erst, wenn seine Karte gewählt ist (Review #195 H-195f).
    pub fn wohnflaeche_leer() -> ListView {
        let mut v = ListView::leer(Grouping::Storey);
        v.wohn = Some(Stand::default());
        v
    }

    /// Die zuletzt gerechnete Wohnfläche; keine, solange nicht gerechnet.
    pub fn wohn_rechnung(&self) -> Option<Rc<Wohnflaeche>> {
        self.wohn
            .as_ref()
            .filter(|w| w.fuer.is_some())
            .map(|w| w.rechnung.clone())
    }

    /// Wie [`ListView::sync`] für die Wohnfläche: Gerechnet wird einmal je
    /// Szene und Modellstand, nie während eines offenen Schritts (Ziehen);
    /// dann bleibt der alte Stand und die Pille zeigt „wird aktualisiert“.
    pub(super) fn sync_wohn(&mut self, s: &mut Scene, animate: bool) -> bool {
        if self.leiste.sync(s.model()) {
            self.umfang_shown = None;
        }
        let st = self.wohn.as_mut().expect("Blatt Wohnfläche");
        let stand = (s.kennung(), s.model().revision());
        if st.fuer != Some(stand) && (st.fuer.is_none() || !s.model().in_step()) {
            st.rechnung = Rc::new(rechnen(s));
            st.fuer = Some(stand);
            st.laeufe += 1;
        }
        let (laeufe, w, alt) = (st.laeufe, st.rechnung.clone(), st.fuer != Some(stand));
        let mut changed = false;
        let umfang_neu = self.umfang_shown.as_ref() != Some(&self.leiste.umfang);
        if self.runs != Some(laeufe) || umfang_neu {
            let animate = animate && self.runs.is_some() && !umfang_neu;
            let lines = zeilen(&w, &self.leiste.umfang, &self.open_groups);
            self.setzen(s.model(), lines, Vec::new(), animate);
            self.runs = Some(laeufe);
            self.umfang_shown = Some(self.leiste.umfang.clone());
            changed = true;
        }
        if alt != self.stale.is_some() {
            self.stale = alt.then(Instant::now);
            changed = true;
        }
        changed
    }
}

/// Fläche aus 0,01 m² für die Tabelle: „19,73“.
fn csv_qm(h: i64) -> String {
    let v = if h < 0 { "-" } else { "" };
    format!("{v}{},{:02}", h.abs() / 100, h.abs() % 100)
}

/// „Als Tabelle speichern“ auf dem Blatt Wohnfläche: UTF-8 mit BOM,
/// Semikolon, Dezimalkomma; die Kopfzeile des Umfangs zuerst. Je Raum eine
/// Zeile mit Grundfläche, Faktor und Wohnfläche, darunter seine
/// Mengenansätze (Teilflächen auf 4 Stellen) und Hinweise; Summen je
/// Geschoss, Gebäude und, bei mehreren Gebäuden, für das Projekt.
pub fn csv(w: &Wohnflaeche, u: &Umfang, kopf: &str) -> Vec<u8> {
    let mut out = String::from("\u{feff}");
    let mut row = |cols: [&str; 10]| {
        let v: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&v.join(";"));
        out.push_str("\r\n");
    };
    row([
        &format!("{TITEL} · {kopf}"),
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
    ]);
    row([
        "Gebäude",
        "Geschoss",
        "Raum",
        "Nr.",
        "Mengenansatz",
        "Teilfläche (m²)",
        "Grundfläche (m²)",
        "Faktor (%)",
        "Wohnfläche (m²)",
        "Hinweis",
    ]);
    let teil = im_umfang(w, u);
    let prozent = |p: f64| wf::prozent(p).trim_end_matches(" %").to_string();
    for (b, gs) in &teil {
        for g in gs {
            for r in &g.raeume {
                let f = &r.faktor;
                row([
                    &b.name,
                    &g.name,
                    &r.name,
                    &r.nummer,
                    "",
                    "",
                    &csv_qm(r.grund),
                    &prozent(f.prozent),
                    &csv_qm(r.wohn),
                    &f.grund,
                ]);
                for t in &r.teile {
                    let (ansatz, wert, hinweis) = match t.art {
                        Art::Hinweis => (String::new(), String::new(), t.text.as_str()),
                        _ => (ansatz(t), csv_num(t.wert / 1e6), ""),
                    };
                    row([
                        &b.name, &g.name, &r.name, &r.nummer, &ansatz, &wert, "", "", "", hinweis,
                    ]);
                }
            }
            let summe = format!("Summe {}", g.name);
            row([
                &b.name,
                &g.name,
                &summe,
                "",
                "",
                "",
                &csv_qm(g.grund()),
                "",
                &csv_qm(g.wohn()),
                "",
            ]);
        }
        let grund: i64 = gs.iter().map(|g| g.grund()).sum();
        let wohn: i64 = gs.iter().map(|g| g.wohn()).sum();
        let summe = format!("Summe {}", b.name);
        row([
            &b.name,
            "",
            &summe,
            "",
            "",
            "",
            &csv_qm(grund),
            "",
            &csv_qm(wohn),
            "",
        ]);
    }
    if teil.len() > 1 {
        let (wohn, grund, _) = summe(w, u);
        row([
            "",
            "",
            "Summe Projekt",
            "",
            "",
            "",
            &csv_qm(grund),
            "",
            &csv_qm(wohn),
            "",
        ]);
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    /// Negative Flächen behalten ihr Vorzeichen (Review #195 H-195h).
    #[test]
    fn csv_qm_mit_vorzeichen() {
        assert_eq!(super::csv_qm(1973), "19,73");
        assert_eq!(super::csv_qm(0), "0,00");
        assert_eq!(super::csv_qm(-5), "-0,05");
        assert_eq!(super::csv_qm(-1973), "-19,73");
    }
}
