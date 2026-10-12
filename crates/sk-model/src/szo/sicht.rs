//! Schnelloption je Ansicht in der `.szo` ([`crate::view`], Jörn 12.10.):
//! `[viewhide] view=front cat=downpipe`, eine Zeile je Ansicht und Art, nur
//! wo eine Ansicht etwas ausblendet. Eine Zeile, die nicht zählt, gibt einen
//! Hinweis und bleibt roh stehen (Regel 72).

use super::{Line, Record};
use crate::view::{ViewHide, SICHT_VIEWS};
use crate::{Category, Model};

pub(super) fn write(m: &Model, out: &mut String) {
    for (i, name) in SICHT_VIEWS.iter().enumerate() {
        for c in m.view_hidden(i) {
            Line::new("viewhide")
                .word("view", name)
                .word("cat", crate::kinds::spec(*c).szo)
                .finish(out);
        }
    }
    for raw in &m.view_hide.raw {
        out.push_str(raw);
        out.push('\n');
    }
}

pub(super) fn read(recs: &[Record], lines: &[&str], hints: &mut Vec<String>) -> ViewHide {
    let mut v = ViewHide::default();
    for r in recs {
        let view = r
            .opt("view")
            .and_then(|w| SICHT_VIEWS.iter().position(|n| *n == w));
        let cat = r.opt("cat").and_then(|w| {
            Category::ALL
                .into_iter()
                .chain([Category::Extension])
                .find(|c| crate::kinds::spec(*c).szo == w)
        });
        match (view, cat) {
            (Some(i), Some(c)) => {
                v.cats[i].insert(c);
            }
            _ => {
                r.skip();
                v.raw.push(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [viewhide] unlesbar; die Ansicht zeigt alles, die Zeile bleibt",
                    r.line
                ));
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::super::tests::house;
    use super::super::{read, write};
    use crate::{Category, GuidGen};

    /// Jörn 12.10.: Ohne Schnelloption kein `[viewhide]`, alte Dateien
    /// bytegleich; je Ansicht und Art eine Zeile, Rundlauf bytegleich, keine
    /// neue Revision, die Sichtbarkeit des Baums bleibt. Unlesbares bleibt
    /// roh mit Hinweis, bis eine Wahl für diese Ansicht es ersetzt.
    #[test]
    fn schnelloption_je_ansicht_in_der_datei() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut m = house();
        let alt = write(&m);
        assert!(!alt.contains("[viewhide]"));
        let rev = m.revision();
        m.set_view_drain_hidden(0, true);
        m.set_view_drain_hidden(4, true);
        assert_eq!(m.revision(), rev, "Ansichtszustand");
        assert!(m.visibility().is_plain(), "der Baum bleibt");
        let neu = write(&m);
        let zeilen: Vec<&str> = neu
            .lines()
            .filter(|z| z.starts_with("[viewhide]"))
            .collect();
        assert_eq!(
            zeilen,
            [
                "[viewhide] view=persp cat=downpipe",
                "[viewhide] view=back cat=downpipe"
            ]
        );
        let back = load(&neu);
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert!(back.model.view_drain_hidden(0) && back.model.view_drain_hidden(4));
        assert!(!back.model.view_drain_hidden(1));
        assert!(!back
            .model
            .visibility()
            .hidden_cat
            .contains(&Category::Downpipe));
        assert_eq!(write(&back.model), neu);
        let mut m2 = back.model;
        m2.set_view_drain_hidden(0, false);
        m2.set_view_drain_hidden(4, false);
        assert_eq!(write(&m2), alt);
        for roh in [
            "[viewhide] view=oben cat=downpipe",
            "[viewhide] view=front cat=dachrinne",
            "[viewhide] view=front",
        ] {
            let text = alt.replacen("[storey]", &format!("{roh}\n[storey]"), 1);
            let mut l = load(&text);
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert!((0..7).all(|i| !l.model.view_drain_hidden(i)), "{roh}");
            assert!(write(&l.model).lines().any(|z| z == roh), "{roh}: roh");
            // Eine Wahl für Vorne ersetzt die unlesbare Zeile von Vorne
            l.model.set_view_drain_hidden(3, false);
            let raus = !roh.contains("view=front");
            assert_eq!(write(&l.model).lines().any(|z| z == roh), raus, "{roh}");
        }
    }
}
