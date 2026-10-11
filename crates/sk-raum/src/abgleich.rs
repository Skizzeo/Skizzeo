//! Abgleich der gespeicherten Räume mit den erkannten Flächen (Grundriss G3,
//! schnittstelle-raum.md §4). Jede Fläche hat höchstens einen geltenden Raum:
//! den ältesten, dessen Anker in ihr liegt; die übrigen ruhen mit ihren
//! Daten und leben wieder auf, wenn ihre Fläche zurückkommt. Beim Abschluss
//! eines Schritts bekommt jede Fläche ohne Raum einen neuen („Raum n“), und
//! der Anker jedes geltenden Raums rückt auf seinen Stempel: Wird ein Raum
//! geteilt, behält der Teil mit dem Stempel Name und Nutzung.

use crate::erkennung::{raeume, RaumForm};
use sk_model::{ElementId, Model, Space, StoreyId};

/// Räume des Geschosses, ältere zuerst (R-002 vor R-010 vor R-1000).
fn nach_alter(m: &Model, geschoss: StoreyId) -> Vec<(ElementId, &Space)> {
    let mut v: Vec<(&str, ElementId, &Space)> = m
        .spaces(geschoss)
        .into_iter()
        .filter_map(|id| Some((m.element(id)?.number.as_str(), id, m.space(id)?)))
        .collect();
    v.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
    v.into_iter().map(|(_, id, s)| (id, s)).collect()
}

/// Trägt in `formen` den geltenden Raum und die Stempellage ein (rein).
pub(crate) fn zuordnen(m: &Model, geschoss: StoreyId, formen: &mut [RaumForm]) {
    for (id, s) in nach_alter(m, geschoss) {
        let Some(f) = formen.iter_mut().find(|f| f.enthaelt(s.anker)) else {
            continue;
        };
        if f.raum.is_none() {
            f.raum = Some(id);
            f.stempel = s.stempel.filter(|p| f.enthaelt(*p)).unwrap_or(f.mitte);
        }
    }
}

/// Räume des Geschosses, die gerade ruhen (keine Fläche oder ein älterer
/// Raum in derselben Fläche).
pub fn ruhende(m: &Model, geschoss: StoreyId) -> Vec<ElementId> {
    let formen = raeume(m, geschoss);
    m.spaces(geschoss)
        .into_iter()
        .filter(|id| !formen.iter().any(|f| f.raum == Some(*id)))
        .collect()
}

