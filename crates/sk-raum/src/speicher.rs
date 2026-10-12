//! Zwischenspeicher der Raumerkennung (Review #87 B1 bis B3): der Rohbau
//! je Wandzug, solange sich seine Kette nicht ändert, und die Flächen je
//! Hausgruppe, solange sich ihre Umrisse nicht ändern. Eine Hausgruppe
//! sind die Wandzüge eines Geschosses, deren Umrisse (um die Fangweite
//! erweitert) einander berühren; nur innerhalb einer Gruppe schließen
//! Wände gemeinsam Räume. Ein Wandschritt rechnet so nur die Gruppe neu,
//! an der er zieht, und der Abgleich sieht nur ihre Räume an.

use crate::abgleich::{zuteilen, Stand};
use crate::erkennung::{formen_aus, rohbau, Kante, RaumForm, Teil, TOL};
use sk_math::Vec3;
use sk_model::{Model, RunId, StoreyId, WallChain};
use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Achsparalleler Kasten: x und y von–bis (mm).
pub(crate) type Kasten = [f64; 4];

/// So weit (mm) wird der Kasten eines Zugs je Seite erweitert: Umrisse, die
/// sich bis auf die Fangweite nähern, kommen in dieselbe Gruppe.
const RAND: f64 = 2.0 * TOL;

/// Woher die Erkennung die Ketten der Wandzüge nimmt: aus dem Modell (alles
/// neu) oder aus den berechneten Zügen der Szene.
pub trait Ketten {
    /// Fassung der Kette des Zugs: gleich, solange sie sich nicht ändert;
    /// `None`, wenn unbekannt (dann rechnet die Erkennung den Rohbau neu).
    fn fassung(&self, run: RunId) -> Option<u64>;
    /// Die Kette des Zugs ([`Model::chain`]).
    fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>>;
}

impl Ketten for Model {
    fn fassung(&self, _: RunId) -> Option<u64> {
        None
    }

    fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
        self.chain(run).map(Cow::Owned)
    }
}

/// Rohbauumrisse eines Zugs, ein Teil je Umriss.
pub(crate) struct Roh {
    umrisse: Vec<Vec<Vec3>>,
    teile: Vec<Teil>,
    kasten: Option<Kasten>,
    /// Fingerabdruck von Umrissen und Teilen.
    kennung: u64,
}

impl Roh {
    fn neu(m: &Model, run: RunId, c: &WallChain) -> Roh {
        let (mut umrisse, mut teile) = (Vec::new(), Vec::new());
        rohbau(m, run, c, &mut umrisse, &mut teile);
        let kasten = kasten_um(umrisse.iter().flatten());
        let mut h = DefaultHasher::new();
        for u in &umrisse {
            u.len().hash(&mut h);
            for p in u {
                [p.x, p.y].map(f64::to_bits).hash(&mut h);
            }
        }
        for t in &teile {
            t.hash_in(&mut h);
        }
        Roh {
            umrisse,
            teile,
            kasten,
            kennung: h.finish(),
        }
    }
}

/// Eine erkannte Fläche (noch ohne Raum) mit allem, was Zuordnung und
/// Stempel oft brauchen.
pub(crate) struct Fl {
    /// Raum `None`, Stempel auf der Mitte.
    pub form: RaumForm,
    pub kasten: Kasten,
    rand: Vec<Vec3>,
    loecher: Vec<Vec<Vec3>>,
    /// Fingerabdruck der Geometrie: dieselbe Fläche hat denselben Wert.
    pub kennung: u64,
    /// Tiefster, dann linkester Punkt des Umrisses (Ordnung der Räume).
    rang: Vec3,
    grund: OnceCell<f64>,
}

