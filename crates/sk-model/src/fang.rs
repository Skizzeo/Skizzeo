//! Fangziele an Wänden (Grundriss G1, Jörn 11.10. 16:47): Wandflächen und
//! freie Wandenden eines Geschosses, an denen beim Zeichnen einer Innenwand
//! der Cursor einrastet. Methoden von `Model`, aus `model.rs` ausgelagert.
//!
//! Ziele sind die Oberflächen der Wandkörper mit ihren Anschlüssen: beim
//! geschlossenen Ring der Außenwand nur die Innenfläche, sonst beide Flächen
//! (Innenwand, auch geschlossen als Schacht). Schichtfugen fangen nicht.

use super::*;

/// Eine fangbare Wandfläche im Grundriss (z = 0): Strecke auf der Fläche
/// eines Wandsegments.
#[derive(Clone, Debug, PartialEq)]
pub struct Wandflaeche {
    pub wand: ElementId,
    pub a: Vec3,
    pub b: Vec3,
    /// Einheitsnormale von der Wand weg (zum Raum hin).
    pub normale: Vec3,
    /// Name am Gummiband, z. B. „AW-003 innen“ oder „IW-002“.
    pub name: String,
}

impl Wandflaeche {
    /// Einheitsrichtung von `a` nach `b`.
    pub fn richtung(&self) -> Vec3 {
        (self.b - self.a).normalized()
    }

    /// Nächster Punkt der Strecke zu `p` (Grundrissebene).
    pub fn naechster(&self, p: Vec3) -> Vec3 {
        let e = self.b - self.a;
        let len2 = e.x * e.x + e.y * e.y;
        if len2 < 1e-12 {
            return self.a;
        }
        let t = (((p.x - self.a.x) * e.x + (p.y - self.a.y) * e.y) / len2).clamp(0.0, 1.0);
        self.a + e * t
    }
}

/// Freies Ende eines offenen Zuges (Punkt auf der Bezugslinie).
#[derive(Clone, Debug, PartialEq)]
pub struct Wandende {
    pub wand: ElementId,
    pub p: Vec3,
    pub name: String,
}

/// Fangziele eines Geschosses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fangziele {
    pub flaechen: Vec<Wandflaeche>,
    pub enden: Vec<Wandende>,
}

impl Model {
    /// Wandflächen und freie Wandenden im Geschoss `storey`, auf z = 0.
    pub fn fangziele(&self, storey: StoreyId) -> Fangziele {
        self.fangziele_aus(storey, |id| self.chain(id))
    }

