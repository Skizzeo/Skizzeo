//! Erweiterungsbauteile im Mengenfenster (Schrittplan E8a): je Bauteil eine
//! aufklappbare Gruppe mit der Summe je `[menge]`, aufgeklappt je Exemplar
//! Nummer und Typ mit seinen Mengen. Nach Gewerk steht jede Menge unter ihrem
//! Gewerk (Menge, sonst Bauteil). Die Spalte folgt der Einheit: m und Stück
//! unter „Länge · Stück“, m² unter „Fläche“, m³, t und kg unter „Volumen“.

use super::{de, number_range, storey_name, Key, Kind, Line};
use sk_model::erweiterung::anzeige;
use sk_model::qto::{BuildingQto, ElementQto, ExtMenge, GroupQto, RowQto};
use sk_model::trade::TradeId;
use sk_model::{ElementKind, Model, StoreyId};

/// Höchstzahl Zeichen eines Namens aus der Definition.
const NAME_MAX: usize = 60;

/// Kennzeichen der Gruppen von Erweiterungen in [`Key::Group`].
const EXT_BIT: u32 = 0x8000_0000;

/// Menge mit Einheit wie in der Werkbank: „0,152 m³“, „20,00 m“, „4 Stk“.
pub(super) fn menge_text(v: f64, einheit: &str) -> String {
    let (d, e) = match einheit {
        "m3" => (3, "m³".to_string()),
        "m2" => (2, "m²".to_string()),
        "m" => (2, "m".to_string()),
        "t" => (3, "t".to_string()),
        "kg" => (1, "kg".to_string()),
        "stk" => (0, "Stk".to_string()),
        e => (2, anzeige(e, 12)),
    };
    format!("{} {e}", de(v, d)).trim_end().to_string()
}

/// Spalte einer Menge (Index in `Line::cells`).
pub(super) fn spalte(einheit: &str) -> usize {
    match einheit {
        "m" | "stk" => 2,
        "m2" => 3,
        _ => 4,
    }
}

/// Stelle der Definition `key` im Projekt (für die Zeilenschlüssel).
fn stelle(m: &Model, key: &str) -> u32 {
    m.ext_defs()
        .iter()
        .position(|d| d.key == key)
        .map_or(EXT_BIT - 1, |i| i as u32)
}

/// Name der Gruppe: Mehrzahl der Definition.
pub(super) fn titel(m: &Model, key: &str) -> String {
    m.ext_def(key)
        .map_or_else(|| anzeige(key, NAME_MAX), |d| anzeige(d.plural(), NAME_MAX))
}

/// Name des Bauteils in der Einzahl.
fn name(m: &Model, key: &str) -> String {
    m.ext_def(key)
        .map_or_else(|| anzeige(key, NAME_MAX), |d| anzeige(d.name(), NAME_MAX))
}

/// Typname des Exemplars, wenn es einen Typ hat.
fn typ(m: &Model, r: &RowQto) -> Option<String> {
    let ElementKind::Ext(p) = &m.element(r.element)?.kind else {
        return None;
    };
    let t = m.ext_def(&p.key)?.typ(p.typ.as_deref()?)?;
    Some(anzeige(t.get("name").unwrap_or(t.key()), NAME_MAX))
}

/// Ein Exemplar mit den Mengen, die es in dieser Gliederung zeigt.
struct Exemplar<'a> {
    row: &'a RowQto,
    mengen: Vec<&'a ExtMenge>,
    /// Kürzel des Geschosses (nach Gewerk), sonst `None`.
    geschoss: Option<String>,
}

/// Summe je Menge über die Exemplare: Satz, Name, Einheit, Wert (`None`,
/// wenn keines rechnet).
fn summen<'a>(ex: &[Exemplar<'a>]) -> Vec<(usize, &'a str, &'a str, Option<f64>)> {
    let mut out: Vec<(usize, &str, &str, Option<f64>)> = Vec::new();
    for q in ex.iter().flat_map(|x| x.mengen.iter()) {
        match out.iter_mut().find(|s| s.0 == q.satz) {
            Some(s) => {
                if let Some(v) = q.wert {
                    s.3 = Some(s.3.unwrap_or(0.0) + v);
                }
            }
            None => out.push((q.satz, &q.name, &q.einheit, q.wert)),
        }
    }
    out.sort_by_key(|s| s.0);
    out
}