impl Fl {
    fn neu(form: RaumForm) -> Fl {
        let ring = |k: &[Kante]| -> Vec<Vec3> { k.iter().map(|k| k.a).collect() };
        let rand = ring(&form.umriss);
        let loecher: Vec<Vec<Vec3>> = form.loecher.iter().map(|l| ring(l)).collect();
        let mut h = DefaultHasher::new();
        for r in std::iter::once(&rand).chain(&loecher) {
            r.len().hash(&mut h);
            for p in r {
                [p.x, p.y].map(f64::to_bits).hash(&mut h);
            }
        }
        let rang = rand
            .iter()
            .copied()
            .min_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)))
            .unwrap_or_default();
        Fl {
            kasten: kasten_um(rand.iter()).unwrap_or_default(),
            rand,
            loecher,
            kennung: h.finish(),
            rang,
            grund: OnceCell::new(),
            form,
        }
    }

    /// Liegt `p` in der Fläche (nicht in einem Loch)?
    pub fn enthaelt(&self, p: Vec3) -> bool {
        use sk_math::flaeche::enthaelt;
        drin(&self.kasten, p)
            && enthaelt(&self.rand, p)
            && !self.loecher.iter().any(|l| enthaelt(l, p))
    }

    /// Grundfläche (mm², [`crate::flaechen::grundflaeche`]), einmal gerechnet.
    pub fn grundflaeche(&self) -> f64 {
        *self
            .grund
            .get_or_init(|| crate::flaechen::grundflaeche(&self.form))
    }

    /// Ordnung der Räume: nach dem tiefsten, dann linkesten Punkt.
    pub fn ordnung(a: &Fl, b: &Fl) -> std::cmp::Ordering {
        (a.rang.y.total_cmp(&b.rang.y)).then(a.rang.x.total_cmp(&b.rang.x))
    }
}

/// Wandzüge einer Hausgruppe und ihre Flächen.
pub(crate) struct Gruppe {
    /// Fingerabdruck von Geschoss und Rohbau der Züge.
    pub schluessel: u64,
    /// Um alle Züge, je Seite um [`RAND`] erweitert; enthält jede Fläche.
    pub kasten: Kasten,
    pub formen: Rc<[Rc<Fl>]>,
}

/// Erkannte Flächen eines Geschosses, je Hausgruppe.
pub struct Lage {
    pub geschoss: StoreyId,
    pub(crate) gruppen: Vec<Gruppe>,
}

impl Lage {
    /// Alle Flächen, nach ihrem tiefsten, dann linkesten Punkt.
    pub(crate) fn flaechen(&self) -> Vec<&Fl> {
        let mut v: Vec<&Fl> = self
            .gruppen
            .iter()
            .flat_map(|g| g.formen.iter().map(|f| f.as_ref()))
            .collect();
        v.sort_by(|a, b| Fl::ordnung(a, b));
        v
    }

    /// Die Räume des Geschosses wie [`crate::raeume`]: Formen mit dem
    /// geltenden Raum und der Stempellage.
    pub fn raeume(&self, m: &Model) -> Vec<RaumForm> {
        self.mit_grundflaeche_wenn(m, false)
            .into_iter()
            .map(|(f, _)| f)
            .collect()
    }

    /// Wie [`Lage::raeume`], dazu je Raum die Grundfläche (mm²).
    pub fn mit_grundflaeche(&self, m: &Model) -> Vec<(RaumForm, f64)> {
        self.mit_grundflaeche_wenn(m, true)
    }

    fn mit_grundflaeche_wenn(&self, m: &Model, grund: bool) -> Vec<(RaumForm, f64)> {
        let fl = self.flaechen();
        let raeume = crate::abgleich::raeume_im(m, self.geschoss, |_| true);
        let z = zuteilen(&fl, &raeume);
        fl.iter()
            .zip(z)
            .map(|(f, r)| {
                let mut form = f.form.clone();
                if let Some(id) = r {
                    form.raum = Some(id);
                    if let Some(s) = m.space(id) {
                        form.stempel = crate::abgleich::stempel_in(s, f);
                    }
                }
                (form, if grund { f.grundflaeche() } else { 0.0 })
            })
            .collect()
    }

    /// Geltende Räume je Fläche, als Stand vor dem nächsten Schritt
    /// ([`crate::abgleichen_lage`]).
    pub fn stand(&self, m: &Model) -> Stand {
        let fl = self.flaechen();
        let raeume = crate::abgleich::raeume_im(m, self.geschoss, |_| true);
        let z: HashMap<*const Fl, _> = fl
            .iter()
            .map(|f| *f as *const Fl)
            .zip(zuteilen(&fl, &raeume))
            .collect();
        let mut st = Stand::default();
        for g in &self.gruppen {
            let raeume = g
                .formen
                .iter()
                .map(|f| (f.clone(), z.get(&Rc::as_ptr(f)).copied().flatten()))
                .collect();
            st.setzen(g.schluessel, raeume);
        }
        st
    }
}

