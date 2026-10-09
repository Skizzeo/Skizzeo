//! Zeilen der Kostenansicht aus dem Kostenblatt: Zahlen formatieren und die
//! Gliederung aus `Kostenblatt::aufteilung` in Zeilen legen. Rechnet kein
//! Geld (Review 3ai).

use super::{Art, Gliederung, Modus, Zeile};
use crate::umfang_view;
use sk_cost::gliederung::{Gruppe, Schluessel, Teilung};
use sk_cost::katalog::{Einheit, Katalog};
use sk_cost::rechnung::{Ansatz, OhneHerkunft, Position, Quelle};
use sk_cost::{Cent, Dez, Kostenblatt};
use sk_model::trade::TradeId;
use sk_model::{ElementId, Guid, Model, StoreyId};
use std::collections::HashSet;

// --- Zahlen ------------------------------------------------------------------

/// Tausenderpunkte vor eine Ziffernfolge.
pub(crate) fn tausender(ziffern: &str) -> String {
    let mut s = String::new();
    for (i, c) in ziffern.chars().enumerate() {
        if i > 0 && (ziffern.len() - i).is_multiple_of(3) {
            s.push('.');
        }
        s.push(c);
    }
    s
}

/// Menge auf 3 Stellen mit Einheit: „1.172,224 m²“.
pub fn menge_text(d: Dez, e: Einheit) -> String {
    let milli = d.0 / 1000;
    let neg = milli < 0;
    let a = milli.unsigned_abs();
    format!(
        "{}{},{:03} {}",
        if neg { "−" } else { "" },
        tausender(&(a / 1000).to_string()),
        a % 1000,
        e.zeichen()
    )
}

/// Betrag mit zwei Stellen: „60.089,83“.
pub(crate) fn euro(c: Cent) -> String {
    c.deutsch()
}

/// Betrag auf ganze Euro mit Zeichen: „60.090 €“ (Karten, Chips).
pub fn euro_ganz(c: Cent) -> String {
    let e = (c.0 + if c.0 < 0 { -50 } else { 50 }) / 100;
    let neg = e < 0;
    format!(
        "{}{} €",
        if neg { "−" } else { "" },
        tausender(&e.unsigned_abs().to_string())
    )
}

/// Prozent ganzzahlig, kaufmännisch: `teil / ganz`.
pub(crate) fn prozent(teil: Cent, ganz: Cent) -> Option<i64> {
    (ganz.0 > 0).then(|| (teil.0 * 200 + ganz.0) / (2 * ganz.0))
}

// --- Zeilen bauen ------------------------------------------------------------

/// Name und DIN-Nummer eines Gewerks; ohne Gewerk „Ohne Gewerk“.
pub(crate) fn gewerk_name(m: &Model, g: Option<Guid>) -> (String, String, u16) {
    match g.and_then(|g| m.trade(TradeId(g))) {
        Some(t) => (t.name.clone(), format!("DIN {}", t.code), t.order),
        None => ("Ohne Gewerk".into(), String::new(), u16::MAX),
    }
}

/// Name eines Geschosses in der Gliederung („Gründung“, „Erdgeschoss“).
pub(crate) fn geschoss_name(m: &Model, s: StoreyId) -> String {
    m.storey(s).map_or_else(String::new, |x| x.name.clone())
}

/// Kostengruppe mit Namen: „322 Flachgründungen und Bodenplatten“.
pub(crate) fn kg_name(kg: Option<u16>) -> String {
    match kg {
        Some(k) => match sk_cost::din276::name(k) {
            Some(n) => format!("{k} {n}"),
            None => k.to_string(),
        },
        None => "Ohne Kostengruppe".into(),
    }
}

/// Kurzname einer Bauleistung für „geschätzt nach …“ (Namen statt
/// Kennungen, Bedienbarkeit 4.9).
pub(super) fn leistung_name(k: &Katalog, g: Guid) -> String {
    k.leistung(g).map_or_else(String::new, |l| l.kurz.clone())
}

