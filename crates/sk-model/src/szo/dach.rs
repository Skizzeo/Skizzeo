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
