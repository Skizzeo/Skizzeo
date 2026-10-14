//! Der Ausbau in eigenen Ablagen des Renderers (Review #137 B1): Die Netze
//! der Hausgruppen liegen in [`TEILE`] Teilen, jede Hausgruppe fest nach
//! ihrem ersten Raum in einem. Ändert sich eine Hausgruppe, wird nur ihr
//! Teil neu zusammengesetzt und hochgeladen. Eine neue Schnitthöhe im
//! Grundriss hebt in den Teilen nur die Ecken in Schnitthöhe an, über eine
//! Liste, die jeder Teil beim Zusammensetzen anlegt; die Netze der
//! Hausgruppen bleiben, wie sie gerechnet sind. Das Netz der Ansicht trägt
//! den Ausbau dann nur noch mit Filter; ohne App (Tests, Abnahme) hängt die
//! Szene die Teile an ihr Netz.

use super::{Sicht, HEBEN};
use crate::scene::{MeshData, Scene};
use sk_model::ElementId;

/// Zweierlogarithmus von [`TEILE`].
const BITS: u32 = 4;
/// So viele Teile hat der Ausbau.
pub const TEILE: usize = 1 << BITS;

/// Teil der Hausgruppe, deren erster Raum `id` ist, gestreut über alle.
pub(super) fn teil_von(id: ElementId) -> usize {
    (id.index().wrapping_mul(0x9E37_79B9) >> (32 - BITS)) as usize
}

/// Netz einer Hausgruppe für ihren Teil: Nummer, Netz und die Sicht, in der
/// es gerechnet ist.
pub(super) type Eingang<'a> = (u64, &'a MeshData, Sicht);

/// Ein Teil: die Netze der Hausgruppen darin (Nummern) in ihrer Sicht,
/// zusammengesetzt, und sein Stand (0: nie gefüllt).
#[derive(Default)]
struct Teil {
    netze: Vec<u64>,
    sicht: Option<Sicht>,
    mesh: MeshData,
    /// Grundriss: Ecken und Kantenpunkte (Kante mal zwei und Ende) in
    /// Schnitthöhe.
    oben: (Vec<u32>, Vec<u32>),
    stand: u64,
}

#[derive(Default)]
pub(super) struct Teile {
    teile: [Teil; TEILE],
    /// Die App lädt die Teile selbst ([`Scene::ausbau_liefern`]).
    pub(super) getrennt: bool,
    /// Das letzte Netz der Szene zeigt den Ausbau in den Teilen.
    pub(super) zeigen: bool,
    /// Je Teil der Stand, den die App hat (0: leer); `None`: unbekannt.
    geliefert: [Option<u64>; TEILE],
    /// Zuletzt vergebener Stand.
    stand: u64,
}

impl Teile {
    /// Setzt die Teile aus den Netzen `netze` der Sicht zusammen, je Teil
    /// in der Reihenfolge der Geschosse und Gruppen. Bleiben die Netze
    /// eines Teils dieselben, bleibt er, im Grundriss in neuer Schnitthöhe
    /// angehoben.
    pub(super) fn abgleichen(&mut self, sicht: Sicht, netze: [Vec<Eingang>; TEILE]) {
        self.zeigen = true;
        for (t, n) in self.teile.iter_mut().zip(netze) {
            let nr: Vec<u64> = n.iter().map(|x| x.0).collect();
            if t.netze == nr {
                if t.sicht == Some(sicht) || nr.is_empty() {
                    t.sicht = Some(sicht);
                    continue;
                }
                if let (Some(Sicht::Plan(_)), Sicht::Plan(nach)) = (t.sicht, sicht) {
                    t.heben(nach);
                    t.sicht = Some(sicht);
                    self.stand += 1;
                    t.stand = self.stand;
                    continue;
                }
            }
            t.zusammensetzen(sicht, &n);
            t.netze = nr;
            self.stand += 1;
            t.stand = self.stand;
        }
    }