/// Schlüssel einer Position zum Aufklappen (über Gliederungen gleich).
pub(super) fn pos_key(p: &Position, teil: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let quelle = match &p.quelle {
        Quelle::Leistung(g) => (1u8, g.0),
        Quelle::Geschaetzt(g) => (2, g.0),
        Quelle::Richtpreis(g) => (3, g.0),
    };
    for b in quelle
        .0
        .to_le_bytes()
        .iter()
        .chain(&quelle.1.to_le_bytes())
        .chain(p.kurz.as_bytes())
        .chain(&teil.to_le_bytes())
    {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Alles, was die Zeilen brauchen.
pub(super) struct Bau<'a> {
    m: &'a Model,
    k: &'a Katalog,
    b: &'a Kostenblatt,
    modus: Modus,
    offen: &'a HashSet<u64>,
}

impl Bau<'_> {
    /// Zeile einer Position mit der Menge `menge` (ganz oder Teil) aus den
    /// Ansatzzeilen `ansatz`; darunter, wenn offen, der Mengenansatz.
    fn position(
        &self,
        out: &mut Vec<Zeile>,
        t: &sk_cost::gliederung::Zeile,
        ansatz: &[&Ansatz],
        teil: u64,
        ebene: u8,
        gruppe: &str,
    ) {
        let (i, menge) = (t.pos, t.menge);
        let p = &self.b.positionen[i];
        let key = pos_key(p, teil);
        let offen = self.offen.contains(&key);
        let mut z = Zeile::neu(Art::Position { offen }, ebene, p.kurz.clone());
        z.key = key;
        z.gruppe = gruppe.to_string();
        z.menge = menge_text(menge, p.einheit);
        z.pos = Some((i, menge));
        z.geschaetzt = !matches!(p.quelle, Quelle::Leistung(_));
        z.leise = match &p.quelle {
            Quelle::Geschaetzt(g) => format!("geschätzt nach {}", leistung_name(self.k, *g)),
            Quelle::Richtpreis(_) => "geschätzt, nur Material".into(),
            Quelle::Leistung(_) => String::new(),
        };
        let mut els: Vec<ElementId> = Vec::new();
        for a in ansatz {
            if !els.contains(&a.element) {
                els.push(a.element);
            }
        }
        z.elements = els;
        // Betrag aus dem Kostenblatt; ohne Betrag (NU bei „nur Material“)
        // steht „NU-Preis“
        match t.betrag {
            Some(gp) => {
                let ep = if self.modus.nur_material() {
                    p.stoff
                } else {
                    p.ep
                };
                z.ep = euro(ep);
                z.gp = euro(gp);
            }
            None => z.ep = "NU-Preis".into(),
        }
        z.betrag = t.betrag;
        out.push(z);
        if offen {
            self.ansatz(out, p, ansatz, ebene + 1, gruppe);
        }
    }

    /// Mengenansatz: je Geschoss und Herkunft die Bauteilnummern und die
    /// Menge, Herkunft „aus Modell“ bzw. „aus {Bauleistung}“.
    fn ansatz(
        &self,
        out: &mut Vec<Zeile>,
        p: &Position,
        ansatz: &[&Ansatz],
        ebene: u8,
        gruppe: &str,
    ) {
        let mut teile: Vec<(StoreyId, Option<Guid>, Vec<&Ansatz>)> = Vec::new();
        for a in ansatz {
            match teile.iter_mut().find(|t| t.0 == a.geschoss && t.1 == a.aus) {
                Some(t) => t.2.push(a),
                None => teile.push((a.geschoss, a.aus, vec![a])),
            }
        }
        for (st, aus, v) in teile {
            let mut nummern: Vec<&str> = Vec::new();
            for a in &v {
                if !nummern.contains(&a.nummer.as_str()) {
                    nummern.push(&a.nummer);
                }
            }
            let kurz = self.m.storey(st).map_or(String::new(), |s| s.short.clone());
            let mut z = Zeile::neu(
                Art::Ansatz,
                ebene,
                format!("{kurz} · {}", nummern.join(", ")),
            );
            z.gruppe = gruppe.to_string();
            z.leise = match aus {
                Some(g) => format!("aus {}", leistung_name(self.k, g)),
                None => "aus Modell".into(),
            };
            let teil = Position {
                ansatz: v.iter().map(|a| (*a).clone()).collect(),
                ..p.clone()
            };
            let menge = teil.teile(|_| ()).first().map_or(Dez::NULL, |t| t.1);
            z.menge = menge_text(menge, p.einheit);
            z.elements = v.iter().map(|a| a.element).collect();
            out.push(z);
        }
    }

    /// Zeilen ohne Bauleistung (grau, Menge, kein Preis).
    fn ohne(&self, out: &mut Vec<Zeile>, welche: &[usize], ebene: u8, gruppe: &str) {
        for (&j, o) in welche.iter().map(|j| (j, &self.b.ohne[*j])) {
            let text = match &o.herkunft {
                OhneHerkunft::Schicht { baustoff, .. } => {
                    let name = self.m.element(o.element).map_or(String::new(), |e| {
                        sk_model::kinds::spec(e.category).name.to_string()
                    });
                    // „Dachterrasse · Dämmung hart“: Bauteilart und Baustoff,
                    // ohne Bauteilnummer; „ohne Bauleistung“ steht einmal an
                    // der Gruppe (Einstellungen §3 KA-2 Punkt 4)
                    let baustoff = baustoff_name(self.m, *baustoff);
                    if baustoff.is_empty() {
                        name
                    } else {
                        format!("{name} · {baustoff}")
                    }
                }
                // „Stahlbetonstütze · Schalung Stütze (Einheit m² passt
                // nicht zur Bauleistung in m)“ (E8b)
                OhneHerkunft::Erweiterung {
                    key, name, grund, ..
                } => {
                    let bauteil = self.m.ext_def(key).map_or(key.clone(), |d| {
                        sk_model::erweiterung::anzeige(d.name(), 60)
                    });
                    format!("{bauteil} · {name} ({grund})")
                }
            };
            let mut z = Zeile::neu(Art::Ohne, ebene, text);
            z.gruppe = gruppe.to_string();
            z.menge = menge_text(o.menge, o.einheit);
            z.elements = vec![o.element];
            z.ohne = Some(j);
            out.push(z);
        }
    }

    /// Name und leiser Zusatz einer Gruppe.
    fn gruppe_name(&self, k: Schluessel) -> (String, String) {
        match k {
            Schluessel::Gewerk(g) => {
                let (name, din, _) = gewerk_name(self.m, g);
                (name, din)
            }
            Schluessel::Geschoss(st) => (geschoss_name(self.m, st), String::new()),
            Schluessel::Kg2(kg) | Schluessel::Kg(kg) => (kg_name(kg), String::new()),
        }
    }

    /// Gruppenzeile mit der Summe aus dem Kostenblatt, darunter Untergruppen,
    /// (Teil-)Zeilen und Zeilen ohne Bauleistung. `teil`: Schlüssel der
    /// Teilung zum Aufklappen; `csv`: Gliederung für die CSV-Spalte, wenn
    /// eine Gruppe darüber sie vorgibt (Geschoss).
    fn gruppe(&self, out: &mut Vec<Zeile>, g: &Gruppe, ebene: u8, teil: u64, csv: Option<&str>) {
        let (text, leise) = self.gruppe_name(g.schluessel);
        let teil = match g.schluessel {
            Schluessel::Geschoss(st) => st_key(st),
            Schluessel::Kg(kg) => 1 << 32 | u64::from(kg.unwrap_or(0)),
            _ => teil,
        };
        let name = csv.unwrap_or(&text).to_string();
        let csv_kinder = match g.schluessel {
            Schluessel::Geschoss(_) => Some(name.as_str()),
            _ => csv,
        };
        let mut kinder = Vec::new();
        for u in &g.gruppen {
            self.gruppe(&mut kinder, u, ebene + 1, teil, csv_kinder);
        }
        for z in &g.zeilen {
            let p = &self.b.positionen[z.pos];
            let a: Vec<&Ansatz> = z.ansatz.iter().map(|&j| &p.ansatz[j]).collect();
            self.position(&mut kinder, z, &a, teil, ebene + 1, &name);
        }
        self.ohne(&mut kinder, &g.ohne, ebene + 1, &name);
        let mut z = Zeile::neu(Art::Gruppe, ebene, text);
        z.leise = leise;
        z.betrag = g.summe;
        z.gp = match g.summe {
            Some(c) => euro(c),
            None if g.zeilen.is_empty() && g.gruppen.is_empty() => "ohne Bauleistung".into(),
            None => String::new(),
        };
        let mut els: Vec<ElementId> = Vec::new();
        for k in &kinder {
            for e in &k.elements {
                if !els.contains(e) {
                    els.push(*e);
                }
            }
        }
        z.elements = els;
        out.push(z);
        out.extend(kinder);
    }

    /// Alle Zeilen der Gliederung, am Ende der „Rundungsausgleich“, wenn
    /// das Kostenblatt einen hat.
    fn alle(&self, t: Teilung) -> Vec<Zeile> {
        let a = self.b.aufteilung(self.m, t, self.modus.nur_material());
        let mut out = Vec::new();
        for g in &a.gruppen {
            self.gruppe(&mut out, g, 0, 0, None);
        }
        if a.ausgleich != Cent(0) {
            let mut z = Zeile::neu(Art::Ausgleich, 0, "Rundungsausgleich".into());
            z.leise = "Teilmengen je auf 3 Stellen gerundet".into();
            z.gp = euro(a.ausgleich);
            z.betrag = Some(a.ausgleich);
            out.push(z);
        }
        out
    }
}