    /// Wie [`Model::fangziele`], die Geometrie der Züge aus `chain` (etwa
    /// dem Zwischenspeicher der App, Review #82 V4), statt sie je Zug neu
    /// zu rechnen.
    pub fn fangziele_aus<C: std::borrow::Borrow<WallChain>>(
        &self,
        storey: StoreyId,
        chain: impl Fn(RunId) -> Option<C>,
    ) -> Fangziele {
        let mut out = Fangziele::default();
        // Angeschlossene Enden einmal, nicht je Zug alle Anschlüsse
        let belegt: std::collections::HashSet<(RunId, JoinEnd)> =
            self.joins.iter().map(|j| (j.a_run, j.a_end)).collect();
        for (id, r) in self.runs.iter() {
            if r.storey != storey {
                continue;
            }
            let Some(c) = chain(id) else {
                continue;
            };
            let c = c.borrow();
            let frames = c.segment_frames();
            let name = |k: usize| {
                r.segments
                    .get(k)
                    .and_then(|w| self.element(*w))
                    .map_or_else(String::new, |e| e.number.clone())
            };
            // Ring der Außenwand: nur die Innenfläche; sonst beide Flächen,
            // auch bei einer geschlossen gezeichneten Innenwand (Schacht)
            let (lo, hi) = c.span();
            let ring = c.closed && self.category_of(id) == Some(Category::ExteriorWall);
            let offs = if ring {
                vec![(c.inner_offset(), " innen")]
            } else {
                vec![(lo, ""), (hi, "")]
            };
            for (off, zusatz) in offs {
                let pts = c.face_corners(off);
                let n = pts.len();
                // Normale zur Seite der Fläche, vom Wandkörper weg
                let seite = if off > (lo + hi) * 0.5 { 1.0 } else { -1.0 };
                for (k, (_, d)) in frames.iter().enumerate() {
                    let Some(&wand) = r.segments.get(k) else {
                        continue;
                    };
                    let (a, b) = (pts[k], pts[(k + 1) % n]);
                    if (b - a).length() < 1.0 {
                        continue;
                    }
                    out.flaechen.push(Wandflaeche {
                        wand,
                        a: flat(a),
                        b: flat(b),
                        normale: crate::wall::right_of(*d) * seite,
                        name: format!("{}{zusatz}", name(k)),
                    });
                }
            }
            for e in JoinEnd::BOTH {
                let Some(p) = Model::end_point(r, e) else {
                    continue;
                };
                let frei = !belegt.contains(&(id, e));
                let k = match e {
                    JoinEnd::Start => 0,
                    JoinEnd::End => r.segments.len().saturating_sub(1),
                };
                if let (true, Some(&wand)) = (frei, r.segments.get(k)) {
                    out.enden.push(Wandende {
                        wand,
                        p: flat(p),
                        name: name(k),
                    });
                }
            }
        }
        out
    }
}

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 2750.0;

    /// Haus 10 × 8 m (Außenwand AW-36, Bezug außen) mit einer Innenwand
    /// (Achse) von der Innenfläche unten bis 4 m hoch, freies Ende oben.
    fn haus() -> (Model, StoreyId) {
        let mut m = Model::new();
        let st = m.eg_at(H);
        let aw = m.defaults().exterior_wall;
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let t = m
            .layer_set(aw)
            .map(|s| s.layers.iter().map(|l| l.thickness).sum::<f64>())
            .unwrap();
        let iw = m.defaults().interior_wall;
        m.add_wall_run(
            &[vec3(5000.0, t, 0.0), vec3(5000.0, 4000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
        (m, st)
    }

    #[test]
    fn aussenwand_nur_innenflaeche_innenwand_beide() {
        let (m, st) = haus();
        let z = m.fangziele(st);
        let aussen: Vec<&Wandflaeche> = z
            .flaechen
            .iter()
            .filter(|f| f.name.ends_with(" innen"))
            .collect();
        assert_eq!(aussen.len(), 4, "vier Innenflächen der Außenwand");
        // Die Innenflächen liegen um die Wanddicke innen und zeigen nach innen
        for f in &aussen {
            let mitte = (f.a + f.b) * 0.5;
            let rein = mitte + f.normale * 100.0;
            assert!(rein.x > 0.0 && rein.x < 10000.0 && rein.y > 0.0 && rein.y < 8000.0);
            assert!(mitte.x > 300.0 || mitte.y > 300.0);
        }
        let innen: Vec<&Wandflaeche> = z
            .flaechen
            .iter()
            .filter(|f| !f.name.ends_with(" innen"))
            .collect();
        assert_eq!(innen.len(), 2, "zwei Flächen der Innenwand");
        // Normalen der Innenwandflächen zeigen voneinander weg
        assert!(innen[0].normale.dot(innen[1].normale) < -0.99);
    }

    #[test]
    fn nur_das_freie_ende_faengt() {
        let (m, st) = haus();
        let z = m.fangziele(st);
        // Anfang steht per T an der Außenwand, nur das obere Ende ist frei
        assert_eq!(z.enden.len(), 1);
        assert!((z.enden[0].p - vec3(5000.0, 4000.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn naechster_punkt_bleibt_auf_der_strecke() {
        let (m, st) = haus();
        let f = Wandflaeche {
            a: vec3(0.0, 0.0, 0.0),
            b: vec3(1000.0, 0.0, 0.0),
            ..m.fangziele(st).flaechen[0].clone()
        };
        assert_eq!(f.naechster(vec3(500.0, 300.0, 0.0)), vec3(500.0, 0.0, 0.0));
        assert_eq!(f.naechster(vec3(-50.0, 10.0, 0.0)), vec3(0.0, 0.0, 0.0));
    }

    /// Geometrie (Prüfung #76): Eine geschlossen gezeichnete Innenwand
    /// (Schacht, Kammer im Raum) ist keine Außenwand. Ihre Außenseite liegt
    /// im Raum und muss fangen; nur die Außenwand bietet allein die
    /// Innenfläche an.
    #[test]
    fn geschlossene_innenwand_faengt_aussen() {
        let (mut m, st) = haus();
        let iw = m.defaults().interior_wall;
        let schacht = [
            vec3(2000.0, 2000.0, 0.0),
            vec3(3000.0, 2000.0, 0.0),
            vec3(3000.0, 3000.0, 0.0),
            vec3(2000.0, 3000.0, 0.0),
        ];
        m.add_wall_run(
            &schacht,
            true,
            RefSide::Center,
            st,
            iw,
            Category::InteriorWall,
        )
        .unwrap();
        let z = m.fangziele(st);
        // Außenseite des Schachts: Flächen außerhalb des Rings 2000…3000,
        // Normale vom Schacht weg in den Raum
        let aussen = z
            .flaechen
            .iter()
            .filter(|f| {
                let mitte = (f.a + f.b) * 0.5;
                let raus = mitte + f.normale * 100.0;
                let im_ring =
                    |p: Vec3| p.x > 1900.0 && p.x < 3100.0 && p.y > 1900.0 && p.y < 3100.0;
                let im_schacht =
                    |p: Vec3| p.x > 2000.0 && p.x < 3000.0 && p.y > 2000.0 && p.y < 3000.0;
                im_ring(mitte) && !im_schacht(mitte) && !im_ring(raus)
            })
            .count();
        assert_eq!(aussen, 4, "vier Außenflächen des Schachts");
    }
}
