//! Zeichnet Schnitt B–B durch die Nordwand eines Gebäudes aus dem Modell
//! (Prüfbild für den Geometriekern, G7 K4): das OG springt vor (30 cm und
//! 10 cm, Decke, Untersichtdämmung und herabgezogene Dämmung) oder zurück
//! (30 cm, nichts wächst). Schreibt daneben das Gebäude mit 30 cm Vorsprung
//! als `.szo` für den Bildvergleich in 3D.
//! `cargo run -p sk-model --example vorsprung_svg -- datei.svg [datei.szo]`

use sk_math::vec3;
use sk_model::{edge_kind, material, merge_seam, Model, RunId, Solid};
use std::fmt::Write;

/// Gebäude 10 × 8 m, OG-Nordwand um `out` nach außen.
fn gebaeude(out: f64) -> (Model, RunId, RunId) {
    let mut m = Model::with_seed(71);
    m.allow_unstepped();
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    let eg = m.build_from_polygon(b, &pts).unwrap();
    let og = m.runs_above(eg)[0];
    let c = m.chain(og).unwrap();
    let c = c.with_segment_moved(1, c.outward_sign() * out).unwrap();
    m.set_run_points(og, &c.points).unwrap();
    (m, eg, og)
}

/// Schnittflächen in der Ebene x = 5000 (Blick nach Westen).
fn section(out: f64) -> Vec<Solid> {
    let (m, eg, og) = gebaeude(out);
    let (p0, n) = (vec3(5000.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
    let (w0, w1) = (m.chain(eg).unwrap(), m.chain(og).unwrap());
    let (mut c0, mut c1) = (w0.section_caps(p0, n), w1.section_caps(p0, n));
    merge_seam(&mut c0, &mut c1, w1.base);
    let f0 = m.floor(eg).unwrap().unwrap();
    let f1 = m.floor(og).unwrap().unwrap();
    vec![c0, c1, f0.section_caps(p0, n), f1.section_caps(p0, n)]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("vorsprung.svg".into());
    if let Some(szo) = args.get(2) {
        let (m, _, _) = gebaeude(300.0);
        std::fs::write(szo, sk_model::szo::write(&m)).unwrap();
    }
    // Ausschnitt je Bild: Nordwand, y 7000 … 9000, z 1300 … 4000
    let (y0, y1, z0, z1) = (6900.0, 8900.0, 1300.0, 4000.0);
    let s = 0.2; // px je mm
    let (bw, h) = ((y1 - y0) * s, (z1 - z0) * s);
    let w = 3.0 * bw + 40.0;
    let mut svg = String::new();
    writeln!(svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{}" viewBox="0 0 {w} {}">
<defs>
<pattern id="beton" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="6" stroke="#000" stroke-width="0.6"/><line x1="3" y1="0" x2="3" y2="6" stroke="#000" stroke-width="0.6" stroke-dasharray="1.5 1.5"/></pattern>
<pattern id="mw" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="7" stroke="#000" stroke-width="0.6"/></pattern>
<pattern id="daemm" width="10" height="10" patternUnits="userSpaceOnUse"><path d="M0,5 q2.5,-5 5,0 t5,0" fill="none" stroke="#666" stroke-width="0.6"/></pattern>
</defs>
<rect width="100%" height="100%" fill="#fff"/>"##, h + 24.0, h + 24.0).unwrap();
    for (k, (offset, label)) in [
        (300.0, "OG 30 cm vor"),
        (100.0, "OG 10 cm vor (weniger als Dämmdicke)"),
        (-300.0, "OG 30 cm zurück"),
    ]
    .into_iter()
    .enumerate()
    {
        let dx = k as f64 * (bw + 20.0);
        let py = |y: f64| (y - y0) * s + dx;
        let pz = |z: f64| (z1 - z) * s;
        writeln!(
            svg,
            r#"<clipPath id="c{k}"><rect x="{dx}" y="0" width="{bw}" height="{h}"/></clipPath><g clip-path="url(#c{k})">"#
        )
        .unwrap();
        let sols = section(offset);
        let m = gebaeude(0.0).0;
        // Erst alle Flächen, dann alle Kanten (sonst verdecken Flächen Linien)
        for sol in &sols {
            for t in &sol.triangles {
                let cat = m
                    .material_by_key(t.mat & !material::CUT)
                    .map(|x| x.category);
                let fill = match cat {
                    Some(sk_model::MatCategory::Concrete) => "url(#beton)",
                    Some(sk_model::MatCategory::Insulation) => "url(#daemm)",
                    Some(_) => "url(#mw)",
                    None => "#e8e8e8",
                };
                let pts: Vec<String> =
                    t.p.iter()
                        .map(|q| format!("{:.1},{:.1}", py(q.y), pz(q.z)))
                        .collect();
                writeln!(
                    svg,
                    r#"<polygon points="{}" fill="{fill}" stroke="none"/>"#,
                    pts.join(" ")
                )
                .unwrap();
            }
        }
        for sol in &sols {
            for e in &sol.edges {
                let sw = match e.kind {
                    edge_kind::CUT => 2.0,
                    edge_kind::CUT_LAYER => 1.0,
                    _ => 0.35,
                };
                writeln!(svg, r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#000" stroke-width="{sw}" stroke-linecap="square"/>"##,
                    py(e.a.y), pz(e.a.z), py(e.b.y), pz(e.b.z)).unwrap();
            }
        }
        writeln!(svg, "</g>").unwrap();
        writeln!(svg, r##"<text x="{}" y="{}" font-family="sans-serif" font-size="13" fill="#555">{label}</text>"##, dx + 8.0, h + 18.0).unwrap();
    }
    writeln!(svg, "</svg>").unwrap();
    std::fs::write(&out, svg).unwrap();
    println!("{out}");
}
