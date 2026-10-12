//! Abgleich der gespeicherten Räume mit den erkannten Flächen (Grundriss G3,
//! schnittstelle-raum.md §4).
//!
//! Ankerregel (gilt für jeden, der das Modell liest, ohne Vorgeschichte):
//! Jede Fläche hat höchstens einen geltenden Raum unter den Räumen, deren
//! Anker in ihr liegt: zuerst einen, dessen Anker auf seiner Stempellage in
//! dieser Fläche steht (dorthin rückt ihn jeder Abgleich), sonst den
//! ältesten. Die übrigen ruhen mit ihren Daten und leben wieder auf, wenn
//! ihre Fläche zurückkommt.
//!
//! Beim Abschluss eines Schritts folgt jeder Raum seiner Fläche (Review #87
//! B4), wenn der Stand davor bekannt ist ([`Stand`]): Eine unveränderte
//! Fläche behält ihren Raum. Sonst behält ein Raum die Fläche, in der sein
//! Anker liegt, wenn sie überwiegend aus ihm hervorgeht (geteilt: der Teil
//! mit dem Stempel), danach gehen die Flächen nach der größten Überdeckung
//! mit den alten (Wand verschoben; vereint: der größere Anteil gilt). Die
//! Anker rücken auf den Stempel in der neuen Fläche, so gibt die Ankerregel
//! danach dasselbe. Jede Fläche ohne Raum bekommt einen neuen („Raum n“).
//! Nur die Hausgruppen, deren Wände der Schritt ändert, werden angesehen.

use crate::speicher::{beruehren, drin, Fl, Kasten, Lage, NachSchluessel, Raster};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, ElementKind, Model, Space, StoreyId};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// So nah (mm) an seiner Stempellage gilt ein Anker als dorthin gerückt.
const GESETZT: f64 = 0.01;

