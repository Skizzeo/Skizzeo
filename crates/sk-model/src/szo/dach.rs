//! Flachdach in der `.szo`: Gefälle und Abläufe am `[roof]`, Attikadämmung
//! an der `[floor]`. Werte außerhalb des Bereichs gelten mit Hinweis als
//! Vorgabe, das Projekt öffnet (Regel 114).

use super::{ausserhalb, LoadError, Record};

pub(super) fn read_drainage(
    r: &Record,
    hints: &mut Vec<String>,
) -> Result<crate::gefaelle::Drainage, LoadError> {
    // Regel 114: außerhalb des Bereichs mit Hinweis die Vorgabe (eben),
    // unlesbare Abläufe fallen mit Hinweis weg; das Projekt öffnet
    let mut slope = match r.opt("slope") {
        Some(_) => r.f64("slope")?,
        None => 0.0,
    };
    if !(slope.is_finite() && (0.0..=100.0).contains(&slope)) {
        hints.push(ausserhalb(r.line, "Gefälle", slope, 0.0, 100.0, " %"));
        slope = 0.0;
    }
    let mut drains = Vec::new();
    for p in r
        .opt("drains")
        .unwrap_or("")
        .split(';')
        .filter(|t| !t.is_empty())
    {
        let xy: Vec<f64> = p.split(',').filter_map(|t| t.parse::<f64>().ok()).collect();
        match xy[..] {
            [x, y] if x.is_finite() && y.is_finite() => drains.push(sk_math::vec3(x, y, 0.0)),
            _ => {
                hints.push(format!("Zeile {}: Abläufe unlesbar, ohne Abläufe", r.line));
                drains.clear();
                break;
            }
        }
    }
    Ok(crate::gefaelle::Drainage { slope, drains })
}

/// Dicke der Attikadämmung `k` an `[floor]`: fehlt sie, gilt die Vorgabe
/// (vor der Attikadämmung); 0 oder `lo` … `hi` wie im Dialog, sonst mit
/// Hinweis ebenfalls die Vorgabe (Regel 114).
pub(super) fn attika_wert(
    r: &Record,
    k: &str,
    name: &str,
    vorgabe: f64,
    (lo, hi): (f64, f64),
    hints: &mut Vec<String>,
) -> Result<f64, LoadError> {
    if r.opt(k).is_none() {
        return Ok(vorgabe);
    }
    let v = r.f64(k)?;
    if v == 0.0 || (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        hints.push(ausserhalb(r.line, name, v, lo, hi, " mm"));
        Ok(vorgabe)
    }
}

/// Je Regenfallrohr (Jörn 11.10.) ein `[downpipe]`: Dachaufbau, Nummer
/// des Ablaufs, abweichende Eigenschaften.
pub(super) fn write_downpipes(
    m: &crate::model::Model,
    els: &[&crate::element::Element],
    out: &mut String,
) {
    for e in els {
        let crate::element::ElementKind::Downpipe { roof, drain, spec } = e.kind else {
            continue;
        };
        let l = super::Line::new("downpipe")
            .guid("guid", Some(e.guid))
            .guid("roof", m.element(roof).map(|x| x.guid))
            .text("number", &e.number)
            .word("cat", super::category(e.category))
            .num("drain", drain);
        write_downpipe_spec(l, &spec)
            .num("seq", e.seq)
            .guid("storey", m.storey(e.storey).map(|s| s.guid))
            .finish(out);
    }
}

/// Eigenschaften eines `[downpipe]` (Regenfallrohr, Jörn 11.10.), nur wenn
/// sie von der Vorgabe abweichen: `overflow=spout`, `control=0`,
/// `spacing=` in mm.
fn write_downpipe_spec(line: super::Line, s: &crate::fallrohr::DownpipeSpec) -> super::Line {
    use crate::fallrohr::{Overflow, SPACING};
    let mut line = line;
    if s.overflow == Overflow::Spout {
        line = line.word("overflow", "spout");
    }
    if !s.control {
        line = line.flag("control", false);
    }
    if s.spacing != SPACING {
        line = line.num("spacing", s.spacing);
    }
    line
}

/// Liest die Eigenschaften eines `[downpipe]`; Unbekanntes und Werte
/// außerhalb des Bereichs gelten mit Hinweis als Vorgabe (Regel 114).
pub(super) fn read_downpipe_spec(
    r: &Record,
    hints: &mut Vec<String>,
) -> Result<crate::fallrohr::DownpipeSpec, LoadError> {
    use crate::fallrohr::{DownpipeSpec, Overflow, SPACING_MAX, SPACING_MIN};
    let mut s = DownpipeSpec::default();
    match r.opt("overflow") {
        None | Some("pipe") => {}
        Some("spout") => s.overflow = Overflow::Spout,
        Some(w) => hints.push(format!(
            "Zeile {}: Notentwässerung „{w}“ unbekannt, Notfallrohr",
            r.line
        )),
    }
    if r.opt("control").is_some() {
        s.control = r.flag("control")?;
    }
    if r.opt("spacing").is_some() {
        let v = r.f64("spacing")?;
        if (SPACING_MIN..=SPACING_MAX).contains(&v) {
            s.spacing = v;
        } else {
            hints.push(ausserhalb(
                r.line,
                "Schellenabstand",
                v,
                SPACING_MIN,
                SPACING_MAX,
                " mm",
            ));
        }
    }
    Ok(s)
}
