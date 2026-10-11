//! Schnitthöhe des Grundrisses in der `.szo` ([`crate::hoehen`]):
//! `[plancut] h=1500`, nur wenn sie von der Vorgabe abweicht. Eine Zeile,
//! die nicht zählt, gibt einen Hinweis und bleibt roh stehen (Regel 72).

use super::{ausserhalb, Line, LoadError, Record};
use crate::hoehen::{MAX_PLAN_CUT, MIN_PLAN_CUT};
use crate::Model;

pub(super) fn write_plan_cut(m: &Model, out: &mut String) {
    if let Some(h) = m.plan_cut_own() {
        Line::new("plancut").num("h", h).finish(out);
    } else if let Some(raw) = m.plan_cut_raw() {
        out.push_str(raw);
        out.push('\n');
    }
}

/// Eigene Schnitthöhe und eine Zeile, die nicht zählt (roh).
pub(super) fn read_plan_cut(
    recs: &[Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<(Option<f64>, Option<String>), LoadError> {
    match recs {
        [] => Ok((None, None)),
        [r] => {
            let h = r.opt("h").and_then(|t| t.parse::<f64>().ok());
            match h {
                Some(h) if h.is_finite() && (MIN_PLAN_CUT..=MAX_PLAN_CUT).contains(&h) => {
                    Ok((Some(h), None))
                }
                _ => {
                    r.skip();
                    let grund = match h {
                        Some(v) => {
                            ausserhalb(r.line, "Schnitthöhe", v, MIN_PLAN_CUT, MAX_PLAN_CUT, " mm")
                        }
                        None => format!("Zeile {}: [plancut] unlesbar, Vorgabe gilt", r.line),
                    };
                    hints.push(format!("{grund}; die Zeile bleibt"));
                    Ok((None, Some(lines[r.line - 1].to_string())))
                }
            }
        }
        [_, r, ..] => Err(super::err(r.line, "[plancut] doppelt")),
    }
}

#[cfg(test)]
mod tests {
    use crate::szo::{read, write};
    use crate::{GuidGen, Model};

    fn load(t: &str) -> crate::szo::Loaded {
        read(t, GuidGen::with_seed(99)).unwrap()
    }

    /// Jörn 11.10. 14:05: Schnitthöhe nur abweichend eine Zeile, alte Dateien
    /// bytegleich, keine neue Revision; Unlesbares bleibt roh mit Hinweis.
    #[test]
    fn schnitthoehe_in_der_datei() {
        let mut m = Model::with_seed(1);
        let alt = write(&m);
        assert!(!alt.contains("[plancut]"));
        let rev = m.revision();
        m.set_plan_cut_height(1000.0);
        assert_eq!(m.revision(), rev, "Ansichtszustand");
        let neu = write(&m);
        assert!(neu.lines().any(|z| z == "[plancut] h=1000"), "{neu}");
        let back = load(&neu);
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.plan_cut_height(), 1000.0);
        assert_eq!(write(&back.model), neu);
        let mut m2 = back.model;
        m2.set_plan_cut_height(1500.0);
        assert_eq!(write(&m2), alt);
        for roh in ["[plancut] h=20", "[plancut] h=hoch", "[plancut]"] {
            let text = format!("{alt}{roh}\n");
            let l = load(&text);
            assert_eq!(l.hints.len(), 1, "{roh}: {:?}", l.hints);
            assert_eq!(l.model.plan_cut_height(), 1500.0, "{roh}");
            let neu = write(&l.model);
            assert!(neu.lines().any(|z| z == roh), "{roh}: Zeile bleibt roh");
            assert!(write(&load(&neu).model) == neu, "{roh}: stabil");
            let mut m3 = l.model;
            m3.set_plan_cut_height(1200.0);
            assert!(!write(&m3).lines().any(|z| z == roh), "{roh}: ersetzt");
        }
        let doppelt = format!("{alt}[plancut] h=1000\n[plancut] h=1100\n");
        assert!(read(&doppelt, GuidGen::with_seed(99)).is_err());
    }
}