/// Räume des Geschosses, deren Anker `filter` annimmt, ältere zuerst
/// (R-002 vor R-010 vor R-1000).
pub(crate) fn raeume_im(
    m: &Model,
    g: StoreyId,
    filter: impl Fn(Vec3) -> bool,
) -> Vec<(ElementId, &Space)> {
    let mut v: Vec<(&str, ElementId, &Space)> = m
        .elements()
        .iter()
        .filter_map(|(id, e)| match &e.kind {
            ElementKind::Space(s) if e.storey == g && filter(s.anker) => {
                Some((e.number.as_str(), id, &**s))
            }
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
    v.into_iter().map(|(_, id, s)| (id, s)).collect()
}

/// Stempellage des Raums `s` in der Fläche `f`: die verschobene, wenn sie
/// in ihr liegt, sonst ihre Mitte.
pub(crate) fn stempel_in(s: &Space, f: &Fl) -> Vec3 {
    s.stempel.filter(|p| f.enthaelt(*p)).unwrap_or(f.form.mitte)
}

fn gesetzt(s: &Space, f: &Fl) -> bool {
    let p = stempel_in(s, f);
    (s.anker.x - p.x).abs() <= GESETZT && (s.anker.y - p.y).abs() <= GESETZT
}

/// Geltender Raum je Fläche nach der Ankerregel; `raeume` ältere zuerst.
pub(crate) fn zuteilen(fl: &[&Fl], raeume: &[(ElementId, &Space)]) -> Vec<Option<ElementId>> {
    let kaesten: Vec<Kasten> = fl.iter().map(|f| f.kasten).collect();
    let raster = Raster::neu(&kaesten);
    let mut z: Vec<Option<(ElementId, bool)>> = vec![None; fl.len()];
    for &(id, s) in raeume {
        let Some(i) = raster
            .bei(s.anker)
            .iter()
            .map(|&i| i as usize)
            .find(|&i| fl[i].enthaelt(s.anker))
        else {
            continue;
        };
        let g = gesetzt(s, fl[i]);
        match z[i] {
            None => z[i] = Some((id, g)),
            Some((_, false)) if g => z[i] = Some((id, true)),
            _ => {}
        }
    }
    z.into_iter().map(|x| x.map(|(id, _)| id)).collect()
}

/// Geltende Räume eines Geschosses je Hausgruppe, zum Stand vor einem
/// Schritt ([`Lage::stand`], [`abgleichen_lage`]).
#[derive(Clone, Default)]
pub struct Stand {
    gruppen: NachSchluessel<StandGruppe>,
}

/// Flächen einer Hausgruppe mit ihrem geltenden Raum.
type StandGruppe = Vec<(Rc<Fl>, Option<ElementId>)>;

impl Stand {
    pub(crate) fn setzen(&mut self, schluessel: u64, raeume: StandGruppe) {
        self.gruppen.insert(schluessel, raeume);
    }

    /// Zahl der geltenden Räume.
    pub fn raeume(&self) -> usize {
        self.gruppen
            .values()
            .flatten()
            .filter(|r| r.1.is_some())
            .count()
    }
}

/// Räume des Geschosses, die gerade ruhen (keine Fläche oder ein anderer
/// Raum gilt in ihrer Fläche).
pub fn ruhende(m: &Model, geschoss: StoreyId) -> Vec<ElementId> {
    let gelten: HashSet<ElementId> = crate::raeume(m, geschoss)
        .iter()
        .filter_map(|f| f.raum)
        .collect();
    m.spaces(geschoss)
        .into_iter()
        .filter(|id| !gelten.contains(id))
        .collect()
}

/// Ergebnis von [`abgleichen_lage`].
pub struct Abgleich {
    /// Neu angelegte Räume mit ihrer Rohbaufläche (mm²).
    pub neu: Vec<(ElementId, f64)>,
    /// Geltende Räume nach dem Abgleich (Stand vor dem nächsten Schritt).
    pub stand: Stand,
    /// Zahl der Änderungen am Modell (0: nichts geändert).
    pub aenderungen: usize,
}

/// Gleicht das Geschoss der `lage` im offenen Schritt ab (siehe Modulkopf);
/// `alt`: Stand vor dem Schritt, ohne ihn gilt nur die Ankerregel.
pub fn abgleichen_lage(m: &mut Model, lage: &Lage, alt: Option<Stand>) -> Abgleich {
    let g = lage.geschoss;
    let mit_stand = alt.is_some();
    let mut alt = alt.unwrap_or_default();
    let mut out = Abgleich {
        neu: Vec::new(),
        stand: Stand::default(),
        aenderungen: 0,
    };
    // Unveränderte Hausgruppen behalten ihre Räume
    let mut geaendert = Vec::new();
    for (gi, gr) in lage.gruppen.iter().enumerate() {
        match alt.gruppen.remove(&gr.schluessel) {
            Some(sg) => {
                out.stand.gruppen.insert(gr.schluessel, sg);
            }
            None => geaendert.push(gi),
        }
    }
    if geaendert.is_empty() {
        return out;
    }
    // Flächen der geänderten Gruppen, in der Ordnung der Räume
    let mut neu: Vec<(usize, Rc<Fl>)> = geaendert
        .iter()
        .flat_map(|&gi| lage.gruppen[gi].formen.iter().map(move |f| (gi, f.clone())))
        .collect();
    neu.sort_by(|a, b| Fl::ordnung(&a.1, &b.1));
    let fl: Vec<&Fl> = neu.iter().map(|(_, f)| f.as_ref()).collect();
    let kaesten: Vec<Kasten> = geaendert
        .iter()
        .map(|&gi| lage.gruppen[gi].kasten)
        .collect();
    let bereich = Raster::neu(&kaesten);
    let im_bereich = |p: Vec3| {
        bereich
            .bei(p)
            .iter()
            .any(|&i| drin(&kaesten[i as usize], p))
    };
    // Mit dem Stand davor folgen die Räume der weggefallenen Gruppen ihrer
    // Fläche
    let mut paare = Vec::new();
    if mit_stand {
        let mut alte: Vec<(&str, ElementId, &Fl)> = alt
            .gruppen
            .values()
            .flatten()
            .filter_map(|(f, r)| {
                let id = (*r)?;
                let e = m.element(id)?;
                (e.storey == g && m.space(id).is_some()).then_some((e.number.as_str(), id, &**f))
            })
            .collect();
        alte.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.0.cmp(b.0)));
        let alte: Vec<(ElementId, &Fl)> = alte.into_iter().map(|(_, id, f)| (id, f)).collect();
        paare = nach_ueberdeckung(m, &alte, &fl);
        for &(id, i) in &paare {
            out.aenderungen += anker_auf_stempel(m, id, fl[i]);
        }
    }
    let mut z = zuteilen(&fl, &raeume_im(m, g, im_bereich));
    // Ein ruhender, älterer Raum, dessen verschobener Stempel in einer
    // zugeordneten Fläche liegt, gibt die Stempellage ab und ruht weiter
    let mut nochmal = false;
    for &(id, i) in &paare {
        if let Some(y) = z[i].filter(|y| *y != id) {
            if m.space(y).is_some_and(|s| s.stempel.is_some()) {
                m.set_space(y, |s| s.stempel = None);
                out.aenderungen += 1;
                nochmal = true;
            }
        }
    }
    if nochmal {
        z = zuteilen(&fl, &raeume_im(m, g, im_bereich));
    }
    // Anker auf den Stempel; jede Fläche ohne Raum bekommt einen
    let mut namen = None;
    for (i, f) in fl.iter().enumerate() {
        match z[i] {
            Some(id) => out.aenderungen += anker_auf_stempel(m, id, f),
            None => {
                let name = namen
                    .get_or_insert_with(|| m.freie_raumnamen())
                    .next()
                    .unwrap_or_default();
                let id = m.add_space(g, Space::neu(f.form.mitte, name));
                z[i] = Some(id);
                out.neu.push((id, f.form.flaeche));
                out.aenderungen += 1;
            }
        }
    }
    let mut je: HashMap<usize, StandGruppe> = HashMap::new();
    for ((gi, f), r) in neu.iter().zip(&z) {
        je.entry(*gi).or_default().push((f.clone(), *r));
    }
    for gi in geaendert {
        let raeume = je.remove(&gi).unwrap_or_default();
        out.stand.setzen(lage.gruppen[gi].schluessel, raeume);
    }
    out
}