/// Gruppenzeile, Summen und die Exemplare eines Bauteils. `gkey` ist die
/// Gruppe, `sum` erzeugt den Schlüssel der Summe einer Menge.
fn block(
    m: &Model,
    key: &str,
    skey: Key,
    gkey: Key,
    sum: impl Fn(u16) -> Key,
    ex: &[Exemplar],
    lines: &mut Vec<Line>,
) {
    let alle: Vec<_> = ex.iter().map(|x| x.row.element).collect();
    let mut gl = Line::new(Kind::Group, 1, gkey);
    gl.storey = skey;
    gl.elements = alle.clone();
    gl.cells[0] = titel(m, key);
    gl.cells[1] = match ex {
        [] => String::new(),
        [one] => one.row.number.clone(),
        [first, .., last] => {
            let tail = last
                .row
                .number
                .rsplit('-')
                .next()
                .unwrap_or(&last.row.number);
            format!("{} … {tail}", first.row.number)
        }
    };
    gl.cells[2] = ex.iter().filter(|x| x.row.q.is_some()).count().to_string();
    lines.push(gl);
    for (satz, name, einheit, wert) in summen(ex) {
        let mut cl = Line::new(Kind::Control, 2, sum(satz as u16));
        cl.storey = skey;
        cl.elements = alle.clone();
        cl.cells[0] = name.to_string();
        cl.cells[spalte(einheit)] = wert.map_or("–".into(), |v| menge_text(v, einheit));
        lines.push(cl);
    }
    for x in ex {
        let e = x.row.element;
        let mut rl = Line::new(Kind::Row, 2, super::elem_key(e));
        rl.storey = skey;
        rl.group = gkey;
        rl.elements = vec![e];
        let mut c = x.row.number.clone();
        for zusatz in [x.geschoss.clone(), typ(m, x.row)].into_iter().flatten() {
            c.push_str(" · ");
            c.push_str(&zusatz);
        }
        rl.cells[0] = c;
        if x.row.q.is_none() {
            rl.cells[4] = "–".into();
            rl.note = x.row.note.clone();
        }
        lines.push(rl);
        for q in &x.mengen {
            let k = Key::ExtMenge(e.index(), e.generation(), q.satz as u16);
            let mut cl = Line::new(Kind::Control, 3, k);
            cl.storey = skey;
            cl.group = gkey;
            cl.elements = vec![e];
            cl.cells[0] = q.name.clone();
            cl.cells[spalte(&q.einheit)] =
                q.wert.map_or("–".into(), |v| menge_text(v, &q.einheit));
            lines.push(cl);
        }
    }
}

fn mengen(r: &RowQto) -> &[ExtMenge] {
    match &r.q {
        Some(ElementQto::Ext(x)) => &x.mengen,
        _ => &[],
    }
}

/// Nach Geschoss: die Gruppe `g` eines Bauteils im Geschoss `st`.
pub(super) fn gruppe(m: &Model, st: StoreyId, skey: Key, g: &GroupQto, lines: &mut Vec<Line>) {
    let Some(key) = g.ext.as_deref() else {
        return;
    };
    let c = EXT_BIT | stelle(m, key);
    let gkey = Key::Group(st.index(), g.category as u8, c);
    let ex: Vec<Exemplar> = g
        .rows
        .iter()
        .map(|r| Exemplar {
            row: r,
            mengen: mengen(r).iter().collect(),
            geschoss: None,
        })
        .collect();
    let s = st.index();
    block(m, key, skey, gkey, |q| Key::ExtSumme(s, c, q), &ex, lines);
}

/// Gewerk, unter dem ein Exemplar ohne Mengen steht: das des Bauteils.
fn gewerk_bauteil(m: &Model, key: &str) -> Option<TradeId> {
    let d = m.ext_def(key)?;
    m.trade_by_code(d.def.bauteil_feld("gewerk")?)
}