struct RohEintrag {
    geschoss: StoreyId,
    fassung: u64,
    roh: Rc<Roh>,
    lauf: u64,
}

struct GruppenEintrag {
    geschoss: StoreyId,
    formen: Rc<[Rc<Fl>]>,
    lauf: u64,
}

/// Raumerkennung mit Zwischenspeicher (siehe Modulkopf). Ergebnis wie
/// [`crate::raeume`]; was in einem Aufruf je Geschoss nicht mehr vorkommt,
/// fällt aus dem Speicher.
#[derive(Default)]
pub struct Erkennung {
    roh: HashMap<RunId, RohEintrag>,
    gruppen: HashMap<u64, GruppenEintrag>,
    lauf: u64,
    gerechnet: u64,
}

impl Erkennung {
    /// Wie oft eine Hausgruppe neu gerechnet wurde (für Tests und Messung).
    pub fn gerechnet(&self) -> u64 {
        self.gerechnet
    }

    /// Flächen des Geschosses `g`, je Hausgruppe.
    pub fn lage(&mut self, m: &Model, k: &(impl Ketten + ?Sized), g: StoreyId) -> Lage {
        self.lauf += 1;
        let lauf = self.lauf;
        let mut zuege: Vec<Rc<Roh>> = Vec::new();
        for (id, r) in m.runs().iter() {
            if r.storey != g {
                continue;
            }
            let fassung = k.fassung(id);
            let roh = match (fassung, self.roh.get_mut(&id)) {
                (Some(f), Some(e)) if e.fassung == f && e.geschoss == g => {
                    e.lauf = lauf;
                    e.roh.clone()
                }
                _ => {
                    let Some(c) = k.kette(id) else {
                        continue;
                    };
                    let roh = Rc::new(Roh::neu(m, id, &c));
                    match fassung {
                        Some(fassung) => {
                            let e = RohEintrag {
                                geschoss: g,
                                fassung,
                                roh: roh.clone(),
                                lauf,
                            };
                            self.roh.insert(id, e);
                        }
                        None => {
                            self.roh.remove(&id);
                        }
                    }
                    roh
                }
            };
            if roh.kasten.is_some() {
                zuege.push(roh);
            }
        }
        self.roh.retain(|_, e| e.geschoss != g || e.lauf == lauf);
        let mut gruppen = Vec::new();
        for zg in gruppieren(&zuege) {
            let mut h = DefaultHasher::new();
            g.hash(&mut h);
            for &i in &zg {
                zuege[i].kennung.hash(&mut h);
            }
            let schluessel = h.finish();
            let kasten = zg
                .iter()
                .filter_map(|&i| zuege[i].kasten)
                .map(|k| weiter(&k, RAND))
                .reduce(|a, b| vereint(&a, &b))
                .unwrap_or_default();
            let formen = match self.gruppen.get_mut(&schluessel) {
                Some(e) if e.geschoss == g => {
                    e.lauf = lauf;
                    e.formen.clone()
                }
                _ => {
                    self.gerechnet += 1;
                    let mut umrisse = Vec::new();
                    let mut teile = Vec::new();
                    for &i in &zg {
                        umrisse.extend(zuege[i].umrisse.iter().cloned());
                        teile.extend(zuege[i].teile.iter().cloned());
                    }
                    let formen: Rc<[Rc<Fl>]> = formen_aus(&umrisse, &teile, g)
                        .into_iter()
                        .map(|f| Rc::new(Fl::neu(f)))
                        .collect();
                    let e = GruppenEintrag {
                        geschoss: g,
                        formen: formen.clone(),
                        lauf,
                    };
                    self.gruppen.insert(schluessel, e);
                    formen
                }
            };
            gruppen.push(Gruppe {
                schluessel,
                kasten,
                formen,
            });
        }
        self.gruppen
            .retain(|_, e| e.geschoss != g || e.lauf == lauf);
        Lage {
            geschoss: g,
            gruppen,
        }
    }
}

