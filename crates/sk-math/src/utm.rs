//! Abbildung UTM auf dem Ellipsoid GRS80 (ETRS89, EPSG:25832/25833) für die
//! Georeferenz im IFC (ifc/plan.md §10): Krüger-Reihen bis n⁶ nach Karney,
//! „Transverse Mercator with an accuracy of a few nanometers“ (J. Geodesy
//! 85, 2011), auf weniger als einen Millimeter genau in einer Zone.
//!
//! Winkel in Grad, Ost- und Nordwert in m (Ostwert ohne Zonenkennzahl,
//! Nordhalbkugel).

/// Große Halbachse und Abplattung von GRS80.
const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_222_101;
/// Maßstab am Mittelmeridian und falscher Ostwert.
const K0: f64 = 0.9996;
const FE: f64 = 500_000.0;

struct Reihe {
    /// Rektifizierender Radius.
    a: f64,
    /// Exzentrizität.
    e: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
}

fn reihe() -> Reihe {
    let n = F / (2.0 - F);
    let (n2, n3) = (n * n, n * n * n);
    let (n4, n5, n6) = (n3 * n, n3 * n2, n3 * n3);
    Reihe {
        a: A / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0),
        e: (F * (2.0 - F)).sqrt(),
        alpha: [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0 - 127.0 * n5 / 288.0
                + 7891.0 * n6 / 37800.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0 + 281.0 * n5 / 630.0
                - 1_983_433.0 * n6 / 1_935_360.0,
            61.0 * n3 / 240.0 - 103.0 * n4 / 140.0
                + 15061.0 * n5 / 26880.0
                + 167_603.0 * n6 / 181_440.0,
            49561.0 * n4 / 161_280.0 - 179.0 * n5 / 168.0 + 6_601_661.0 * n6 / 7_257_600.0,
            34729.0 * n5 / 80640.0 - 3_418_889.0 * n6 / 1_995_840.0,
            212_378_941.0 * n6 / 319_334_400.0,
        ],
        beta: [
            n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0 - n4 / 360.0 - 81.0 * n5 / 512.0
                + 96199.0 * n6 / 604_800.0,
            n2 / 48.0 + n3 / 15.0 - 437.0 * n4 / 1440.0 + 46.0 * n5 / 105.0
                - 1_118_711.0 * n6 / 3_870_720.0,
            17.0 * n3 / 480.0 - 37.0 * n4 / 840.0 - 209.0 * n5 / 4480.0 + 5569.0 * n6 / 90720.0,
            4397.0 * n4 / 161_280.0 - 11.0 * n5 / 504.0 - 830_251.0 * n6 / 7_257_600.0,
            4583.0 * n5 / 161_280.0 - 108_847.0 * n6 / 3_991_680.0,
            20_648_693.0 * n6 / 638_668_800.0,
        ],
    }
}

/// Mittelmeridian der Zone in Grad (Zone 32: 9° O).
pub fn mittelmeridian(zone: u8) -> f64 {
    zone as f64 * 6.0 - 183.0
}

