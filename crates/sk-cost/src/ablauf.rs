//! Geführte Abläufe (KA-3b5, BIM §3.14–§3.15, Regeln 102–104, 108, 109,
//! verwaltung.md §11): `[flow]` und `[flowstep]` aus dem Werksbestand und
//! dem Firmenkatalog. Ein Ablauf fragt Seite für Seite (Regel 109) und
//! ergibt zuletzt Operationen, die die App als **einen** Schreibvorgang bzw.
//! Rückgängig-Schritt ausführt (Regel 104). Die Zeilen werden nur gelesen;
//! die Datei bleibt bytegleich.
//!
//! Die Abläufe sind dieselbe Liste, die später eine Sprachsteuerung
//! durchgeht (Bausteingrenze §7 Regel 5): Seiten, Fragen, Antworten prüfen
//! und Operationen bilden liegen deshalb hier, nicht im Fenster.

use crate::befund::{Befund, Ort, Schwere};
use crate::einheit::{self, Umgerechnet};
use crate::geld::Dez;
use crate::katalog::{Artikel, Einheit, Katalog};
use crate::op::{Op, NAMEN};
use crate::zeile;
use sk_model::Guid;
use std::collections::BTreeMap;

/// `[flow] kind`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zugang {
    /// Nur in der Verwaltung (Kennwort).
    Admin,
    /// Im Reiter Kosten, nur für dieses Haus.
    User,
}

/// `[flowstep] type`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feldart {
    Mm,
    Geld,
    Stunden,
    Text,
    /// `pick:<abschnitt>`: `material`, `layerset`, `trade`, `article`,
    /// `service`, `lot`.
    Wahl(String),
}

const WAHL: [&str; 6] = ["material", "layerset", "trade", "article", "service", "lot"];

/// `[flowstep] step`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Frage,
    Operation,
    Pruefung,
    Schluss,
}

/// Eine Preiseinheit aus `per` (Regel 108): die erste ohne Quelle, jede
/// weitere mit der Frage, aus der die Umrechnung kommt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Per {
    pub einheit: Einheit,
    pub quelle: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Schritt {
    pub nr: u32,
    pub art: Art,
    pub key: String,
    pub text: String,
    pub typ: Option<Feldart>,
    pub min: Option<Dez>,
    pub max: Option<Dez>,
    pub optional: bool,
    pub op: String,
    pub args: String,
    pub regel: Option<u16>,
    pub preset: String,
    pub hint: String,
    pub per: Vec<Per>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ablauf {
    pub guid: Guid,
    pub name: String,
    /// Einleitungssatz über dem ersten Schritt.
    pub ask: String,
    pub zugang: Zugang,
    pub retired: bool,
    /// Nach `nr` sortiert.
    pub schritte: Vec<Schritt>,
    /// Ungültig (Regeln 102–104, 108, 109): grau, startet nicht.
    pub befund: Option<Befund>,
}

/// Eine Antwort: `wert` geht in `args` (Guid, Zahl mit Punkt, Text),
/// `anzeige` in Texte, Vorbelegung und Schlussmeldung („Porenbeton“,
/// „300“, „33,00“).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Antwort {
    pub wert: String,
    pub anzeige: String,
    /// Bei einem Preis in anderer Einheit: „eingegeben 110,00 €/m³ × 0,3 m“
    /// (Regel 108), sonst leer.
    pub eingabe: String,
    /// Die Rechnung für die Anzeige, sonst leer.
    pub rechnung: String,
}

pub type Antworten = BTreeMap<String, Antwort>;

/// Operationen, die ein Ablauf bilden kann; andere Namen aus `schema()`
/// sind für Abläufe unbekannt (Regel 102). Keine legt eine Bauteilart,
/// einen Typ oder Geometrie an (Regel 104).
const OPS: [&str; 4] = [
    "artikel_anlegen",
    "preis_setzen",
    "firmenwert_setzen",
    "umrechnung_setzen",
];

fn befund(regel: u16, guid: Guid, satz: String) -> Befund {
    Befund::neu(
        regel,
        Schwere::Fehler,
        satz,
        Ort::Satz {
            abschnitt: "flow",
            kennung: guid.to_ifc(),
        },
    )
}

fn r102(name: &str, grund: &str) -> String {
    format!("Ablauf {name} kann nicht starten: {grund}.")
}

