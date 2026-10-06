//! Zeichnet Schnitt A–A durch Wand, Sohlplatte und Frostschürze als SVG
//! (Prüfbild für den Geometriekern): `cargo run -p sk-model --example gruendung_svg -- datei.svg [rücksprung]`

use sk_math::vec3;
use sk_model::{
    edge_kind, material, Foundation, FoundationParams, Layer, RefSide, Solid, WallChain,
};
use std::fmt::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("gruendung.svg".into());
    let recess: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let wall = WallChain {
        points: vec![
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ],
        closed: true,
        ref_side: RefSide::Left,
        layers: vec![Layer::new(140.0, 2), Layer::core(175.0, 1)],
        height: 1200.0,
        joints: Default::default(),
    };
    let p = FoundationParams {
        recess,
        slab_mat: 7,
        footing_mat: 7,
        ..FoundationParams::default()
    };
    let f = Foundation::from_chain(&wall, &p).unwrap();
    let (p0, n) = (vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
    let (slab, foot) = f.section_caps(p0, n);
    let walls = wall.section_caps(p0, n);
    // Ausschnitt: linke Außenecke, x −400 … 1600, z −1000 … 1200
    let (x0, x1, z0, z1) = (-400.0, 1600.0, -1000.0, 1200.0);
    let s = 0.4; // px je mm
    let (w, h) = ((x1 - x0) * s, (z1 - z0) * s);
    let px = |x: f64| (x - x0) * s;
    let pz = |z: f64| (z1 - z) * s;
    let mut svg = String::new();
    writeln!(svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<defs>
<pattern id="beton" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="6" stroke="#000" stroke-width="0.6"/><line x1="3" y1="0" x2="3" y2="6" stroke="#000" stroke-width="0.6" stroke-dasharray="1.5 1.5"/></pattern>
<pattern id="mw" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="7" stroke="#000" stroke-width="0.6"/></pattern>
</defs>
<rect width="100%" height="100%" fill="#fff"/>"##).unwrap();
    let mut draw = |sol: &Solid| {
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
        for e in &sol.edges {
            let sw = match e.kind {
                edge_kind::CUT => 2.0,
                edge_kind::CUT_LAYER => 1.0,
                _ => 0.35,
            };
            writeln!(svg, r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#000" stroke-width="{sw}" stroke-linecap="square"/>"##,
                px(e.a.x), pz(e.a.z), px(e.b.x), pz(e.b.z)).unwrap();
        }
    };
    draw(&walls);
    draw(&slab);
    draw(&foot);
    writeln!(svg, r##"<text x="8" y="{}" font-family="sans-serif" font-size="13" fill="#555">Schnitt A–A, Sockelrücksprung {recess} mm</text></svg>"##, h - 10.0).unwrap();
    std::fs::write(&out, svg).unwrap();
    println!(
        "{out}: Platte {:.2} m², Schürze {:.2} m",
        f.slab_area() / 1e6,
        f.footing_axis_length() / 1e3
    );
}