/// Konforme Breite: τ' aus τ = tan φ.
fn tau_strich(tau: f64, e: f64) -> f64 {
    let s = (e * (e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
    tau * (1.0 + s * s).sqrt() - s * (1.0 + tau * tau).sqrt()
}

/// Gauß-Krüger-Koordinaten ohne Maßstab und Ostwert: (ξ, η) und die
/// Meridiankonvergenz in Bogenmaß.
fn vorwaerts(r: &Reihe, lat: f64, dlon: f64) -> (f64, f64, f64) {
    let t = tau_strich(lat.tan(), r.e);
    let xi1 = t.atan2(dlon.cos());
    let eta1 = (dlon.sin() / (t * t + dlon.cos().powi(2)).sqrt()).asinh();
    let (mut xi, mut eta, mut p, mut q) = (xi1, eta1, 1.0, 0.0);
    for (j, a) in r.alpha.iter().enumerate() {
        let k = 2.0 * (j + 1) as f64;
        xi += a * (k * xi1).sin() * (k * eta1).cosh();
        eta += a * (k * xi1).cos() * (k * eta1).sinh();
        p += k * a * (k * xi1).cos() * (k * eta1).cosh();
        q += k * a * (k * xi1).sin() * (k * eta1).sinh();
    }
    let gamma = (t / (1.0 + t * t).sqrt() * dlon.tan()).atan() + q.atan2(p);
    (xi, eta, gamma)
}

/// Ost- und Nordwert (m) eines Punkts (Breite, Länge in Grad) in der Zone.
pub fn nach_utm(lat: f64, lon: f64, zone: u8) -> (f64, f64) {
    let r = reihe();
    let (xi, eta, _) = vorwaerts(
        &r,
        lat.to_radians(),
        (lon - mittelmeridian(zone)).to_radians(),
    );
    (FE + K0 * r.a * eta, K0 * r.a * xi)
}

/// Breite und Länge (Grad) zu Ost- und Nordwert (m) in der Zone.
pub fn von_utm(e: f64, n: f64, zone: u8) -> (f64, f64) {
    let r = reihe();
    let xi = n / (K0 * r.a);
    let eta = (e - FE) / (K0 * r.a);
    let (mut xi1, mut eta1) = (xi, eta);
    for (j, b) in r.beta.iter().enumerate() {
        let k = 2.0 * (j + 1) as f64;
        xi1 -= b * (k * xi).sin() * (k * eta).cosh();
        eta1 -= b * (k * xi).cos() * (k * eta).sinh();
    }
    let t1 = xi1.sin() / (eta1.sinh().powi(2) + xi1.cos().powi(2)).sqrt();
    let dlon = eta1.sinh().atan2(xi1.cos());
    // τ aus τ' nach Newton (Karney Gl. 19–21), wenige Schritte
    let mut tau = t1;
    for _ in 0..8 {
        let ts = tau_strich(tau, r.e);
        let d = (t1 - ts) / (1.0 + ts * ts).sqrt() * (1.0 + (1.0 - r.e * r.e) * tau * tau)
            / ((1.0 - r.e * r.e) * (1.0 + tau * tau).sqrt());
        tau += d;
        if d.abs() < 1e-15 {
            break;
        }
    }
    (
        tau.atan().to_degrees(),
        mittelmeridian(zone) + dlon.to_degrees(),
    )
}

/// Meridiankonvergenz in Grad: um so viel liegt Gitter-Nord im
/// Uhrzeigersinn von geografisch Nord (östlich des Mittelmeridians positiv).
pub fn konvergenz(lat: f64, lon: f64, zone: u8) -> f64 {
    let r = reihe();
    vorwaerts(
        &r,
        lat.to_radians(),
        (lon - mittelmeridian(zone)).to_radians(),
    )
    .2
    .to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Punkte mit Werten aus PROJ 9.8 (ETRS89 → EPSG:25832/25833), dazu die
    /// Meridiankonvergenz: Ganderkesee, Brandenburger Tor, Allgäu, Rügen.
    const PUNKTE: [(u8, f64, f64, f64, f64, f64); 4] = [
        (
            32,
            53.0589,
            8.591,
            472_589.2907,
            5_878_900.8945,
            -0.326_896_805_251_699_1,
        ),
        (
            33,
            52.516275,
            13.377704,
            389_918.0415,
            5_819_699.1323,
            -1.287_462_780_321_350_8,
        ),
        (
            32,
            47.421,
            10.985,
            649_718.7832,
            5_253_860.1103,
            1.461_915_293_602_200_6,
        ),
        (
            33,
            54.5,
            14.9,
            493_523.8480,
            6_039_158.8273,
            -0.081_411_579_906_350_16,
        ),
    ];

    #[test]
    fn bekannte_punkte_hin_und_zurueck() {
        for (zone, lat, lon, e, n, gamma) in PUNKTE {
            let (ee, nn) = nach_utm(lat, lon, zone);
            assert!(
                (ee - e).abs() < 1e-3 && (nn - n).abs() < 1e-3,
                "{lat} {lon}: {ee} {nn}"
            );
            let (b, l) = von_utm(e, n, zone);
            // 1e-8° sind weniger als 1,1 mm
            assert!(
                (b - lat).abs() < 1e-8 && (l - lon).abs() < 1e-8,
                "{e} {n}: {b} {l}"
            );
            let g = konvergenz(lat, lon, zone);
            assert!((g - gamma).abs() < 1e-7, "{lat} {lon}: {g} statt {gamma}");
        }
    }

    #[test]
    fn mittelmeridian_ohne_konvergenz() {
        let (e, _) = nach_utm(51.0, 9.0, 32);
        assert!((e - FE).abs() < 1e-6);
        assert!(konvergenz(51.0, 9.0, 32).abs() < 1e-12);
        assert_eq!(mittelmeridian(33), 15.0);
    }
}