/// Rückt den Anker des Raums auf seine Stempellage in `f`; 1, wenn er sich
/// bewegt hat.
fn anker_auf_stempel(m: &mut Model, id: ElementId, f: &Fl) -> usize {
    let Some(s) = m.space(id) else {
        return 0;
    };
    let ziel = stempel_in(s, f);
    if s.anker == ziel {
        return 0;
    }
    m.set_space(id, |s| s.anker = ziel);
    1
}

/// Paare (Raum, Nummer der neuen Fläche) nach dem Stand davor; `alte`:
/// geltende Räume der weggefallenen Gruppen mit ihrer alten Fläche, ältere
/// zuerst (siehe Modulkopf).
fn nach_ueberdeckung(m: &Model, alte: &[(ElementId, &Fl)], neu: &[&Fl]) -> Vec<(ElementId, usize)> {
    let mut frei_a = vec![true; alte.len()];
    let mut frei_n = vec![true; neu.len()];
    let mut paare = Vec::new();
    // Unveränderte Fläche
    let mut gleich: NachSchluessel<Vec<usize>> = NachSchluessel::default();
    for (i, f) in neu.iter().enumerate() {
        gleich.entry(f.kennung).or_default().push(i);
    }
    for (a, &(id, f)) in alte.iter().enumerate() {
        let i = gleich
            .get(&f.kennung)
            .and_then(|v| v.iter().copied().find(|&i| frei_n[i]));
        if let Some(i) = i {
            frei_a[a] = false;
            frei_n[i] = false;
            paare.push((id, i));
        }
    }
    // Anteil jeder neuen Fläche, der in einer alten liegt (Stichproben)
    let mut anteil: Vec<(usize, usize, f64)> = Vec::new();
    for (i, f) in neu.iter().enumerate() {
        if !frei_n[i] {
            continue;
        }
        let kand: Vec<usize> = (0..alte.len())
            .filter(|&a| frei_a[a] && beruehren(&alte[a].1.kasten, &f.kasten))
            .collect();
        if kand.is_empty() {
            continue;
        }
        let p = proben(f);
        for a in kand {
            let n = p.iter().filter(|q| alte[a].1.enthaelt(**q)).count();
            if n > 0 {
                anteil.push((a, i, n as f64 / p.len() as f64));
            }
        }
    }
    // Der Teil mit dem Anker, wenn er überwiegend aus dem Raum hervorgeht
    for (a, &(id, _)) in alte.iter().enumerate() {
        let Some(anker) = m.space(id).map(|s| s.anker) else {
            continue;
        };
        let teil = anteil.iter().find(|&&(b, i, q)| {
            b == a && q > 0.5 && frei_a[a] && frei_n[i] && neu[i].enthaelt(anker)
        });
        if let Some(&(_, i, _)) = teil {
            frei_a[a] = false;
            frei_n[i] = false;
            paare.push((id, i));
        }
    }
    // Größte Überdeckung zuerst, bei Gleichstand der ältere Raum
    let mut nach: Vec<(f64, usize, usize)> = anteil
        .iter()
        .map(|&(a, i, q)| (q * neu[i].form.flaeche, a, i))
        .filter(|&(f, a, i)| f > 0.01 * alte[a].1.form.flaeche.min(neu[i].form.flaeche))
        .collect();
    nach.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
    for (_, a, i) in nach {
        if frei_a[a] && frei_n[i] {
            frei_a[a] = false;
            frei_n[i] = false;
            paare.push((alte[a].0, i));
        }
    }
    paare
}