/// Liest die Abläufe aus `texte` (Werksbestand zuerst, dann Firma): Ein
/// späterer Ablauf mit derselben Guid ersetzt den früheren samt Schritten.
/// Ausgemusterte bleiben in der Liste (`retired`).
pub fn lesen(texte: &[&str]) -> Vec<Ablauf> {
    let mut out: Vec<Ablauf> = Vec::new();
    for t in texte {
        let mut flows: Vec<(Ablauf, Vec<String>)> = Vec::new();
        let mut schritte: Vec<(Guid, Result<Schritt, String>)> = Vec::new();
        for l in t.lines() {
            let Some(z) = zeile::zerlegen(l) else {
                continue;
            };
            let get = |k: &str| {
                z.paare
                    .iter()
                    .find(|(a, _)| a == k)
                    .map(|(_, v)| v.as_str())
            };
            match z.abschnitt.as_str() {
                "flow" => {
                    let Some(guid) = get("guid").and_then(Guid::from_ifc) else {
                        continue;
                    };
                    let mut fehler = Vec::new();
                    let name = get("name").unwrap_or_default().to_string();
                    if name.is_empty() || name.chars().count() > 70 {
                        fehler.push("der Name fehlt oder ist länger als 70 Zeichen".to_string());
                    }
                    let zugang = match get("kind") {
                        Some("admin") => Zugang::Admin,
                        Some("user") => Zugang::User,
                        _ => {
                            fehler.push("die Angabe Zugang fehlt".to_string());
                            Zugang::Admin
                        }
                    };
                    let a = Ablauf {
                        guid,
                        name,
                        ask: get("ask").unwrap_or_default().to_string(),
                        zugang,
                        retired: get("retired") == Some("1"),
                        schritte: Vec::new(),
                        befund: None,
                    };
                    flows.retain(|(f, _)| f.guid != guid);
                    flows.push((a, fehler));
                }
                "flowstep" => {
                    // Ein Schritt ohne Ablauf bleibt roh (Regel 73)
                    if let Some(flow) = get("flow").and_then(Guid::from_ifc) {
                        schritte.push((flow, schritt(&get)));
                    }
                }
                _ => {}
            }
        }
        for (mut a, mut fehler) in flows {
            for (_, s) in schritte.iter().filter(|(f, _)| *f == a.guid) {
                match s {
                    Ok(s) => a.schritte.push(s.clone()),
                    Err(e) => fehler.push(e.clone()),
                }
            }
            a.schritte.sort_by_key(|s| s.nr);
            a.befund = match fehler.first() {
                Some(g) => Some(befund(102, a.guid, r102(&a.name, g))),
                None => pruefen(&a),
            };
            match out.iter_mut().find(|x| x.guid == a.guid) {
                Some(x) => *x = a,
                None => out.push(a),
            }
        }
    }
    out
}

fn schritt<'a>(get: &dyn Fn(&str) -> Option<&'a str>) -> Result<Schritt, String> {
    let nr = get("nr")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n >= 1)
        .ok_or("ein Schritt hat keine Nummer")?;
    let art = match get("step") {
        Some("ask") => Art::Frage,
        Some("op") => Art::Operation,
        Some("check") => Art::Pruefung,
        Some("done") => Art::Schluss,
        _ => return Err(format!("Schritt {nr} hat eine unbekannte Art")),
    };
    let typ = match get("type") {
        None => None,
        Some("mm") => Some(Feldart::Mm),
        Some("money") => Some(Feldart::Geld),
        Some("hours") => Some(Feldart::Stunden),
        Some("text") => Some(Feldart::Text),
        Some(t) => match t.strip_prefix("pick:") {
            Some(a) if WAHL.contains(&a) => Some(Feldart::Wahl(a.to_string())),
            _ => return Err(format!("Schritt {nr} hat eine unbekannte Feldart")),
        },
    };
    let zahl = |k: &str| -> Result<Option<Dez>, String> {
        match get(k) {
            None => Ok(None),
            Some(v) => Dez::lesen(v, 4)
                .map(Some)
                .ok_or_else(|| format!("Schritt {nr} hat eine ungültige Grenze")),
        }
    };
    let mut per = Vec::new();
    for w in get("per").unwrap_or_default().split_whitespace() {
        let (e, q) = match w.split_once(':') {
            Some((e, q)) => {
                let k = q
                    .strip_prefix('{')
                    .and_then(|q| q.strip_suffix('}'))
                    .filter(|k| ist_key(k))
                    .ok_or_else(|| format!("Schritt {nr}: Umrechnung {e} ist nicht möglich"))?;
                (e, Some(k.to_string()))
            }
            None => (w, None),
        };
        let einheit =
            Einheit::aus(e).ok_or_else(|| format!("Schritt {nr} hat eine unbekannte Einheit"))?;
        per.push(Per { einheit, quelle: q });
    }
    Ok(Schritt {
        nr,
        art,
        key: get("key").unwrap_or_default().to_string(),
        text: get("text").unwrap_or_default().to_string(),
        typ,
        min: zahl("min")?,
        max: zahl("max")?,
        optional: get("optional") == Some("1"),
        op: get("op").unwrap_or_default().to_string(),
        args: get("args").unwrap_or_default().to_string(),
        regel: get("rule").and_then(|v| v.parse().ok()),
        preset: get("preset").unwrap_or_default().to_string(),
        hint: get("hint").unwrap_or_default().to_string(),
        per,
    })
}

