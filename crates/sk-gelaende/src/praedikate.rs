//! Exakte Lagetests für das Netz. Die Punkte liegen auf einem Raster von
//! 0,1 mm ([`RASTER`]) als ganze Zahlen; Orientierung und Umkreistest
//! rechnen in `i128` ohne Rundung. So ist das Netz auf jedem Rechner gleich
//! und kippt nicht bei fast gleichen oder fast kollinearen Punkten.

/// Rasterpunkte je Meter.
pub const RASTER: f64 = 10_000.0;

/// Größter Abstand vom Bezugspunkt in Rasterpunkten (25 km). Darunter
/// bleiben alle Zwischenwerte des Umkreistests unter 2¹²⁶.
pub const WEIT: i64 = 250_000_000;

/// Punkt auf dem Raster.
pub type Ip = [i64; 2];

/// Meter (relativ zum Bezugspunkt) auf das Raster; `None` jenseits von
/// [`WEIT`] oder bei nicht endlichen Werten.
pub fn raster(x: f64, y: f64) -> Option<Ip> {
    let r = |v: f64| {
        let v = (v * RASTER).round();
        (v.is_finite() && v.abs() <= WEIT as f64).then_some(v as i64)
    };
    Some([r(x)?, r(y)?])
}

/// Vorzeichen der Fläche von a, b, c: positiv, wenn c links von a → b liegt.
pub fn orient(a: Ip, b: Ip, c: Ip) -> i128 {
    let (ax, ay) = (a[0] as i128, a[1] as i128);
    let (bx, by) = (b[0] as i128, b[1] as i128);
    let (cx, cy) = (c[0] as i128, c[1] as i128);
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

/// Positiv, wenn d echt im Umkreis des gegen den Uhrzeigersinn laufenden
/// Dreiecks a, b, c liegt, null auf dem Kreis.
pub fn umkreis(a: Ip, b: Ip, c: Ip, d: Ip) -> i128 {
    let f = |p: Ip| ((p[0] - d[0]) as i128, (p[1] - d[1]) as i128);
    let (adx, ady) = f(a);
    let (bdx, bdy) = f(b);
    let (cdx, cdy) = f(c);
    let al = adx * adx + ady * ady;
    let bl = bdx * bdx + bdy * bdy;
    let cl = cdx * cdx + cdy * cdy;
    adx * (bdy * cl - cdy * bl) - ady * (bdx * cl - cdx * bl) + al * (bdx * cdy - cdx * bdy)
}

/// Die offenen Strecken p–q und r–s kreuzen sich echt (kein gemeinsamer
/// Endpunkt, keine Berührung).
pub fn kreuzen(p: Ip, q: Ip, r: Ip, s: Ip) -> bool {
    let sg = |v: i128| v.signum();
    sg(orient(p, q, r)) * sg(orient(p, q, s)) < 0 && sg(orient(r, s, p)) * sg(orient(r, s, q)) < 0
}

/// c liegt echt zwischen a und b auf ihrer Strecke.
pub fn auf_strecke(a: Ip, b: Ip, c: Ip) -> bool {
    if orient(a, b, c) != 0 {
        return false;
    }
    let d = |p: Ip, q: Ip| {
        (q[0] - p[0]) as i128 * (q[0] - p[0]) as i128
            + (q[1] - p[1]) as i128 * (q[1] - p[1]) as i128
    };
    let (ab, ac, bc) = (d(a, b), d(a, c), d(b, c));
    ac > 0 && bc > 0 && ac < ab && bc < ab
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grenzfaelle_exakt() {
        let a = [0, 0];
        let b = [WEIT, 0];
        // fast kollinear, 1 Rasterpunkt daneben auf 25 km
        assert!(orient(a, b, [WEIT / 2, 1]) > 0);
        assert!(orient(a, b, [WEIT / 2, -1]) < 0);
        assert_eq!(orient(a, b, [WEIT / 3, 0]), 0);
        // Kreis durch vier Punkte eines Quadrats: null, knapp drinnen positiv
        let q = [[-WEIT, -WEIT], [WEIT, -WEIT], [WEIT, WEIT]];
        assert_eq!(umkreis(q[0], q[1], q[2], [-WEIT, WEIT]), 0);
        assert!(umkreis(q[0], q[1], q[2], [-WEIT + 1, WEIT]) > 0);
        assert!(umkreis(q[0], q[1], q[2], [-WEIT - 1, WEIT]) < 0);
        assert!(kreuzen([0, 0], [10, 10], [0, 10], [10, 0]));
        assert!(!kreuzen([0, 0], [10, 10], [5, 5], [10, 0]), "Berührung");
        assert!(auf_strecke([0, 0], [10, 10], [5, 5]));
        assert!(!auf_strecke([0, 0], [10, 10], [10, 10]));
        assert_eq!(raster(1.23456, -0.00004), Some([12346, 0]));
        assert_eq!(raster(30_000.0, 0.0), None);
    }
}