/// Stichproben in der Fläche: Mitten eines Gitters von etwa 1000 Zellen
/// über ihrem Kasten, die in ihr liegen; mindestens ihre Mitte.
fn proben(f: &Fl) -> Vec<Vec3> {
    let k = f.kasten;
    let (w, h) = (k[2] - k[0], k[3] - k[1]);
    let s = (w * h / 1024.0).sqrt().max(1.0);
    let nx = (w / s).ceil().max(1.0) as usize;
    let ny = (h / s).ceil().max(1.0) as usize;
    let mut v = Vec::new();
    for y in 0..ny {
        for x in 0..nx {
            let p = vec3(
                k[0] + (x as f64 + 0.5) * w / nx as f64,
                k[1] + (y as f64 + 0.5) * h / ny as f64,
                0.0,
            );
            if f.enthaelt(p) {
                v.push(p);
            }
        }
    }
    if v.is_empty() {
        v.push(f.form.mitte);
    }
    v
}

/// Gleicht die Geschosse `geschosse` im offenen Schritt nach der
/// Ankerregel ab (ohne Stand davor, alles neu gerechnet): legt für jede
/// Fläche ohne Raum einen an und rückt die Anker der geltenden Räume auf
/// ihren Stempel. Gibt die Zahl der Änderungen zurück (0: nichts geändert).
pub fn abgleichen(m: &mut Model, geschosse: &[StoreyId]) -> usize {
    let mut n = 0;
    for &g in geschosse {
        let lage = crate::Erkennung::default().lage(m, m, g);
        n += abgleichen_lage(m, &lage, None).aenderungen;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erkennung::{raeume, RaumForm};
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

    /// Name des Raums, der die Fläche um `p` trägt.
    fn name_bei(m: &Model, st: StoreyId, p: Vec3) -> String {
        let r = raeume(m, st);
        let f = r.iter().find(|f| f.enthaelt(p)).expect("Fläche");
        name(m, f)
    }

    /// Schritt mit Abgleich gegen den Stand davor (wie die Szene).
    fn schritt(m: &mut Model, st: StoreyId, f: impl FnOnce(&mut Model)) -> Abgleich {
        let vorher = crate::Erkennung::default().lage(m, m, st).stand(m);
        m.begin("Schritt");
        f(m);
        let lage = crate::Erkennung::default().lage(m, m, st);
        let a = abgleichen_lage(m, &lage, Some(vorher));
        m.commit();
        a
    }

    fn wand_nach(m: &mut Model, w: RunId, x: f64) {
        let pts: Vec<Vec3> = m
            .run(w)
            .unwrap()
            .points
            .iter()
            .map(|p| vec3(x, p.y, 0.0))
            .collect();
        assert!(m.set_run_points(w, &pts).is_some());
    }

    /// Haus mit Innenwand bei x = 3 m: links „Bad“, rechts „Wohnen“; das
    /// Bad ist der ältere Raum oder der jüngere.
    fn bad_und_wohnen(bad_aelter: bool) -> (Model, StoreyId, RunId) {
        let (mut m, st, t) = haus();
        m.begin("Raum");
        if !bad_aelter {
            ab(&mut m);
        }
        let w = innenwand(&mut m, st, t, 3000.0);
        ab(&mut m);
        m.commit().unwrap();
        let r = raeume(&m, st);
        m.begin("Namen");
        for (f, n) in r.iter().filter(|f| f.geschoss == st).zip(["Bad", "Wohnen"]) {
            m.set_space(f.raum.unwrap(), |s| s.name = n.into());
        }
        m.commit().unwrap();
        let bad = r[0].raum.unwrap();
        let wohnen = r[1].raum.unwrap();
        let nummer = |id| m.element(id).unwrap().number.clone();
        assert_eq!(nummer(bad) < nummer(wohnen), bad_aelter);
        (m, st, w)
    }

    /// Review #87 B4: Die Innenwand springt in einem Schritt von 3 m auf
    /// 1,4, 2,5 oder 6 m; links bleibt das Bad, rechts das Wohnen, kein
    /// neuer Raum, keiner ruht, und die Ankerregel sagt danach dasselbe.
    #[test]
    fn wand_verschoben_namen_bleiben() {
        for bad_aelter in [true, false] {
            for x in [1400.0, 2500.0, 6000.0] {
                let (mut m, st, w) = bad_und_wohnen(bad_aelter);
                let n = m.spaces(st).len();
                let a = schritt(&mut m, st, |m| wand_nach(m, w, x));
                let fall = format!("x = {x}, Bad älter: {bad_aelter}");
                assert!(a.neu.is_empty(), "{fall}");
                assert_eq!(m.spaces(st).len(), n, "{fall}");
                assert!(ruhende(&m, st).is_empty(), "{fall}");
                assert_eq!(
                    name_bei(&m, st, vec3(x - 500.0, 4000.0, 0.0)),
                    "Bad",
                    "{fall}"
                );
                assert_eq!(
                    name_bei(&m, st, vec3(x + 500.0, 4000.0, 0.0)),
                    "Wohnen",
                    "{fall}"
                );
                assert_eq!(a.stand.raeume(), 2, "{fall}");
            }
        }
    }

    /// Vereint gilt der größere Anteil, auch wenn er der jüngere ist; kommt
    /// die Wand zurück, lebt der kleinere wieder auf, auch mit einem
    /// verschobenen Stempel.
    #[test]
    fn vereint_gilt_der_groessere() {
        for stempel in [false, true] {
            let (mut m, st, w) = bad_und_wohnen(true);
            let bad = raeume(&m, st)[0].raum.unwrap();
            if stempel {
                m.begin("Stempel");
                m.set_space(bad, |s| {
                    s.stempel = Some(vec3(1000.0, 2000.0, 0.0));
                    s.anker = vec3(1000.0, 2000.0, 0.0);
                });
                m.commit().unwrap();
            }
            let a = schritt(&mut m, st, |m| assert!(m.remove_run(w)));
            assert!(a.neu.is_empty());
            assert_eq!(name_bei(&m, st, vec3(5000.0, 4000.0, 0.0)), "Wohnen");
            assert_eq!(ruhende(&m, st), vec![bad], "Stempel {stempel}");
            let t = m
                .wall_layers(m.defaults().exterior_wall)
                .iter()
                .map(|l| l.thickness)
                .sum();
            let a = schritt(&mut m, st, |m| {
                innenwand(m, st, t, 3000.0);
            });
            assert!(a.neu.is_empty());
            assert_eq!(name_bei(&m, st, vec3(1000.0, 4000.0, 0.0)), "Bad");
            assert_eq!(name_bei(&m, st, vec3(5000.0, 4000.0, 0.0)), "Wohnen");
            assert!(ruhende(&m, st).is_empty());
        }
    }

    /// Ohne geänderte Wände bleibt alles, wie es ist: kein Abgleich in
    /// unveränderten Hausgruppen.
    #[test]
    fn unveraenderte_gruppe_bleibt() {
        let (mut m, st, _) = bad_und_wohnen(true);
        let vorher = crate::Erkennung::default().lage(&m, &m, st).stand(&m);
        m.begin("Nichts");
        let lage = crate::Erkennung::default().lage(&m, &m, st);
        let a = abgleichen_lage(&mut m, &lage, Some(vorher));
        assert_eq!((a.aenderungen, a.stand.raeume()), (0, 2));
        assert!(m.commit().is_none());
    }
}