fn ist_key(k: &str) -> bool {
    !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `{key}`-Namen in `t`; `Err`, wenn eine Klammer nicht zu einem Namen
/// passt.
fn platzhalter(t: &str) -> Result<Vec<&str>, ()> {
    let mut v = Vec::new();
    let mut rest = t;
    while let Some(i) = rest.find('{') {
        let r = &rest[i + 1..];
        let j = r.find('}').ok_or(())?;
        let k = &r[..j];
        if !ist_key(k) {
            return Err(());
        }
        v.push(k);
        rest = &r[j + 1..];
    }
    Ok(v)
}

/// `args` als Paare `name=wert` (Wert mit Platzhaltern).
fn paare(args: &str) -> Option<Vec<(&str, &str)>> {
    args.split_whitespace().map(|p| p.split_once('=')).collect()
}

/// Angaben einer Operation nach `NAMEN` (ohne Klammern und „+“).
fn angaben(op: &str) -> Vec<&'static str> {
    NAMEN
        .iter()
        .find(|n| n.0 == op)
        .map(|n| {
            n.1.split_whitespace()
                .filter(|w| ist_key(w))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// Regeln 102–104, 108, 109 für einen gelesenen Ablauf.
fn pruefen(a: &Ablauf) -> Option<Befund> {
    let name = &a.name;
    let f102 = |g: String| Some(befund(102, a.guid, r102(name, &g)));
    let n = a.schritte.len();
    if n == 0 {
        return f102("er hat keine Schritte".into());
    }
    for (i, s) in a.schritte.iter().enumerate() {
        if s.nr as usize != i + 1 {
            return f102(format!(
                "die Schritte sind nicht lückenlos ab 1 nummeriert (Schritt {})",
                s.nr
            ));
        }
    }
    if a.schritte[n - 1].art != Art::Schluss {
        return f102("der letzte Schritt ist keine Schlussmeldung".into());
    }
    // Erst Fragen und Prüfungen, dann Operationen, zuletzt die Meldung
    let mut in_ops = false;
    let mut keys: Vec<&str> = Vec::new();
    for s in &a.schritte {
        let nr = s.nr;
        match s.art {
            Art::Frage => {
                if in_ops {
                    return f102(format!("Schritt {nr} fragt nach einer Operation"));
                }
                if !ist_key(&s.key) || keys.contains(&s.key.as_str()) {
                    return f102(format!("Schritt {nr} hat keinen eindeutigen Namen"));
                }
                if s.text.is_empty() {
                    return f102(format!("Schritt {nr} hat keine Frage"));
                }
                let Some(typ) = &s.typ else {
                    return f102(format!("Schritt {nr} hat keine Feldart"));
                };
                if !s.per.is_empty() && *typ != Feldart::Geld {
                    return f102(format!(
                        "Schritt {nr}: Preiseinheiten gibt es nur bei einem Preis"
                    ));
                }
                keys.push(&s.key);
            }
            Art::Pruefung => {
                if in_ops {
                    return f102(format!("Schritt {nr} prüft nach einer Operation"));
                }
                if !s.regel.is_some_and(|r| (71..=109).contains(&r)) {
                    return f102(format!("Schritt {nr} nennt keine Regel"));
                }
            }
            Art::Operation => {
                in_ops = true;
                if !OPS.contains(&s.op.as_str()) {
                    return f102(format!("Schritt {nr} ruft einen unbekannten Vorgang auf"));
                }
                let Some(p) = paare(&s.args) else {
                    return f102(format!("Schritt {nr} hat unlesbare Angaben"));
                };
                let erlaubt = angaben(&s.op);
                if let Some((k, _)) = p.iter().find(|(k, _)| !erlaubt.contains(k)) {
                    return f102(format!("Schritt {nr} kennt die Angabe {k} nicht"));
                }
                // Regel 104: user nur mit kennwortfreien Operationen
                let admin = NAMEN.iter().any(|x| x.0 == s.op && x.2);
                if a.zugang == Zugang::User && admin {
                    return Some(befund(
                        104,
                        a.guid,
                        format!("Ablauf {name} ist für die Verwaltung gedacht und braucht das Kennwort."),
                    ));
                }
            }
            Art::Schluss => {
                if nr as usize != n {
                    return f102(format!("Schritt {nr} meldet den Schluss vor dem Ende"));
                }
                if s.text.is_empty() {
                    return f102(format!("Schritt {nr} hat keine Meldung"));
                }
            }
        }
    }
    if !in_ops {
        return f102("er hat keine Operation".into());
    }
    if platzhalter(&a.ask).map_or(true, |v| !v.is_empty()) {
        return f102("der Einleitungssatz setzt eine Angabe ein".into());
    }
    // Regeln 103 und 109: nur Antworten früherer Seiten (Fragen), nur
    // früherer Fragen (Operationen und Meldung)
    let seiten = seiten(a);
    let seite_von = |i: usize| seiten.iter().position(|s| s.contains(&i));
    for (i, s) in a.schritte.iter().enumerate() {
        let frueher: Vec<&str> = match seite_von(i) {
            Some(p) => seiten[..p]
                .iter()
                .flatten()
                .map(|&j| a.schritte[j].key.as_str())
                .collect(),
            None => a.schritte[..i]
                .iter()
                .filter(|x| x.art == Art::Frage)
                .map(|x| x.key.as_str())
                .collect(),
        };
        let werte = paare(&s.args).unwrap_or_default();
        let felder = [s.text.as_str(), s.preset.as_str(), s.hint.as_str()]
            .into_iter()
            .chain(werte.iter().map(|(_, v)| *v));
        for t in felder {
            let Ok(ks) = platzhalter(t) else {
                return f102(format!("Schritt {} hat eine unvollständige Klammer", s.nr));
            };
            if let Some(k) = ks.iter().find(|k| !frueher.contains(k)) {
                let satz = if seite_von(i).is_some() && a.schritte[..i].iter().any(|x| x.key == *k)
                {
                    (109, format!("Ablauf {name}, Schritt {}: Die Vorbelegung braucht eine Angabe derselben Seite, die beim Öffnen noch leer ist.", s.nr))
                } else {
                    (103, format!("Ablauf {name}, Schritt {}: Eine Angabe wird verwendet, bevor sie gefragt ist.", s.nr))
                };
                return Some(befund(satz.0, a.guid, satz.1));
            }
        }
        // Regel 108: Quellen der Umrechnung
        for (j, p) in s.per.iter().enumerate() {
            let ok = match (&p.quelle, j) {
                (None, 0) => true,
                (Some(q), j) if j > 0 => {
                    let quelle = a.schritte[..i].iter().find(|x| x.key == *q);
                    let erst = s.per[0].einheit;
                    match (p.einheit, quelle.and_then(|x| x.typ.as_ref())) {
                        (Einheit::M3, Some(Feldart::Mm)) => erst == Einheit::M2,
                        (Einheit::M3 | Einheit::St, Some(Feldart::Wahl(w))) => w == "article",
                        _ => false,
                    }
                }
                _ => false,
            };
            if !ok {
                return Some(befund(
                    108,
                    a.guid,
                    format!(
                        "Ablauf {name}, Schritt {}: Umrechnung {} ist nicht möglich.",
                        s.nr,
                        einheit::je_text(p.einheit)
                    ),
                ));
            }
        }
    }
    None
}

/// Seiten des Assistenten (Regel 109) als Indizes in `schritte`: jede
/// Frage eine Seite, außer der Reihe von Textfragen direkt vor der ersten
/// Operation bzw. Prüfung.
pub fn seiten(a: &Ablauf) -> Vec<Vec<usize>> {
    let ende = a
        .schritte
        .iter()
        .position(|s| matches!(s.art, Art::Operation | Art::Pruefung))
        .unwrap_or(a.schritte.len());
    let mut reihe = ende;
    while reihe > 0 {
        let s = &a.schritte[reihe - 1];
        if s.art == Art::Frage && s.typ == Some(Feldart::Text) {
            reihe -= 1;
        } else {
            break;
        }
    }
    let mut v: Vec<Vec<usize>> = (0..reihe)
        .filter(|&i| a.schritte[i].art == Art::Frage)
        .map(|i| vec![i])
        .collect();
    if reihe < ende {
        v.push((reihe..ende).collect());
    }
    v
}

/// Setzt Antworten in `t` ein (`anzeige`); eine fehlende ist leer.
pub fn einsetzen(t: &str, antworten: &Antworten) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let r = &rest[i + 1..];
        match r.find('}') {
            Some(j) => {
                if let Some(a) = antworten.get(&r[..j]) {
                    out.push_str(&a.anzeige);
                }
                rest = &r[j + 1..];
            }
            None => {
                out.push('{');
                rest = r;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Deutsche Zahl „1.234,50“, „0,85“, „300“; Punkt ohne Komma ist der
/// Dezimalpunkt („0.85“).
pub fn zahl_lesen(t: &str, stellen: u32) -> Option<Dez> {
    let t = t.trim();
    let t = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else {
        t.to_string()
    };
    Dez::lesen(&t, stellen)
}

/// „300“, „33,00“ (Geld mit zwei Stellen), „0,45“.
fn zahl_anzeige(d: Dez, typ: &Feldart) -> String {
    match typ {
        Feldart::Geld => einheit::zahl(d, 2),
        _ => einheit::zahl(d, 0),
    }
}

/// Prüft die Eingabe `text` zu Frage `s` (Pflicht, Zahl, `min`/`max`) und
/// macht daraus die Antwort. Bei `pick:` ist `text` die Guid, `name` ihr
/// Anzeigename. `einheit`: gewählte Preiseinheit (Index in `per`), dazu
/// die bisherigen Antworten und der Katalog für die Umrechnung.
pub fn antwort(
    s: &Schritt,
    text: &str,
    name: &str,
    per: usize,
    antworten: &Antworten,
    k: &Katalog,
) -> Result<Antwort, String> {
    let t = text.trim();
    let Some(typ) = &s.typ else {
        return Err("Diese Frage hat keine Feldart.".into());
    };
    if t.is_empty() {
        return if s.optional {
            Ok(Antwort::default())
        } else {
            Err(match typ {
                Feldart::Wahl(_) => "Bitte eines wählen.".into(),
                _ => "Bitte ausfüllen.".into(),
            })
        };
    }
    match typ {
        Feldart::Text => Ok(Antwort {
            wert: t.to_string(),
            anzeige: t.to_string(),
            ..Default::default()
        }),
        Feldart::Wahl(_) => Ok(Antwort {
            wert: t.to_string(),
            anzeige: name.to_string(),
            ..Default::default()
        }),
        Feldart::Mm | Feldart::Geld | Feldart::Stunden => {
            let stellen = match typ {
                Feldart::Mm => 1,
                Feldart::Geld => 4,
                _ => 4,
            };
            let d = zahl_lesen(t, stellen).ok_or("Bitte eine Zahl eingeben.")?;
            let einheit_text = match typ {
                Feldart::Mm => " mm".to_string(),
                Feldart::Stunden => " h".to_string(),
                _ => match s.per.get(per) {
                    Some(p) => format!(" €/{}", p.einheit.zeichen()),
                    None => " €".to_string(),
                },
            };
            // Grenzen gelten für die Eingabe, in der Einheit der Frage
            let gr = |g: Dez| format!("{}{einheit_text}", zahl_anzeige(g, typ));
            match (s.min, s.max) {
                (Some(lo), Some(hi)) if d < lo || d > hi => {
                    return Err(format!("Bitte zwischen {} und {}.", gr(lo), gr(hi)))
                }
                (Some(lo), None) if d < lo => return Err(format!("Bitte mindestens {}.", gr(lo))),
                (None, Some(hi)) if d > hi => return Err(format!("Bitte höchstens {}.", gr(hi))),
                _ => {}
            }
            let mut a = Antwort {
                wert: d.text(),
                anzeige: zahl_anzeige(d, typ),
                ..Default::default()
            };
            if per > 0 {
                let u = umrechnen(s, per, d, antworten, k)?;
                a.wert = u.preis.text();
                a.anzeige = einheit::zahl(u.preis, 2);
                a.eingabe = u.eingabe;
                a.rechnung = u.rechnung;
            }
            Ok(a)
        }
    }
}

/// Artikel, aus dem die Umrechnung kommt: der gewählte, oder ein neuer
/// Stein in m² mit der gefragten Dicke.
fn quelle_artikel(
    s: &Schritt,
    per: usize,
    antworten: &Antworten,
    k: &Katalog,
) -> Result<Artikel, String> {
    let p = s.per.get(per).ok_or("Diese Einheit gibt es nicht.")?;
    let q = p.quelle.as_deref().unwrap_or_default();
    let a = antworten
        .get(q)
        .ok_or("Die Angabe für die Umrechnung fehlt.")?;
    if let Some(g) = Guid::from_ifc(&a.wert) {
        return k
            .artikel(g)
            .cloned()
            .ok_or_else(|| "Den Artikel gibt es nicht.".to_string());
    }
    let t = Dez::lesen(&a.wert, 1).ok_or("Die Dicke fehlt.")?;
    Ok(Artikel {
        guid: Guid(0),
        name: "der neue Stein".into(),
        mat: None,
        kategorie: None,
        t: Some(t),
        einheit: s.per[0].einheit,
        preis: None,
        conv: None,
        std: false,
        retired: false,
        satz: crate::satz::Satz::neu(&crate::satz::ARTICLE),
    })
}

fn umrechnen(
    s: &Schritt,
    per: usize,
    d: Dez,
    antworten: &Antworten,
    k: &Katalog,
) -> Result<Umgerechnet, String> {
    let a = quelle_artikel(s, per, antworten, k)?;
    einheit::umrechnen(d, s.per[per].einheit, &a, None).map_err(|b| b.satz)
}

/// Ist Preiseinheit `per` der Frage `s` mit den bisherigen Antworten
/// wählbar? `Err`: der Grund für die graue Einheit (Regel 108).
pub fn einheit_waehlbar(
    s: &Schritt,
    per: usize,
    antworten: &Antworten,
    k: &Katalog,
) -> Result<(), String> {
    if per == 0 {
        return Ok(());
    }
    umrechnen(s, per, Dez::EINS, antworten, k).map(|_| ())
}

/// Die Operationen nach dem letzten Schritt (Regel 104), Angaben aus den
/// Antworten; `stand` ist der Preisstand „MM/JJJJ“ für neue Preise.
pub fn ops(a: &Ablauf, antworten: &Antworten, stand: &str) -> Result<Vec<Op>, String> {
    let mut v = Vec::new();
    for s in a.schritte.iter().filter(|s| s.art == Art::Operation) {
        let mut werte: BTreeMap<&str, String> = BTreeMap::new();
        let mut eingabe = String::new();
        for (k, w) in paare(&s.args).ok_or("unlesbare Angaben")? {
            // Ein Wert ist genau ein Platzhalter oder ein festes Wort
            let x = match w.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
                Some(key) => {
                    let ant = antworten.get(key).cloned().unwrap_or_default();
                    if !ant.eingabe.is_empty() {
                        eingabe = ant.eingabe;
                    }
                    ant.wert
                }
                None => w.to_string(),
            };
            werte.insert(k, x);
        }
        let text = |k: &str| werte.get(k).cloned().unwrap_or_default();
        let guid = |k: &str| -> Result<Option<Guid>, String> {
            match werte.get(k).filter(|w| !w.is_empty()) {
                None => Ok(None),
                Some(w) => Guid::from_ifc(w)
                    .map(Some)
                    .ok_or_else(|| format!("{k} ist keine Kennung")),
            }
        };
        let zahl = |k: &str| -> Result<Option<Dez>, String> {
            match werte.get(k).filter(|w| !w.is_empty()) {
                None => Ok(None),
                Some(w) => Dez::lesen(w, 4)
                    .map(Some)
                    .ok_or_else(|| format!("{k} ist keine Zahl")),
            }
        };
        let preisstand = |p: &Option<Dez>| match text("stand") {
            s if !s.is_empty() => s,
            _ if p.is_some() => stand.to_string(),
            _ => String::new(),
        };
        // Quelle nennt die Eingabe in anderer Einheit (Regel 108)
        let quelle = match (text("quelle"), eingabe.as_str()) {
            (q, "") => q,
            (q, e) if q.is_empty() => e.to_string(),
            (q, e) => format!("{q} · {e}"),
        };
        v.push(match s.op.as_str() {
            "artikel_anlegen" => {
                let preis = zahl("preis")?;
                Op::ArtikelAnlegen {
                    baustoff: guid("baustoff")?,
                    name: text("name"),
                    dicke: zahl("dicke")?,
                    guete: text("guete"),
                    format: text("format"),
                    einheit: Einheit::aus(&text("einheit")).ok_or("einheit fehlt")?,
                    stand: preisstand(&preis),
                    preis,
                    quelle,
                    lieferant: text("lieferant"),
                    standard: text("standard") == "1",
                }
            }
            "preis_setzen" => {
                let preis = zahl("preis")?;
                Op::PreisSetzen {
                    artikel: guid("artikel")?.ok_or("artikel fehlt")?,
                    stand: preisstand(&preis),
                    preis,
                    quelle: text("quelle"),
                    eingabe,
                }
            }
            "firmenwert_setzen" => Op::FirmenwertSetzen {
                schluessel: text("schluessel"),
                wert: zahl("wert")?.ok_or("wert fehlt")?,
            },
            "umrechnung_setzen" => Op::UmrechnungSetzen {
                artikel: guid("artikel")?.ok_or("artikel fehlt")?,
                conv: zahl("conv")?,
            },
            o => return Err(format!("{o} kann ein Ablauf nicht aufrufen")),
        });
    }
    Ok(v)
}

/// Abläufe in `schema()` (K6): Felder, Schritte, Feldarten und die
/// Operationen, die ein Ablauf aufrufen kann.
pub(crate) fn schema_text() -> String {
    let mut s = String::from("Abläufe · nur .szk, gelesen, nie geschrieben (BIM §3.14–§3.15)\n");
    s += "  [flow] guid name ask kind=admin|user retired\n";
    s += "  [flowstep] guid flow nr step=ask|op|check|done key text type min max optional op args rule preset hint per\n";
    s +=
        "  type = mm | money | hours | text | pick:<material|layerset|trade|article|service|lot>\n";
    s += "  per = erste Einheit, dann einheit:{key} (m3 aus mm oder pick:article, st aus pick:article)\n";
    s += &format!("  op = {}\n", OPS.join(" | "));
    s
}

/// Schlussmeldung mit den Antworten.
pub fn schluss(a: &Ablauf, antworten: &Antworten) -> String {
    a.schritte
        .iter()
        .find(|s| s.art == Art::Schluss)
        .map(|s| einsetzen(&s.text, antworten))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn werk() -> Vec<Ablauf> {
        lesen(&[crate::WERK])
    }

    fn stein() -> Ablauf {
        werk()
            .into_iter()
            .find(|a| a.name == "Neuen Stein mit Preis anlegen")
            .unwrap()
    }

    /// Regel 109: Die Werksabläufe haben 4, 3 und 1 Seite und sind gültig;
    /// ihre Operationen und Angaben stehen in `schema()` (Regel 102).
    #[test]
    fn werksablaeufe() {
        let w = werk();
        assert_eq!(w.len(), 3);
        for a in &w {
            assert!(a.befund.is_none(), "{}: {:?}", a.name, a.befund);
        }
        let n: Vec<usize> = w.iter().map(|a| seiten(a).len()).collect();
        assert_eq!(n, [4, 3, 1]);
        assert_eq!(w[1].zugang, Zugang::User);
        let schema = crate::schema();
        for a in &w {
            for s in a.schritte.iter().filter(|s| s.art == Art::Operation) {
                assert!(schema.contains(&format!("  {} (", s.op)), "{}", s.op);
            }
        }
    }

    /// Abnahme 9: Porenbeton, 300 mm, 33,00 € ergibt einen Artikel; der
    /// Name ist vorbelegt, die Quelle bleibt ohne Eingabe leer; je m³ mit
    /// 110,00 ergibt dieselben 33,00 €/m² und die Quelle nennt die Eingabe;
    /// 600 mm geht nicht.
    #[test]
    fn stein_anlegen() {
        let a = stein();
        let k = crate::lesen::werk(&sk_model::Model::new());
        let s = &a.schritte;
        let mut ant = Antworten::new();
        let mat = "2wuC33GkTD9Qack6WJ4EsM";
        ant.insert(
            "baustoff".into(),
            antwort(&s[0], mat, "Porenbeton", 0, &ant, &k).unwrap(),
        );
        assert_eq!(
            antwort(&s[1], "600", "", 0, &ant, &k).unwrap_err(),
            "Bitte zwischen 50 mm und 500 mm."
        );
        ant.insert(
            "dicke".into(),
            antwort(&s[1], "300", "", 0, &ant, &k).unwrap(),
        );
        let p = antwort(&s[2], "33,00", "", 0, &ant, &k).unwrap();
        assert_eq!((p.wert.as_str(), p.anzeige.as_str()), ("33", "33,00"));
        let m3 = antwort(&s[2], "110,00", "", 1, &ant, &k).unwrap();
        assert_eq!(m3.wert, "33");
        assert_eq!(m3.eingabe, "eingegeben 110,00 €/m³ × 0,3 m");
        assert_eq!(einsetzen(&s[3].preset, &ant), "Porenbeton-Stein d=300mm");
        assert_eq!(s[4].hint, "z. B. Händler Müller 10/2026");
        let name = antwort(&s[3], "Porenbeton-Stein d=300mm", "", 0, &ant, &k).unwrap();
        ant.insert("name".into(), name);
        ant.insert(
            "quelle".into(),
            antwort(&s[4], "", "", 0, &ant, &k).unwrap(),
        );
        for (preis, quelle) in [(p, ""), (m3, "eingegeben 110,00 €/m³ × 0,3 m")] {
            ant.insert("preis".into(), preis);
            let ops = ops(&a, &ant, "10/2026").unwrap();
            assert_eq!(ops.len(), 1);
            let Op::ArtikelAnlegen {
                baustoff,
                name,
                dicke,
                einheit,
                preis,
                stand,
                quelle: q,
                ..
            } = &ops[0]
            else {
                panic!("{ops:?}");
            };
            assert_eq!(baustoff.map(|g| g.to_ifc()).as_deref(), Some(mat));
            assert_eq!(name, "Porenbeton-Stein d=300mm");
            assert_eq!(*dicke, Some(Dez::ganz(300)));
            assert_eq!(*einheit, Einheit::M2);
            assert_eq!(*preis, Some(Dez::ganz(33)));
            assert_eq!(stand, "10/2026");
            assert_eq!(q, quelle);
        }
        assert_eq!(
            schluss(&a, &ant),
            "Stein Porenbeton-Stein d=300mm mit 33,00 €/m² angelegt."
        );
    }

    /// Regel 102: Eine Lücke in `nr`, ein unbekannter Vorgang oder ein
    /// fehlender Schluss machen den Ablauf ungültig; Regel 103: eine
    /// Angabe vor ihrer Frage; Regel 109: Vorbelegung aus derselben Seite.
    #[test]
    fn ungueltige_ablaeufe() {
        let kopf = "[flow] guid=1S7bUW0010080700000009 name=\"Probe\" kind=admin\n";
        let s = |nr: u32, rest: &str| {
            format!("[flowstep] guid=1S7bUW00100808000000{nr:02} flow=1S7bUW0010080700000009 nr={nr} {rest}\n")
        };
        let frage = s(1, "step=ask key=lohn text=\"Lohn?\" type=money");
        let op = |nr| {
            s(
                nr,
                "step=op op=firmenwert_setzen args=\"schluessel=wage wert={lohn}\"",
            )
        };
        let done = |nr| s(nr, "step=done text=\"Fertig.\"");
        let regel = |t: String| lesen(&[&t])[0].befund.as_ref().map(|b| b.regel);
        assert_eq!(regel(format!("{kopf}{frage}{}{}", op(2), done(3))), None);
        assert_eq!(
            regel(format!("{kopf}{frage}{}{}", op(2), done(4))),
            Some(102)
        );
        assert_eq!(regel(format!("{kopf}{frage}{}", op(2))), Some(102));
        let fremd = s(2, "step=op op=bauteiltyp_anlegen args=\"name={lohn}\"");
        assert_eq!(regel(format!("{kopf}{frage}{fremd}{}", done(3))), Some(102));
        let vor = s(
            2,
            "step=op op=firmenwert_setzen args=\"schluessel=wage wert={jahr}\"",
        );
        assert_eq!(regel(format!("{kopf}{frage}{vor}{}", done(3))), Some(103));
        let name = s(1, "step=ask key=name text=\"Name\" type=text");
        let quelle = s(
            2,
            "step=ask key=quelle text=\"Quelle\" type=text preset=\"{name}\"",
        );
        let op3 = s(
            3,
            "step=op op=firmenwert_setzen args=\"schluessel=wage wert=60\"",
        );
        assert_eq!(
            regel(format!("{kopf}{name}{quelle}{op3}{}", done(4))),
            Some(109)
        );
        let user = kopf.replace("kind=admin", "kind=user");
        let kw = s(2, "step=op op=vorschlag_ablehnen args=\"key={lohn}\"");
        // vorschlag_ablehnen kann kein Ablauf: unbekannt vor Regel 104
        assert_eq!(regel(format!("{user}{frage}{kw}{}", done(3))), Some(102));
        // Ein späterer Text ersetzt den Ablauf mit derselben Guid
        let alt = format!("{kopf}{frage}{}{}", op(2), done(3));
        let neu = alt.replace("name=\"Probe\"", "name=\"Probe 2\"");
        let l = lesen(&[&alt, &neu]);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "Probe 2");
    }
}