/// Schlüssel eines Geschosses für das Aufklappen von Teilzeilen.
pub(super) fn st_key(st: StoreyId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    st.hash(&mut h);
    h.finish()
}

/// Die Zeilen der Liste: rein aus Modell (Namen), Katalog (Namen),
/// Kostenblatt, Gliederung, Modus und aufgeklappten Positionen.
pub fn zeilen(
    m: &Model,
    k: &Katalog,
    b: &Kostenblatt,
    g: Gliederung,
    modus: Modus,
    offen: &HashSet<u64>,
) -> Vec<Zeile> {
    let bau = Bau {
        m,
        k,
        b,
        modus,
        offen,
    };
    bau.alle(match g {
        Gliederung::Gewerk => Teilung::Gewerk,
        Gliederung::Geschoss => Teilung::Geschoss,
        Gliederung::Kostengruppe => Teilung::Kostengruppe,
    })
}

/// Summe je Chip (Reiter Kosten, auch abgewählt) aus dem Kostenblatt ohne
/// Abwahl, im Modus.
pub fn chip_summen(b: &Kostenblatt, chips: &[umfang_view::Chip], modus: Modus) -> Vec<Cent> {
    chips
        .iter()
        .map(|c| b.summe_geschosse(&c.geschosse, modus.nur_material()))
        .collect()
}

/// Name eines Baustoffs im Modell, sonst leer.
pub(super) fn baustoff_name(m: &sk_model::Model, g: sk_model::Guid) -> String {
    m.materials()
        .iter()
        .find(|(_, x)| x.guid == g)
        .map_or(String::new(), |(_, x)| x.name.clone())
}