/// Hausgruppen: Züge, deren erweiterte Kästen sich berühren (auch über
/// andere), je Gruppe in der Reihenfolge der Züge. Ein Lauf längs x über
/// die nach links sortierten Kästen.
fn gruppieren(zuege: &[Rc<Roh>]) -> Vec<Vec<usize>> {
    let k: Vec<Kasten> = zuege
        .iter()
        .map(|z| weiter(&z.kasten.unwrap_or_default(), RAND))
        .collect();
    let mut eltern: Vec<usize> = (0..k.len()).collect();
    fn wurzel(e: &mut [usize], mut i: usize) -> usize {
        while e[i] != i {
            e[i] = e[e[i]];
            i = e[i];
        }
        i
    }
    let mut folge: Vec<usize> = (0..k.len()).collect();
    folge.sort_by(|&a, &b| k[a][0].total_cmp(&k[b][0]));
    let mut offen: Vec<usize> = Vec::new();
    for &i in &folge {
        offen.retain(|&j| k[j][2] >= k[i][0]);
        for &j in &offen {
            if k[j][1] <= k[i][3] && k[i][1] <= k[j][3] {
                let (a, b) = (wurzel(&mut eltern, i), wurzel(&mut eltern, j));
                eltern[a.max(b)] = a.min(b);
            }
        }
        offen.push(i);
    }
    let mut nach_wurzel: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for i in 0..k.len() {
        let w = wurzel(&mut eltern, i);
        let n = *nach_wurzel.entry(w).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[n].push(i);
    }
    out
}

pub(crate) fn kasten_um<'a>(pts: impl IntoIterator<Item = &'a Vec3>) -> Option<Kasten> {
    pts.into_iter().fold(None, |k, p| {
        Some(match k {
            None => [p.x, p.y, p.x, p.y],
            Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        })
    })
}

pub(crate) fn drin(k: &Kasten, p: Vec3) -> bool {
    k[0] <= p.x && p.x <= k[2] && k[1] <= p.y && p.y <= k[3]
}

pub(crate) fn beruehren(a: &Kasten, b: &Kasten) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

fn weiter(k: &Kasten, d: f64) -> Kasten {
    [k[0] - d, k[1] - d, k[2] + d, k[3] + d]
}

