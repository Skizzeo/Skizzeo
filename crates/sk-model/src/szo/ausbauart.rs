//! Ausgeblendete Ausbauarten der Räume in der `.szo` ([`crate::view`],
//! Jörn 16.10.): `[hide] finish=screed`, eine Zeile je Art, nur wo eine
//! ausgeblendet ist; additiv zu den übrigen `[hide]`-Zeilen. Eine Zeile mit
//! einer Art, die diese Fassung nicht kennt, gibt einen Hinweis und bleibt
//! roh stehen (Regel 72).

use super::{Line, Record};
use crate::{Model, TypeCategory};
use std::collections::BTreeSet;

pub(super) fn write(m: &Model, out: &mut String) {
    for a in &m.visibility().hidden_finish {
        Line::new("hide").word("finish", a.word()).finish(out);
    }
    for raw in &m.view_hide.hide_raw {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Die `[hide]`-Zeilen mit `finish=`: die Arten und die rohen Zeilen.
pub(super) fn read(
    recs: &[Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> (BTreeSet<TypeCategory>, Vec<String>) {
    let mut arten = BTreeSet::new();
    let mut roh = Vec::new();
    for r in recs {
        let Some(w) = r.opt("finish") else { continue };
        match TypeCategory::FINISHES.into_iter().find(|c| c.word() == w) {
            Some(a) => {
                arten.insert(a);
            }
            None => {
                r.skip();
                roh.push(lines[r.line - 1].to_string());
                hints.push(format!(
                    "Zeile {}: [hide] Ausbauart „{w}“ unbekannt; sie blendet nichts aus und bleibt stehen",
                    r.line
                ));
            }
        }
    }
    (arten, roh)
}