    /// Hängt alle Teile an `m` (ohne App).
    pub(super) fn anhaengen(&self, m: &mut MeshData) {
        m.faces
            .reserve(self.teile.iter().map(|t| t.mesh.faces.len()).sum());
        m.edges
            .reserve(self.teile.iter().map(|t| t.mesh.edges.len()).sum());
        for t in &self.teile {
            m.faces.extend_from_slice(&t.mesh.faces);
            m.edges.extend_from_slice(&t.mesh.edges);
        }
    }
}

impl Teil {
    /// Setzt den Teil aus den Netzen `n` zusammen. Im Grundriss merkt er
    /// sich die Ecken in Schnitthöhe jedes Netzes und hebt sie auf die der
    /// Sicht, wo das Netz in einer anderen gerechnet ist.
    fn zusammensetzen(&mut self, sicht: Sicht, n: &[Eingang]) {
        let m = &mut self.mesh;
        m.faces.clear();
        m.edges.clear();
        self.oben.0.clear();
        self.oben.1.clear();
        m.faces.reserve(n.iter().map(|x| x.1.faces.len()).sum());
        m.edges.reserve(n.iter().map(|x| x.1.edges.len()).sum());
        for &(_, x, gebaut) in n {
            let (f0, e0) = (m.faces.len(), m.edges.len());
            m.faces.extend_from_slice(&x.faces);
            m.edges.extend_from_slice(&x.edges);
            let (Sicht::Plan(von), Sicht::Plan(nach)) = (gebaut, sicht) else {
                continue;
            };
            let (a, b) = (von as f32, nach as f32);
            let auf = |z: f32| (z - a).abs() < HEBEN;
            for (i, f) in m.faces.iter_mut().enumerate().skip(f0) {
                if auf(f[2]) {
                    self.oben.0.push(i as u32);
                    if a != b {
                        f[2] = b;
                    }
                }
            }
            for (i, e) in m.edges.iter_mut().enumerate().skip(e0) {
                for (k, p) in e.0.iter_mut().enumerate() {
                    if auf(p[2]) {
                        self.oben.1.push((2 * i + k) as u32);
                        if a != b {
                            p[2] = b;
                        }
                    }
                }
            }
        }
        self.sicht = Some(sicht);
    }

    /// Hebt die Ecken in Schnitthöhe auf `nach`.
    fn heben(&mut self, nach: f64) {
        let b = nach as f32;
        for &i in &self.oben.0 {
            self.mesh.faces[i as usize][2] = b;
        }
        for &j in &self.oben.1 {
            let j = j as usize;
            self.mesh.edges[j / 2].0[j % 2][2] = b;
        }
    }
}

impl Scene {
    /// Die App lädt den Ausbau selbst in eigene Ablagen
    /// ([`Scene::ausbau_liefern`]); das Netz der Ansicht trägt ihn dann nur
    /// noch mit Filter.
    pub fn ausbau_getrennt(&mut self) {
        self.ausbau.teile.getrennt = true;
    }

    /// Gibt `f` jeden Teil des Ausbaus, der sich seit dem letzten Aufruf
    /// geändert hat, mit seinem Platz (`0..TEILE`); zeigt das letzte Netz
    /// den Ausbau nicht über die Teile (Filter, Ansicht ohne Ausbau), leer.
    /// Mischt sich das Netz in einem Übergang, bleibt der alte Stand bis zu
    /// dessen Ende, wie beim Ziehen bis zum Loslassen.
    pub fn ausbau_liefern(&mut self, mut f: impl FnMut(usize, &MeshData)) {
        if self.blend_shown {
            return;
        }
        let t = &mut self.ausbau.teile;
        let leer = MeshData::default();
        for (j, teil) in t.teile.iter().enumerate() {
            let (soll, m) = if t.zeigen {
                (teil.stand, &teil.mesh)
            } else {
                (0, &leer)
            };
            if t.geliefert[j] != Some(soll) {
                f(j, m);
                t.geliefert[j] = Some(soll);
            }
        }
    }
}
