//! Schalter „Ausbau innen“ je Ansicht in der `.szo` (Raumbuch RB4,
//! [`crate::FINISH_VIEWS`]): `[viewfinish] view=plan on=1`, nur wo er an
//! ist, nach den Raumvorlagen. Eine Zeile, die nicht zählt, gibt einen
//! Hinweis und bleibt roh stehen (Regel 72), wie `[viewbelow]`.

use super::{Line, Record};
use crate::{Model, FINISH_VIEWS};

pub(super) fn write(m: &Model, out: &mut String) {
    for (i, name) in FINISH_VIEWS.iter().enumerate() {
        if m.view_finish(i) {
            Line::new("viewfinish")
                .word("view", name)
                .flag("on", true)
                .finish(out);
        }
    }
    for raw in m.view_finish_raw() {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Schalter je Ansicht und die Zeilen, die nicht zählen (roh).
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> ([bool; 3], Vec<String>) {
    let mut an = [false; 3];
    let mut gesehen = [false; 3];
    let mut roh = Vec::new();
    for r in recs {
        let view = r
            .opt("view")
            .and_then(|v| FINISH_VIEWS.iter().position(|n| *n == v));
        let on = r.opt("on").and_then(|_| r.flag("on").ok());
        match (view, on) {
            (Some(i), Some(o)) if !gesehen[i] => {
                gesehen[i] = true;
                an[i] = o;
            }
            _ => {
                r.skip();
                roh.push(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [viewfinish] unlesbar oder doppelt; es gilt die erste lesbare Zeile der Ansicht, sonst aus; die Zeile bleibt",
                    r.line
                ));
            }
        }
    }
    (an, roh)
}

#[cfg(test)]
mod tests {
    use crate::szo::{read, write};
    use crate::{GuidGen, Model};

    /// Ohne Schalter keine Zeile, alte Dateien bytegleich, keine neue
    /// Revision; an je Ansicht, Unlesbares bleibt roh mit Hinweis.
    #[test]
    fn ausbau_innen_in_der_datei() {
        let mut m = Model::with_seed(1);
        let alt = write(&m);
        assert!(!alt.contains("[viewfinish]"));
        let rev = m.revision();
        m.set_view_finish(2, true);
        m.set_view_finish(0, true);
        assert_eq!(m.revision(), rev, "Ansichtszustand");
        let t = write(&m);
        let zeilen: Vec<&str> = t
            .lines()
            .filter(|z| z.starts_with("[viewfinish]"))
            .collect();
        assert_eq!(
            zeilen,
            ["[viewfinish] view=plan on=1", "[viewfinish] view=3d on=1"]
        );
        let l = read(&t, GuidGen::with_seed(2)).unwrap();
        assert!(l.model.view_finish(0) && !l.model.view_finish(1) && l.model.view_finish(2));
        assert_eq!(write(&l.model), t);
        // unlesbar und doppelt: roh mit Hinweis, die erste lesbare gilt
        let kaputt = t.replace(
            "[viewfinish] view=3d on=1",
            "[viewfinish] view=3d on=1\n[viewfinish] view=3d on=0\n[viewfinish] view=keller on=1",
        );
        let l = read(&kaputt, GuidGen::with_seed(3)).unwrap();
        assert!(l.model.view_finish(2));
        assert_eq!(
            l.hints
                .iter()
                .filter(|h| h.contains("[viewfinish]"))
                .count(),
            2,
            "{:?}",
            l.hints
        );
        assert_eq!(write(&l.model), kaputt);
        // Umschalten nimmt die rohe Zeile dieser Ansicht weg
        let mut m = l.model;
        m.set_view_finish(2, false);
        let t = write(&m);
        assert!(!t.contains("view=3d"), "{t}");
        assert!(t.contains("view=keller"));
    }
}