fn vereint(a: &Kasten, b: &Kasten) -> Kasten {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Raster über Kästen: zu einem Punkt die Kästen, die ihn enthalten können.
pub(crate) struct Raster {
    x0: f64,
    y0: f64,
    zx: f64,
    zy: f64,
    nx: usize,
    ny: usize,
    zellen: Vec<Vec<u32>>,
}

impl Raster {
    /// Etwa ein Kasten je Zelle, höchstens 256 × 256 Zellen.
    pub fn neu(kaesten: &[Kasten]) -> Raster {
        let Some(g) = kaesten.iter().copied().reduce(|a, b| vereint(&a, &b)) else {
            return Raster {
                x0: 0.0,
                y0: 0.0,
                zx: 1.0,
                zy: 1.0,
                nx: 0,
                ny: 0,
                zellen: Vec::new(),
            };
        };
        let (w, h) = ((g[2] - g[0]).max(1.0), (g[3] - g[1]).max(1.0));
        let z = (w * h / kaesten.len() as f64).sqrt().max(1.0);
        let nx = ((w / z).ceil() as usize).clamp(1, 256);
        let ny = ((h / z).ceil() as usize).clamp(1, 256);
        let mut r = Raster {
            x0: g[0],
            y0: g[1],
            zx: w / nx as f64,
            zy: h / ny as f64,
            nx,
            ny,
            zellen: vec![Vec::new(); nx * ny],
        };
        for (i, k) in kaesten.iter().enumerate() {
            let (a, b) = (r.zelle(k[0], k[1]), r.zelle(k[2], k[3]));
            for y in a.1..=b.1 {
                for x in a.0..=b.0 {
                    r.zellen[y * nx + x].push(i as u32);
                }
            }
        }
        r
    }

    fn zelle(&self, x: f64, y: f64) -> (usize, usize) {
        let i = |v: f64, z: f64, n: usize| ((v / z).floor().max(0.0) as usize).min(n - 1);
        (
            i(x - self.x0, self.zx, self.nx),
            i(y - self.y0, self.zy, self.ny),
        )
    }

    /// Kästen, die `p` enthalten können (Nummern wie beim Anlegen).
    pub fn bei(&self, p: Vec3) -> &[u32] {
        if self.zellen.is_empty() || !(p.x.is_finite() && p.y.is_finite()) {
            return &[];
        }
        let (x, y) = self.zelle(p.x, p.y);
        &self.zellen[y * self.nx + x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Category, RefSide};

    /// `n` Häuser 10 × 8 m nebeneinander, je eine Innenwand bei x = 3 m.
    fn stadt(n: usize) -> (Model, StoreyId) {
        let mut m = Model::new();
        let st = m.defaults().storey;
        let aw = m.defaults().exterior_wall;
        let iw = m.defaults().interior_wall;
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        for i in 0..n {
            let ox = i as f64 * 15000.0;
            let p = |x: f64, y: f64| vec3(ox + x, y, 0.0);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
                .unwrap();
            let iwp = [p(3000.0, t), p(3000.0, 8000.0 - t)];
            m.add_wall_run(&iwp, false, RefSide::Center, st, iw, Category::InteriorWall)
                .unwrap();
        }
        (m, st)
    }

    /// Fassung je Zug von außen: ändert sich nur, wenn der Test es sagt.
    struct Feste<'a>(&'a Model, HashMap<RunId, u64>);

    impl Ketten for Feste<'_> {
        fn fassung(&self, run: RunId) -> Option<u64> {
            Some(self.1.get(&run).copied().unwrap_or(0))
        }

        fn kette(&self, run: RunId) -> Option<Cow<'_, WallChain>> {
            self.0.chain(run).map(Cow::Owned)
        }
    }

    #[test]
    fn gruppen_je_haus_und_gleich_wie_ohne_speicher() {
        let (m, st) = stadt(3);
        let mut e = Erkennung::default();
        let k = Feste(&m, HashMap::new());
        let l = e.lage(&m, &k, st);
        assert_eq!(l.gruppen.len(), 3, "drei Häuser, drei Gruppen");
        assert!(l.gruppen.iter().all(|g| g.formen.len() == 2));
        assert_eq!(l.raeume(&m), crate::raeume(&m, st));
        // Zweiter Lauf: dieselben Flächen aus dem Speicher
        let l2 = e.lage(&m, &k, st);
        for (a, b) in l.gruppen.iter().zip(&l2.gruppen) {
            assert!(Rc::ptr_eq(&a.formen, &b.formen));
        }
    }

    #[test]
    fn nur_die_geaenderte_gruppe_neu() {
        let (mut m, st) = stadt(3);
        let mut e = Erkennung::default();
        let mut fassung = HashMap::new();
        let vorher = e.lage(&m, &Feste(&m, fassung.clone()), st);
        // Innenwand im mittleren Haus nach x = 4 m
        let iw = m
            .runs()
            .iter()
            .filter(|(_, r)| !r.closed)
            .map(|(id, _)| id)
            .nth(1)
            .unwrap();
        let pts: Vec<Vec3> = m
            .run(iw)
            .unwrap()
            .points
            .iter()
            .map(|p| *p + vec3(1000.0, 0.0, 0.0))
            .collect();
        m.begin("Wand");
        assert!(m.set_run_points(iw, &pts).is_some());
        m.commit();
        fassung.insert(iw, 1);
        let nachher = e.lage(&m, &Feste(&m, fassung), st);
        let gleich: Vec<bool> = vorher
            .gruppen
            .iter()
            .zip(&nachher.gruppen)
            .map(|(a, b)| Rc::ptr_eq(&a.formen, &b.formen))
            .collect();
        assert_eq!(gleich, [true, false, true]);
        assert_eq!(nachher.raeume(&m), crate::raeume(&m, st));
    }

    #[test]
    fn raster_findet_jeden_kasten() {
        let k: Vec<Kasten> = (0..50)
            .map(|i| {
                let x = (i % 10) as f64 * 1000.0;
                let y = (i / 10) as f64 * 700.0;
                [x, y, x + 900.0, y + 600.0]
            })
            .collect();
        let r = Raster::neu(&k);
        for (i, k) in k.iter().enumerate() {
            let p = vec3((k[0] + k[2]) / 2.0, (k[1] + k[3]) / 2.0, 0.0);
            assert!(r.bei(p).contains(&(i as u32)));
            let ecke = vec3(k[2], k[3], 0.0);
            assert!(r.bei(ecke).contains(&(i as u32)));
        }
    }
}
