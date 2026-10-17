//! Zahlen im Mengenblatt: Dezimalkomma, Tausenderpunkt, kaufmännisch
//! gerundet, mit Einheit. Ausgelagert, damit `schedule_view.rs` unter
//! 3.000 Zeilen bleibt (R4).

/// Zahl mit Dezimalkomma und Tausenderpunkt.
pub fn de(v: f64, dec: usize) -> String {
    // Schneller Weg über ganze Zahlen (Review M1: eine Zelle kostete mit
    // `format!` und drei Zwischentexten ein Mehrfaches); bis 10^15 Einheiten
    // der letzten Stelle ergibt er dieselben Ziffern wie der lange Weg
    let f = 10f64.powi(dec as i32);
    let y = v * f;
    let n = (y + y.signum() * 1e-6f64.max(y.abs() * 1e-15)).round();
    if dec > 9 || n.is_nan() || n.abs() >= 1e15 {
        return de_lang(v, dec);
    }
    let n = n as i64;
    let teiler = 10u64.pow(dec as u32);
    let (int, frac) = (n.unsigned_abs() / teiler, n.unsigned_abs() % teiler);
    let ziffern = int.to_string();
    let mut out = String::with_capacity(ziffern.len() * 4 / 3 + dec + 4);
    if n < 0 {
        out.push('−');
    }
    for (i, ch) in ziffern.chars().enumerate() {
        if i > 0 && (ziffern.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if dec > 0 {
        out.push(',');
        let f = frac.to_string();
        out.extend(std::iter::repeat_n('0', dec - f.len()));
        out.push_str(&f);
    }
    out
}

/// [`de`] über `format!`, für sehr große Zahlen und NaN.
pub(super) fn de_lang(v: f64, dec: usize) -> String {
    let v = kaufmaennisch(v, dec);
    let neg = v < 0.0 && (v * 10f64.powi(dec as i32)).round() != 0.0;
    let s = format!("{:.*}", dec, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s.clone(), None),
    };
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if neg {
        out.push('−');
    }
    out.push_str(&grouped);
    if let Some(f) = frac {
        out.push(',');
        out.push_str(&f);
    }
    out
}

/// Kaufmännisch auf `dec` Stellen: Die Hälfte rundet vom Nullpunkt weg,
/// auch wenn die Rechnung sie als Binärzahl knapp darunter ablegt
/// (4,5675 → 4,568, 23,815 → 23,82; Raumbuch RB5 mit Innenwänden, deren
/// Achse auf halben Millimetern liegt). Die Toleranz ist fest in Einheiten
/// der letzten Stelle, nur so groß wie das Rechenrauschen großer Zahlen
/// (Review #170 H-170a: 236.000,12341 bleibt 236.000,1234).
pub fn kaufmaennisch(v: f64, dec: usize) -> f64 {
    let f = 10f64.powi(dec as i32);
    let y = v * f;
    (y + y.signum() * 1e-6f64.max(y.abs() * 1e-15)).round() / f
}

pub(super) fn m_len(mm: f64) -> String {
    format!("{} m", de(mm / 1e3, 2))
}

pub(super) fn m_area(mm2: f64) -> String {
    format!("{} m²", de(mm2 / 1e6, 2))
}

pub(super) fn m_vol(mm3: f64) -> String {
    format!("{} m³", de(mm3 / 1e9, 3))
}
