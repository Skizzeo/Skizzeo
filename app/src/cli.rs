//! Sichtbarkeit von der Befehlszeile (Paket 3, Bildvergleiche):
//! `--ausblenden <Nummer|art=Wort|gewerk=Code>[,…]`, `--isolieren …` (gleiche
//! Werte), `--gelaende-aus`. Andere Angaben bleiben unberührt. `art=` nennt
//! auch eine Ausbauart der Räume (`art=screed`), die sich nur ausblenden
//! lässt.

use sk_model::view::{Isolate, Visibility};
use sk_model::{Category, Model};
use std::collections::BTreeSet;

/// Ein Wert hinter `--ausblenden` bzw. `--isolieren`.
enum Item {
    Element(sk_model::Guid),
    Category(Category),
    Trade(sk_model::TradeId),
    Ausbau(sk_model::TypeCategory),
}

fn item(m: &Model, s: &str) -> Result<Item, String> {
    if let Some(w) = s.strip_prefix("art=") {
        return Category::ALL
            .iter()
            .copied()
            .find(|c| sk_model::kinds::spec(*c).szo == w)
            .map(Item::Category)
            .or_else(|| {
                let mut a = sk_model::TypeCategory::FINISHES.into_iter();
                a.find(|a| a.word() == w).map(Item::Ausbau)
            })
            .ok_or_else(|| format!("Unbekannte Bauteilart: {w}"));
    }
    if let Some(c) = s.strip_prefix("gewerk=") {
        return m
            .trade_by_code(c)
            .map(Item::Trade)
            .ok_or_else(|| format!("Unbekanntes Gewerk: {c}"));
    }
    m.elements()
        .iter()
        .find(|(_, e)| e.number == s)
        .map(|(_, e)| Item::Element(e.guid))
        .ok_or_else(|| format!("Unbekannte Bauteilnummer: {s}"))
}

/// Sichtbarkeit aus den Angaben `args` für das Modell `m`. Ein unbekannter
/// Wert ist ein Fehler (Text für die Meldung); ohne Angaben alles sichtbar.
pub fn visibility_args(args: &[&str], m: &Model) -> Result<Visibility, String> {
    let mut v = Visibility::default();
    let mut it = args.iter();
    while let Some(&a) = it.next() {
        match a {
            "--gelaende-aus" => v.terrain_hidden = true,
            "--ausblenden" | "--isolieren" => {
                let list = it.next().ok_or_else(|| format!("{a}: Wert fehlt"))?;
                let items = list
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| item(m, s))
                    .collect::<Result<Vec<_>, _>>()?;
                if a == "--ausblenden" {
                    for i in items {
                        match i {
                            Item::Element(g) => v.hidden.insert(g),
                            Item::Category(c) => v.hidden_cat.insert(c),
                            Item::Trade(t) => v.hidden_trade.insert(t),
                            Item::Ausbau(a) => v.hidden_finish.insert(a),
                        };
                    }
                } else {
                    v.isolate = Some(isolate(items)?);
                }
            }
            _ => {}
        }
    }
    Ok(v)
}

/// Isoliert wird ein Ast: Bauteile (auch mehrere), eine Art oder ein Gewerk.
fn isolate(items: Vec<Item>) -> Result<Isolate, String> {
    let mut els = BTreeSet::new();
    let mut one = None;
    for i in items {
        match i {
            Item::Element(g) => {
                els.insert(g);
            }
            Item::Category(c) => one = Some(Isolate::Category(c)),
            Item::Trade(t) => one = Some(Isolate::Trade(t)),
            Item::Ausbau(a) => {
                return Err(format!(
                    "--isolieren: Ausbauart {} nur ausblenden",
                    a.word()
                ))
            }
        }
    }
    match (one, els.is_empty()) {
        (Some(o), true) => Ok(o),
        (None, false) => Ok(Isolate::Elements(els)),
        (None, true) => Err("--isolieren: Wert fehlt".into()),
        (Some(_), false) => Err("--isolieren: Bauteile oder eine Art oder ein Gewerk".into()),
    }
}
