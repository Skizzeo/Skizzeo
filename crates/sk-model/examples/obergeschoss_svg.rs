//! Zeichnet Schnitt A–A durch EG- und OG-Außenwand mit beiden Decken als SVG
//! (Prüfbild für den Geometriekern, G6): links bündig (ohne Naht), rechts das
//! OG um 30 cm nach außen versetzt (Stufe).
//! `cargo run -p sk-model --example obergeschoss_svg -- datei.svg`

use sk_math::vec3;
use sk_model::{
    edge_kind, material, merge_seam, FloorParams, FloorSlab, Foundation, FoundationParams, Layer,
    RefSide, Solid, WallChain,
};
use std::fmt::Write;

const OK_EG: f64 = 2855.0;
const OK_OG: f64 = 5835.0;

fn with_floor(mut w: WallChain) -> (WallChain, FloorSlab) {
    let f = FloorSlab::from_chain(
        &w,
        &FloorParams {
            top: w.top(),
            thickness: 220.0,
            mat: 7,
        },
    )
    .unwrap();
    w.joints.slab_band = Some(f.band());
    (w, f)
}

/// Schnittflächen des Stapels in der Ebene y = 4000 (Blick nach Norden).
fn section(offset: f64) -> Vec<Solid> {
    let eg = WallChain {
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: vec![Layer::new(140.0, 2), Layer::core(175.0, 1)],
        base: 0.0,
        height: OK_EG,
        joints: Default::default(),
    };
    // Segment 0 ist die Westwand (x = 0)
    let og = eg.stacked(&[offset, 0.0, 0.0, 0.0], OK_EG, OK_OG).unwrap();
    let fd = Foundation::from_chain(
        &eg,
        &FoundationParams {
            slab_mat: 7,
            footing_mat: 7,
            ..FoundationParams::default()
        },
    )
    .unwrap();
    let ((w0, f0), (w1, f1)) = (with_floor(eg), with_floor(og));
    let (p0, n) = (vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
    let (mut c0, mut c1) = (w0.section_caps(p0, n), w1.section_caps(p0, n));
    merge_seam(&mut c0, &mut c1, OK_EG);
    let (slab, foot) = fd.section_caps(p0, n);
    vec![
        c0,
        c1,
        f0.section_caps(p0, n),
        f1.section_caps(p0, n),
        slab,
        foot,
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("obergeschoss.svg".into());
    // Ausschnitt je Bild: linke Außenwand, x −700 … 1300, z −1000 … 6200
    let (x0, x1, z0, z1) = (-700.0, 1300.0, -1000.0, 6200.0);
    let s = 0.12; // px je mm
    let (bw, h) = ((x1 - x0) * s, (z1 - z0) * s);
    let w = 2.0 * bw + 20.0;
    let mut svg = String::new();
    writeln!(svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{}" viewBox="0 0 {w} {}">
<defs>
<pattern id="beton" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="6" stroke="#000" stroke-width="0.6"/><line x1="3" y1="0" x2="3" y2="6" stroke="#000" stroke-width="0.6" stroke-dasharray="1.5 1.5"/></pattern>
<pattern id="mw" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="7" stroke="#000" stroke-width="0.6"/></pattern>
</defs>
<rect width="100%" height="100%" fill="#fff"/>"##, h + 24.0, h + 24.0).unwrap();
    for (k, (offset, label)) in [
        (0.0, "OG bündig: keine Naht"),
        (300.0, "OG 30 cm vor: Stufe"),
    ]
    .into_iter()
    .enumerate()
    {
        let dx = k as f64 * (bw + 20.0);
        let px = |x: f64| (x - x0) * s + dx;
        let pz = |z: f64| (z1 - z) * s;
        writeln!(
            svg,
            r#"<clipPath id="c{k}"><rect x="{dx}" y="0" width="{bw}" height="{h}"/></clipPath><g clip-path="url(#c{k})">"#
        )
        .unwrap();
        let sols = section(offset);
        // Erst alle Flächen, dann alle Kanten (sonst verdecken Flächen Linien)
        for sol in &sols {
            for t in &sol.triangles {
                let fill = match t.mat & !material::CUT {
                    7 => "url(#beton)",
                    1 => "url(#mw)",
                    _ => "#e8e8e8",
                };
                let pts: Vec<String> =
                    t.p.iter()
                        .map(|q| format!("{:.1},{:.1}", px(q.x), pz(q.z)))
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
                    px(e.a.x), pz(e.a.z), px(e.b.x), pz(e.b.z)).unwrap();
            }
        }
        writeln!(svg, "</g>").unwrap();
        writeln!(svg, r##"<text x="{}" y="{}" font-family="sans-serif" font-size="13" fill="#555">{label}</text>"##, dx + 8.0, h + 18.0).unwrap();
    }
    writeln!(svg, "</svg>").unwrap();
    std::fs::write(&out, svg).unwrap();
    println!("{out}");
}