/// Gewerke der Erweiterungen eines Gebäudes, in Reihenfolge des ersten
/// Vorkommens; `None` für Mengen ohne Gewerk im Projekt.
pub(super) fn gewerke(m: &Model, b: &BuildingQto) -> Vec<Option<TradeId>> {
    let mut out = Vec::new();
    for g in b.storeys.iter().flat_map(|s| &s.groups) {
        let Some(key) = g.ext.as_deref() else {
            continue;
        };
        for r in &g.rows {
            let ts: Vec<Option<TradeId>> = match &r.q {
                Some(ElementQto::Ext(x)) => x.mengen.iter().map(|q| q.gewerk).collect(),
                _ => vec![gewerk_bauteil(m, key)],
            };
            for t in ts {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

/// Nach Gewerk: je Bauteil die Exemplare des Gebäudes `b` mit ihren Mengen
/// im Gewerk `t` (`None`: ohne Gewerk).
pub(super) fn gewerk_zeilen(
    m: &Model,
    b: &BuildingQto,
    t: Option<TradeId>,
    tkey: Key,
    lines: &mut Vec<Line>,
) {
    let tk = match tkey {
        Key::Trade(bi, order) => (bi << 16) | (order & 0xffff),
        _ => 0,
    };
    let mut je: Vec<(&str, Vec<Exemplar>)> = Vec::new();
    for st in &b.storeys {
        for g in &st.groups {
            let Some(key) = g.ext.as_deref() else {
                continue;
            };
            for r in &g.rows {
                let ms: Vec<&ExtMenge> = mengen(r).iter().filter(|q| q.gewerk == t).collect();
                let ohne = r.q.is_none() && gewerk_bauteil(m, key) == t;
                if ms.is_empty() && !ohne {
                    continue;
                }
                let x = Exemplar {
                    row: r,
                    mengen: ms,
                    geschoss: Some(storey_name(m, st.id).1),
                };
                match je.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, v)) => v.push(x),
                    None => je.push((key, vec![x])),
                }
            }
        }
    }
    je.sort_by_key(|(k, _)| (titel(m, k).to_lowercase(), k.to_string()));
    for (key, mut ex) in je {
        ex.sort_by(|a, b| a.row.number.cmp(&b.row.number));
        let c = EXT_BIT | stelle(m, key);
        let gkey = Key::ExtGewerk(tk, c);
        block(m, key, tkey, gkey, |q| Key::ExtSumme(tk, c, q), &ex, lines);
        if let Some(h) = lines.iter_mut().rev().find(|l| l.key == tkey) {
            for x in &ex {
                if !h.elements.contains(&x.row.element) {
                    h.elements.push(x.row.element);
                }
            }
        }
    }
}

/// CSV nach Geschoss: Zeilen der Gruppe `g` mit den Spalten Gebäude,
/// Geschoss, Kostengruppe, Bauteil, Nr., Länge, Stück, Fläche, Volumen,
/// Hinweis. t und kg stehen als Text im Hinweis.
pub(super) fn csv_geschoss(
    m: &Model,
    gb: &str,
    sname: &str,
    g: &GroupQto,
    row: &mut dyn FnMut([&str; 10]),
) {
    let Some(key) = g.ext.as_deref() else {
        return;
    };
    let (titel, name) = (titel(m, key), name(m, key));
    let kg = m
        .ext_def(key)
        .and_then(|d| d.kg())
        .map_or(String::new(), |k| k.to_string());
    let ex: Vec<Exemplar> = g
        .rows
        .iter()
        .map(|r| Exemplar {
            row: r,
            mengen: mengen(r).iter().collect(),
            geschoss: None,
        })
        .collect();
    let range = number_range(m, g);
    let anzahl = g.total.count.to_string();
    row([
        gb,
        sname,
        &kg,
        &format!("{titel} (Summe)"),
        &range,
        "",
        &anzahl,
        "",
        "",
        "",
    ]);
    for (_, mname, einheit, wert) in summen(&ex) {
        let z = csv_menge(einheit, wert);
        let bauteil = format!("{titel}: {mname}");
        row([
            gb, sname, &kg, &bauteil, &range, &z[0], &z[1], &z[2], &z[3], &z[4],
        ]);
    }
    for x in &ex {
        let bauteil = match typ(m, x.row) {
            Some(t) if t.starts_with(&name) => t,
            Some(t) => format!("{name} {t}"),
            None => name.clone(),
        };
        let note = x.row.note.clone().unwrap_or_default();
        let v = if x.row.q.is_none() { "–" } else { "" };
        row([
            gb,
            sname,
            &kg,
            &bauteil,
            &x.row.number,
            "",
            "1",
            "",
            v,
            &note,
        ]);
        for q in &x.mengen {
            let z = csv_menge(&q.einheit, q.wert);
            let kg = q.kg.map_or(String::new(), |k| k.to_string());
            let bauteil = format!("{name}: {}", q.name);
            row([
                gb,
                sname,
                &kg,
                &bauteil,
                &x.row.number,
                &z[0],
                &z[1],
                &z[2],
                &z[3],
                &z[4],
            ]);
        }
    }
}

/// Länge, Stück, Fläche, Volumen, Hinweis einer Menge für die .csv.
fn csv_menge(einheit: &str, wert: Option<f64>) -> [String; 5] {
    let mut z: [String; 5] = Default::default();
    let Some(v) = wert else {
        z[4] = "Formel rechnet nicht".into();
        return z;
    };
    match einheit {
        "m" => z[0] = super::csv_num(v),
        "stk" => z[1] = super::csv_num(v),
        "m2" => z[2] = super::csv_num(v),
        "m3" => z[3] = super::csv_num(v),
        e => z[4] = menge_text(v, e),
    }
    z
}

/// CSV nach Gewerk: die Mengen der Erweiterungen im Gewerk `t` mit den
/// Spalten Gebäude, Gewerk, Kostengruppe, Bauteil, Nr., Geschoss, Länge,
/// Fläche, Volumen, Hinweis; Stück, t und kg als Text im Hinweis. Ein
/// Zwischentitel davor sagt, dass sie nicht in der Summe des Gewerks stehen
/// (Review 3cl b).
pub(super) fn csv_gewerk(
    m: &Model,
    b: &BuildingQto,
    gb: &str,
    gewerk: &str,
    t: Option<TradeId>,
    row: &mut dyn FnMut([&str; 10]),
) {
    let mut titel = true;
    for st in &b.storeys {
        let sname = storey_name(m, st.id).0;
        for g in &st.groups {
            let Some(key) = g.ext.as_deref() else {
                continue;
            };
            let name = name(m, key);
            for r in &g.rows {
                for q in mengen(r).iter().filter(|q| q.gewerk == t) {
                    if std::mem::take(&mut titel) {
                        let t = "Erweiterungen, nicht in der Summe";
                        row([gb, gewerk, "", t, "", "", "", "", "", ""]);
                    }
                    let kg = q.kg.map_or(String::new(), |k| k.to_string());
                    let z = csv_menge(&q.einheit, q.wert);
                    let hinweis = match (q.einheit.as_str(), q.wert) {
                        ("stk", Some(v)) => menge_text(v, "stk"),
                        _ => z[4].clone(),
                    };
                    row([
                        gb,
                        gewerk,
                        &kg,
                        &format!("{name}: {}", q.name),
                        &r.number,
                        &sname,
                        &z[0],
                        &z[2],
                        &z[3],
                        &hinweis,
                    ]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{csv_grouped, Grouping, ListView};
    use crate::scene::Scene;
    use sk_model::erweiterung::{ExtDef, ExtPart};
    use sk_model::{Model, StoreyId};

    const STUETZE: &str = include_str!("../../crates/sk-szb/beispiele/werk.stuetze.szb");
    const PLATTE: &str = include_str!("../../crates/sk-szb/beispiele/werk.bodenplatte.szb");
    const GELAENDER: &str = include_str!("../../crates/sk-szb/beispiele/werk.stabgelaender.szb");

    fn eg(m: &Model) -> StoreyId {
        m.storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap()
    }

    /// Zwei Stützen, eine Bodenplatte und ein Geländer im EG.
    fn szene() -> Scene {
        let mut m = Model::with_seed(3);
        m.add_building(1);
        for t in [STUETZE, PLATTE, GELAENDER] {
            m.put_ext_def(ExtDef::lesen(t).unwrap()).unwrap();
        }
        let s = eg(&m);
        for (key, x) in [
            ("werk.stuetze", 0.0),
            ("werk.stuetze", 3000.0),
            ("werk.bodenplatte", 0.0),
            ("werk.stabgelaender", 0.0),
        ] {
            let p = ExtPart::new(m.ext_def(key).unwrap(), [x, 0.0]);
            m.add_ext(s, p).unwrap();
        }
        Scene::with_model(m)
    }

    fn zeile(t: &[String], p: &str) -> usize {
        t.iter()
            .position(|l| l.trim_start().starts_with(p))
            .unwrap_or_else(|| panic!("{p}: {t:#?}"))
    }

    /// Abnahmetabelle im Mengenfenster: Gruppe je Bauteil, Summe je Menge,
    /// aufgeklappt je Exemplar; Spalte nach Einheit.
    #[test]
    fn nach_geschoss() {
        let mut s = szene();
        let v = ListView::grouped(&mut s, Grouping::Storey);
        let t = v.line_texts();
        let g = zeile(&t, "Stahlbetonstützen | ST-001 … 002 | 2");
        assert_eq!(t[g + 1].trim_start(), "Beton C25/30 |  |  |  | 0,304 m³");
        assert_eq!(t[g + 2].trim_start(), "Schalung Stütze |  |  | 5,06 m² | ");
        assert_eq!(t[g + 3].trim_start(), "Betonstahl B500 |  |  |  | 0,046 t");
        assert_eq!(t[g + 4].trim_start(), "Stütze |  | 2 Stk |  | ");
        assert!(
            t[g + 5].trim_start().starts_with("ST-001 · Stütze 24/24"),
            "{t:#?}"
        );
        assert_eq!(t[g + 6].trim_start(), "Beton C25/30 |  |  |  | 0,152 m³");
        let p = zeile(&t, "Bodenplatten | BP-001 | 1");
        assert_eq!(t[p + 1].trim_start(), "Beton C25/30 |  |  |  | 4,800 m³");
        assert_eq!(t[p + 2].trim_start(), "Randschalung |  | 20,00 m |  | ");
        assert_eq!(t[p + 3].trim_start(), "Betonstahl B500 |  |  |  | 0,384 t");
        let l = zeile(&t, "Stabgeländer | GL-001 | 1");
        assert_eq!(t[l + 1].trim_start(), "Geländer |  | 3,00 m |  | ");
        assert_eq!(t[l + 2].trim_start(), "Pfosten |  | 4 Stk |  | ");
        // Gruppen nach Mehrzahl, nach den eigenen Bauteilarten
        assert!(p < l && l < g, "{t:#?}");
    }

    /// Nach Gewerk: jede Menge unter ihrem Gewerk, Exemplare mit Geschoss.
    #[test]
    fn nach_gewerk_und_csv() {
        let mut s = szene();
        let v = ListView::grouped(&mut s, Grouping::Trade);
        let t = v.line_texts();
        let beton = zeile(&t, "Betonarbeiten [DIN 18331]");
        let metall = zeile(&t, "Metallbauarbeiten [DIN 18360]");
        let st = zeile(&t, "Stahlbetonstützen | ST-001 … 002 | 2");
        let gl = zeile(&t, "Stabgeländer | GL-001 | 1");
        assert!(beton < st && st < metall && metall < gl, "{t:#?}");
        assert!(
            t[st + 5]
                .trim_start()
                .starts_with("ST-001 · EG · Stütze 24/24"),
            "{t:#?}"
        );

        let m = s.model().clone();
        let sched = sk_model::qto::schedule(&m);
        let c = String::from_utf8(csv_grouped(&m, &sched, Grouping::Storey)).unwrap();
        assert!(
            c.contains("Stahlbetonstützen (Summe);ST-001 … 002;;2;;;"),
            "{c}"
        );
        assert!(
            c.contains("Stahlbetonstützen: Beton C25/30;ST-001 … 002;;;;0,3036;"),
            "{c}"
        );
        assert!(
            c.contains("Stahlbetonstütze: Betonstahl B500;ST-001;;;;;0,023 t"),
            "{c}"
        );
        assert!(c.contains("Stabgeländer: Pfosten;GL-001;;4,0000;;;"), "{c}");
        assert!(c.contains(";322;Bodenplatte 20 cm;BP-001;;1;;;"), "{c}");
        let c = String::from_utf8(csv_grouped(&m, &sched, Grouping::Trade)).unwrap();
        // Zwischentitel je Gewerk vor den Erweiterungszeilen (Review 3cl b)
        let titel = |g: &str| format!(";{g};;Erweiterungen, nicht in der Summe;;;;;;\r\n");
        assert_eq!(c.matches(&titel("18331 Betonarbeiten")).count(), 1, "{c}");
        let i = c
            .find(&titel("18360 Metallbauarbeiten"))
            .expect("Titel Metallbau");
        assert!(i < c.find("Stabgeländer: Geländer").unwrap(), "{c}");
        assert!(
            c.contains(
                "18360 Metallbauarbeiten;359;Stabgeländer: Geländer;GL-001;Erdgeschoss;3,0000;;;"
            ),
            "{c}"
        );
        assert!(
            c.contains("Stahlbetonstütze: Stütze;ST-002;Erdgeschoss;;;;1 Stk"),
            "{c}"
        );
    }
}