/// Gleicht die Geschosse `geschosse` ab, im offenen Schritt: legt für jede
/// Fläche ohne Raum einen an und rückt die Anker der geltenden Räume auf
/// ihren Stempel. Gibt die Zahl der Änderungen zurück (0: nichts geändert).
pub fn abgleichen(m: &mut Model, geschosse: &[StoreyId]) -> usize {
    let mut n = 0;
    for &g in geschosse {
        for f in raeume(m, g) {
            match f.raum {
                Some(id) if m.space(id).is_some_and(|s| s.anker != f.stempel) => {
                    m.set_space(id, |s| s.anker = f.stempel);
                    n += 1;
                }
                Some(_) => {}
                None => {
                    let name = m.naechster_raumname();
                    m.add_space(g, Space::neu(f.stempel, name));
                    n += 1;
                }
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, Direction, RefSide, RunId};

    /// Haus 10 × 8 m, Außenwand Bezug außen.
    fn haus() -> (Model, StoreyId, f64) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        (m, st, t)
    }

    fn innenwand(m: &mut Model, st: StoreyId, t: f64, x: f64) -> RunId {
        let iw = m.defaults().interior_wall;
        let pts = [vec3(x, t, 0.0), vec3(x, 8000.0 - t, 0.0)];
        m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap()
    }

    /// Gleicht alle Geschosse ab.
    fn ab(m: &mut Model) -> usize {
        let g: Vec<StoreyId> = m.storeys().iter().map(|(id, _)| id).collect();
        abgleichen(m, &g)
    }

    fn name(m: &Model, f: &RaumForm) -> String {
        f.raum.and_then(|id| m.space(id)).unwrap().name.clone()
    }

    #[test]
    fn neuer_raum_teilen_vereinen_rueckgaengig() {
        let (mut m, st, t) = haus();
        assert_eq!(raeume(&m, st)[0].raum, None, "noch nicht angelegt");
        m.begin("Raum");
        assert_eq!(ab(&mut m), 2, "EG und OG (der Ring steht in beiden)");
        assert_eq!(ab(&mut m), 0, "zweimal ändert nichts");
        m.commit().unwrap();
        let r = raeume(&m, st);
        let r1 = r[0].raum.unwrap();
        assert_eq!(name(&m, &r[0]), "Raum 1", "EG zuerst");
        assert_eq!(m.space(r1).unwrap().anker, r[0].mitte, "Anker am Stempel");

        // Teilen: der Teil mit dem Stempel (rechts) behält den Raum
        m.begin("Wand");
        let w = innenwand(&mut m, st, t, 3000.0);
        assert_eq!(ab(&mut m), 2, "neuer Raum, der Anker des alten rückt");
        let teilen = m.commit().unwrap();
        let r = raeume(&m, st);
        let rechts = r
            .iter()
            .find(|f| f.enthaelt(vec3(8000.0, 4000.0, 0.0)))
            .unwrap();
        let links = r
            .iter()
            .find(|f| f.enthaelt(vec3(1000.0, 4000.0, 0.0)))
            .unwrap();
        assert_eq!(rechts.raum, Some(r1));
        assert_eq!(name(&m, links), "Raum 3", "Raum 2 ist das OG");
        let r2 = links.raum.unwrap();

        // Vereinen: der ältere gilt, der andere ruht mit seinen Daten
        m.begin("Name");
        m.set_space(r2, |s| s.name = "Küche".into());
        m.commit().unwrap();
        m.begin("Löschen");
        assert!(m.remove_run(w));
        ab(&mut m);
        m.commit().unwrap();
        let r = raeume(&m, st);
        assert_eq!((r.len(), r[0].raum), (1, Some(r1)));
        assert_eq!(ruhende(&m, st), vec![r2]);

        // Kommt die Wand zurück, lebt die Küche wieder auf
        m.begin("Wand");
        innenwand(&mut m, st, t, 3000.0);
        ab(&mut m);
        m.commit().unwrap();
        assert!(ruhende(&m, st).is_empty());
        let r = raeume(&m, st);
        let links = r
            .iter()
            .find(|f| f.enthaelt(vec3(1000.0, 4000.0, 0.0)))
            .unwrap();
        assert_eq!(name(&m, links), "Küche");
        assert_eq!(m.spaces(st).len(), 2, "kein dritter Raum");

        // Rückgängig nimmt den beim Teilen angelegten Raum mit
        let mut m2 = haus().0;
        m2.begin("Raum");
        ab(&mut m2);
        m2.commit().unwrap();
        m2.begin("Wand");
        innenwand(&mut m2, st, t, 3000.0);
        ab(&mut m2);
        let t2 = m2.commit().unwrap();
        assert_eq!(m2.spaces(st).len(), 2);
        m2.apply(&t2, Direction::Undo);
        assert_eq!(m2.spaces(st).len(), 1);
        assert_eq!(teilen.label, t2.label);
    }

    #[test]
    fn nur_geschosse_mit_geaenderten_waenden() {
        let (mut m, st, t) = haus();
        assert!(m.wand_geschosse_im_schritt().is_empty(), "ohne Schritt");
        m.begin("Wand");
        assert!(m.wand_geschosse_im_schritt().is_empty());
        let w = innenwand(&mut m, st, t, 3000.0);
        assert_eq!(m.wand_geschosse_im_schritt(), vec![st]);
        let g = m.wand_geschosse_im_schritt();
        assert_eq!(abgleichen(&mut m, &g), 2, "nur das EG, zwei Räume");
        m.commit().unwrap();
        m.begin("Löschen");
        m.remove_run(w);
        assert_eq!(m.wand_geschosse_im_schritt(), vec![st], "auch gelöschte");
        m.rollback();
    }

    #[test]
    fn verschobener_stempel_entscheidet() {
        let (mut m, st, t) = haus();
        m.begin("Raum");
        ab(&mut m);
        m.commit().unwrap();
        let r1 = raeume(&m, st)[0].raum.unwrap();
        // Stempel nach links geschoben: beim Teilen behält der linke Teil
        m.begin("Stempel");
        m.set_space(r1, |s| s.stempel = Some(vec3(1500.0, 2000.0, 0.0)));
        ab(&mut m);
        m.commit().unwrap();
        assert_eq!(m.space(r1).unwrap().anker, vec3(1500.0, 2000.0, 0.0));
        m.begin("Wand");
        innenwand(&mut m, st, t, 3000.0);
        ab(&mut m);
        m.commit().unwrap();
        let r = raeume(&m, st);
        let links = r.iter().find(|f| f.raum == Some(r1)).unwrap();
        assert!(links.enthaelt(vec3(1000.0, 4000.0, 0.0)));
        assert_eq!(links.stempel, vec3(1500.0, 2000.0, 0.0));
    }
}
