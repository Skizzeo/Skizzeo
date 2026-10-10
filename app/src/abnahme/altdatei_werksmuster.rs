use super::*;

// Abnahmetest A295: Altdateien ohne `[pattern]`-Satz behalten das
// Werksmuster (BIM-Befund zu cb8eda4, bim/befund-cb8eda4.md, Koordinator
// 20:06). Hat die Datei gar keinen `[pattern]`-Satz, gilt einmalig die
// Namenszuordnung; beim nächsten Speichern steht das Muster unter der alten
// Guid. Hat sie mindestens einen, gilt nur die Guid (Regel 60, A275).
//
// Einbau: als `mod altdatei_werksmuster { use super::*; … }` ans Ende von
// app/src/abnahme.rs, dazu `app/src/abnahme_p5.szo` und
// `app/src/abnahme_p6.szo` (Kopien von test/dateien/p5.szo und p6.szo,
// beide vor Paket 6a gespeichert). Ohne Adapter.
//
// Auf cb8eda4 geprüft: p6.szo ist nicht betroffen, weil es aus einem
// neuen Projekt stammt und seine Werks-Oberflächen schon die festen Guids
// tragen. Betroffen sind p5.szo, p5-1.szo, p3-ohne-wdvs.szo und jede
// Datei mit anderen Guids; darum prüft A295 p5.szo (heute rot) und
// p6.szo (muss grün bleiben).

use sk_model::proctex::Pattern;
use sk_model::SurfaceId;

const P5: &str = include_str!("../abnahme_p5.szo");
const P6: &str = include_str!("../abnahme_p6.szo");

fn lesen(t: &str) -> sk_model::szo::Loaded {
    sk_model::szo::read(t, sk_model::GuidGen::with_seed(1)).expect("öffnet")
}

fn flaeche(m: &Model, name: &str) -> SurfaceId {
    m.materials()
        .iter()
        .find(|(_, x)| x.name == name)
        .map(|(_, x)| x.surface)
        .unwrap_or_else(|| panic!("{name} fehlt"))
}

fn muster(m: &Model, s: SurfaceId) -> Option<Pattern> {
    m.attr().surface(s).unwrap().pattern.clone()
}

fn sguid(m: &Model, s: SurfaceId) -> sk_model::Guid {
    m.attr().surface(s).unwrap().guid
}

fn werk(name: &str) -> Option<Pattern> {
    sk_model::proctex::factory(name)
}

const VB: &str = "Verblender (Vormauerziegel)";
const PU: &str = "Putz";

/// Altdatei: Werksmuster nach Namen, danach unter der alten Guid.
fn pruefe_altdatei(text: &str, wer: &str) {
    assert!(!text.contains("[pattern]"), "{wer}: Altdatei ohne Muster");
    let l = lesen(text);
    let (vb, pu) = (flaeche(&l.model, VB), flaeche(&l.model, PU));
    let (gv, gp) = (sguid(&l.model, vb), sguid(&l.model, pu));
    assert!(werk(VB).is_some() && werk(PU).is_some());
    assert_eq!(muster(&l.model, vb), werk(VB), "{wer}: Verblender");
    assert_eq!(muster(&l.model, pu), werk(PU), "{wer}: Putz");
    assert!(
        text.contains(&format!("guid={gv}")),
        "{wer}: Guid aus der Datei behalten"
    );
    let neu = sk_model::szo::write(&l.model);
    for g in [gv, gp] {
        assert!(
            neu.lines().any(|z| z.starts_with("[pattern]")
                && z.contains(&format!("surface={g} gen="))
                && !z.contains("gen=none")),
            "{wer}: Musterzeile unter der alten Guid {g}"
        );
    }
    let l2 = lesen(&neu);
    assert_eq!(
        muster(&l2.model, flaeche(&l2.model, VB)),
        werk(VB),
        "{wer}: bleibt"
    );
    assert_eq!(
        muster(&l2.model, flaeche(&l2.model, PU)),
        werk(PU),
        "{wer}: bleibt"
    );
    assert_eq!(sk_model::szo::write(&l2.model), neu, "{wer}: Rundlauf");
}

/// A295 (BIM-Befund cb8eda4): p5.szo und p6.szo (vor 6a gespeichert) laden mit
/// Werksmustern, schreibt sie beim Speichern unter der alten Guid und
/// behält sie danach. Dasselbe für eine selbst gebaute Altdatei mit
/// fremden Guids. Gegenprobe: Steht in der Datei auch nur ein
/// `[pattern]`-Satz, gilt die Namenszuordnung nicht.
#[test]
fn a295_altdatei_behaelt_werksmuster() {
    pruefe_altdatei(P5, "p5.szo");
    pruefe_altdatei(P6, "p6.szo");

    // Selbst gebaut: Werks-Oberflächen mit anderen Guids, ohne [pattern]
    let mut s = Scene::with_model(Model::with_seed(295));
    gebaeude(&mut s);
    let m = s.model();
    let (gv, gp) = (sguid(m, flaeche(m, VB)), sguid(m, flaeche(m, PU)));
    let mut gen = sk_model::GuidGen::with_seed(2950);
    let (nv, np) = (gen.next_guid(), gen.next_guid());
    let alt: String = sk_model::szo::write(m)
        .lines()
        .filter(|l| !l.starts_with("[pattern]"))
        .map(|l| format!("{l}\n"))
        .collect::<String>()
        .replace(&gv.to_string(), &nv.to_string())
        .replace(&gp.to_string(), &np.to_string());
    pruefe_altdatei(&alt, "selbst gebaut");

    // Gegenprobe: ein [pattern]-Satz für Porenbeton, Werks-Oberflächen mit
    // fremder Guid bekommen kein Muster
    let g = {
        let l = lesen(&alt);
        sguid(&l.model, flaeche(&l.model, "Porenbeton"))
    };
    let mit = format!("{alt}[pattern] surface={g} gen=plaster grain=3 spread=4 seed=5\n");
    let l = lesen(&mit);
    assert_eq!(
        muster(&l.model, flaeche(&l.model, VB)),
        None,
        "Regel 60: Guid zählt"
    );
    assert_eq!(muster(&l.model, flaeche(&l.model, PU)), None);
    assert!(matches!(
        muster(&l.model, flaeche(&l.model, "Porenbeton")),
        Some(Pattern::Plaster { grain, .. }) if grain == 3.0
    ));
}
