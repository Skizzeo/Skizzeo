//! Exportiert eine Projektdatei nach IFC4, ohne die App:
//! `cargo run -p sk-ifc --example szo2ifc -- haus.szo haus.ifc`

use sk_model::{szo, GuidGen};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (Some(ein), Some(aus)) = (a.get(1), a.get(2)) else {
        eprintln!("Aufruf: szo2ifc <haus.szo> <haus.ifc>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(ein).expect("Projektdatei lesen");
    let m = szo::read(&text, GuidGen::with_seed(1))
        .expect("Projektdatei deuten")
        .model;
    let name = std::path::Path::new(aus)
        .file_name()
        .map_or("skizzeo.ifc".into(), |n| n.to_string_lossy().into_owned());
    let opt = sk_ifc::Optionen {
        datei: name,
        ..Default::default()
    };
    let (ifc, b) = sk_ifc::export(&m, &opt);
    std::fs::write(aus, ifc).expect("IFC schreiben");
    let liste: Vec<String> = b
        .ausgelassen
        .iter()
        .map(|(nr, g)| format!("{nr} ({})", g.text()))
        .collect();
    println!(
        "{aus}: {} Gebäude, {} Geschosse, {} Bauteile, {} Wandzüge, ausgelassen: {}",
        b.gebaeude,
        b.geschosse,
        b.bauteile,
        b.baugruppen,
        liste.join(", ")
    );
}
