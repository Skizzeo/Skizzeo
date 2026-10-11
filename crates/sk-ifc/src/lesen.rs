//! IFC-Datei lesen für die Vorschau (ifc/plan.md §6, I3): aus einer gelesenen
//! STEP-Datei eine neutrale Liste, die die App zeichnet. Die Vorschau zeigt
//! die Datei, nicht das Modell: Was in der Datei fehlt, fehlt auch hier.
//!
//! Verstanden wird, was Reference View an Form erlaubt und Skizzeo schreibt:
//! Dreiecksnetze (`IfcTriangulatedFaceSet`) an Platzierungsketten
//! (`IfcLocalPlacement`), Farben über `IfcStyledItem`. Andere Formen (Extrusion,
//! Flächenmodell fremder Dateien) zählt [`Vorschau::ohne_form`]; sie kommen mit
//! I7.

use crate::step::{File, V};
use sk_math::{vec3, Vec3};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Inhalt einer IFC-Datei, wie ihn ein Empfänger sieht.
#[derive(Clone, Debug, Default)]
pub struct Vorschau {
    pub schema: String,
    pub projekt: String,
    /// Grundstück › Gebäude › Geschosse (und was sonst aggregiert ist).
    pub baum: Vec<Knoten>,
    pub teile: Vec<Teil>,
    /// Bauteile mit Darstellung, deren Form oder Platzierung die Vorschau
    /// (noch) nicht liest.
    pub ohne_form: usize,
}

/// Ein Ort der räumlichen Gliederung.
#[derive(Clone, Debug, Default)]
pub struct Knoten {
    /// IFC-Klasse wie in der Datei, z. B. `IFCBUILDINGSTOREY`.
    pub klasse: String,
    pub name: String,
    pub langname: String,
    /// Höhe des Geschosses (mm), sonst 0.
    pub hoehe: f64,
    pub kinder: Vec<Knoten>,
    /// Indizes in [`Vorschau::teile`], die hier enthalten sind.
    pub teile: Vec<usize>,
}

/// Ein Bauteil mit Form und Angaben.
#[derive(Clone, Debug, Default)]
pub struct Teil {
    pub guid: String,
    pub klasse: String,
    pub name: String,
    pub beschreibung: String,
    pub art: String,
    pub typ: String,
    /// Netze in Weltkoordinaten (mm), je Farbe eines.
    pub netze: Vec<Netz>,
    /// Schichten (Baustoff, Dicke mm), aus dem Bauteil oder seinem Typ.
    pub schichten: Vec<(String, f64)>,
    /// Baustoffe ohne Schichtdicke (Einzelbaustoff, Anteile).
    pub baustoffe: Vec<String>,
    /// Klassifikationen „DIN 276 · 331“.
    pub klassen: Vec<String>,
    /// Eigenschaftssätze mit (Name, Wert als Text).
    pub saetze: Vec<(String, Vec<(String, String)>)>,
}

#[derive(Clone, Debug, Default)]
pub struct Netz {
    /// Farbe 0..1, ohne Stil grau.
    pub farbe: [f32; 3],
    pub punkte: Vec<Vec3>,
    pub dreiecke: Vec<[u32; 3]>,
}

/// Lage einer Platzierung: Ursprung und drei Achsen.
#[derive(Clone, Copy, Debug)]
struct Lage {
    o: Vec3,
    x: Vec3,
    y: Vec3,
    z: Vec3,
}

impl Lage {
    const NULL: Lage = Lage {
        o: Vec3::ZERO,
        x: Vec3::X,
        y: vec3(0.0, 1.0, 0.0),
        z: vec3(0.0, 0.0, 1.0),
    };

    fn punkt(&self, p: Vec3) -> Vec3 {
        self.o + self.x * p.x + self.y * p.y + self.z * p.z
    }

    /// `innen` relativ zu `self`.
    fn dann(&self, innen: Lage) -> Lage {
        let achse = |v: Vec3| self.x * v.x + self.y * v.y + self.z * v.z;
        Lage {
            o: self.punkt(innen.o),
            x: achse(innen.x),
            y: achse(innen.y),
            z: achse(innen.z),
        }
    }
}

const RAEUMLICH: [&str; 5] = [
    "IFCPROJECT",
    "IFCSITE",
    "IFCBUILDING",
    "IFCBUILDINGSTOREY",
    "IFCSPACE",
];

struct Leser<'a> {
    f: &'a File,
    /// Länge einer Einheit der Datei in mm.
    mm: f64,
    lagen: HashMap<u32, Lage>,
    farben: HashMap<u32, [f32; 3]>,
}

fn text(v: Option<&V>) -> String {
    match v {
        Some(V::Str(t)) => t.clone(),
        Some(V::Typed(_, x)) => text(Some(x)),
        _ => String::new(),
    }
}

/// Zahl der Datei; nicht endliche (`1.E999`) gelten als fehlend.
fn zahl(v: Option<&V>) -> Option<f64> {
    match v? {
        V::Real(x) | V::Len(x) => x.is_finite().then_some(*x),
        V::Int(i) => Some(*i as f64),
        V::Typed(_, x) => zahl(Some(x)),
        _ => None,
    }
}

fn endlich(p: Vec3) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.z.is_finite()
}

fn verweis(v: Option<&V>) -> Option<u32> {
    match v? {
        V::Ref(i) => Some(*i),
        _ => None,
    }
}

fn verweise(v: Option<&V>) -> Vec<u32> {
    match v {
        Some(V::List(xs)) => xs.iter().filter_map(|x| verweis(Some(x))).collect(),
        Some(V::Ref(i)) => vec![*i],
        _ => Vec::new(),
    }
}

/// Wert einer Eigenschaft als Text, wie ihn ein Viewer zeigt.
fn wert(v: &V) -> String {
    match v {
        V::Str(t) | V::Enum(t) => t.clone(),
        V::Real(x) | V::Len(x) => {
            let t = format!("{:.4}", x);
            let t = t.trim_end_matches('0').trim_end_matches('.');
            t.replace('.', ",")
        }
        V::Int(i) => i.to_string(),
        V::Bool(true) => "ja".into(),
        V::Bool(false) => "nein".into(),
        V::Unknown => "unbekannt".into(),
        V::Typed(_, x) => wert(x),
        V::List(xs) => xs.iter().map(wert).collect::<Vec<_>>().join("; "),
        _ => String::new(),
    }
}

impl Leser<'_> {
    fn args(&self, id: u32) -> &[V] {
        self.f.get(id).map_or(&[], |e| e.args.as_slice())
    }

    fn name(&self, id: u32) -> &str {
        self.f.get(id).map_or("", |e| e.name.as_str())
    }

    fn vektor(&self, id: Option<u32>, sonst: Vec3) -> Vec3 {
        let Some(id) = id else { return sonst };
        let a = self.args(id);
        let k: Vec<f64> = match a.first() {
            Some(V::List(xs)) => xs.iter().filter_map(|x| zahl(Some(x))).collect(),
            _ => Vec::new(),
        };
        match k.as_slice() {
            [x, y, z] => vec3(*x, *y, *z),
            [x, y] => vec3(*x, *y, 0.0),
            _ => sonst,
        }
    }

    /// `IfcAxis2Placement3D` (bzw. 2D) als Lage; Ursprung in mm. Eine
    /// Nullrichtung oder eine x-Richtung parallel zu z fällt auf die Vorgabe
    /// zurück, statt alle Punkte auf den Ursprung zu legen.
    fn achsen(&self, id: u32) -> Lage {
        let a = self.args(id);
        let o = self.vektor(verweis(a.first()), Vec3::ZERO) * self.mm;
        let o = if endlich(o) { o } else { Vec3::ZERO };
        let einheit = |v: Vec3, sonst: Vec3| {
            let l = v.length();
            if l.is_finite() && l > 1e-9 {
                v / l
            } else {
                sonst
            }
        };
        let z = einheit(
            self.vektor(verweis(a.get(1)), vec3(0.0, 0.0, 1.0)),
            vec3(0.0, 0.0, 1.0),
        );
        let x0 = self.vektor(verweis(a.get(2)), Vec3::X);
        // x senkrecht zu z (wie IfcBuildAxes)
        let quer = |v: Vec3| v - z * v.dot(z);
        let x = einheit(
            quer(x0),
            einheit(quer(Vec3::X), einheit(quer(vec3(0.0, 1.0, 0.0)), Vec3::X)),
        );
        let y = z.cross(x);
        Lage { o, x, y, z }
    }

    /// Lage einer `IfcLocalPlacement` mit ihrer Kette.
    fn lage(&mut self, id: u32, tiefe: u32) -> Lage {
        if let Some(l) = self.lagen.get(&id) {
            return *l;
        }
        let a = self.args(id).to_vec();
        let l = if self.name(id) != "IFCLOCALPLACEMENT" || tiefe > 64 {
            Lage::NULL
        } else {
            let aussen = verweis(a.first()).map_or(Lage::NULL, |p| self.lage(p, tiefe + 1));
            let innen = verweis(a.get(1)).map_or(Lage::NULL, |p| self.achsen(p));
            aussen.dann(innen)
        };
        self.lagen.insert(id, l);
        l
    }

    fn farbe(&self, item: u32) -> [f32; 3] {
        self.farben.get(&item).copied().unwrap_or([0.7, 0.7, 0.7])
    }

    /// Farben aus `IfcStyledItem` → `IfcSurfaceStyle` →
    /// `IfcSurfaceStyleShading`/`Rendering` → `IfcColourRgb` (auch über die
    /// IFC2x3-Zwischenstufe `IfcPresentationStyleAssignment`).
    fn farben_lesen(&mut self) {
        let mut farben = HashMap::new();
        for (_, e) in self.f.all("IFCSTYLEDITEM") {
            let Some(item) = verweis(e.args.first()) else {
                continue;
            };
            let mut offen = verweise(e.args.get(1));
            let mut tiefe = 0;
            while let Some(s) = offen.pop() {
                tiefe += 1;
                if tiefe > 32 {
                    break;
                }
                let a = self.args(s);
                match self.name(s) {
                    "IFCPRESENTATIONSTYLEASSIGNMENT" => offen.extend(verweise(a.first())),
                    "IFCSURFACESTYLE" => offen.extend(verweise(a.get(2))),
                    "IFCSURFACESTYLESHADING" | "IFCSURFACESTYLERENDERING" => {
                        if let Some(c) = verweis(a.first()) {
                            let c = self.args(c);
                            let k = |i| zahl(c.get(i)).unwrap_or(0.7) as f32;
                            farben.insert(item, [k(1), k(2), k(3)]);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        self.farben = farben;
    }

    /// Netze eines Produkts aus seiner Darstellung; `None`, wenn es eine
    /// Form hat, die die Vorschau nicht liest.
    fn netze(&mut self, shape: u32, lage: Lage) -> Option<Vec<Netz>> {
        let mut netze = Vec::new();
        let mut fremd = false;
        for rep in verweise(self.args(shape).get(2)) {
            let a = self.args(rep);
            let kennung = text(a.get(1));
            if kennung != "Body" && kennung != "Facetation" {
                continue;
            }
            for item in verweise(a.get(3)) {
                if self.name(item) != "IFCTRIANGULATEDFACESET" {
                    fremd = true;
                    continue;
                }
                let ia = self.args(item);
                let Some(pl) = verweis(ia.first()) else {
                    continue;
                };
                // Punkte, die nicht endlich sind (oder keine drei Zahlen
                // haben), sind `None`; Dreiecke an ihnen fallen weg
                let roh: Vec<Option<Vec3>> = match self.args(pl).first() {
                    Some(V::List(ps)) => ps
                        .iter()
                        .map(|p| match p {
                            V::List(k) if k.len() == 3 => {
                                let k: Vec<f64> = k.iter().filter_map(|x| zahl(Some(x))).collect();
                                let q =
                                    lage.punkt(vec3(*k.first()?, *k.get(1)?, *k.get(2)?) * self.mm);
                                endlich(q).then_some(q)
                            }
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let nummer = |x: &V| match x {
                    V::Int(i) => i
                        .checked_sub(1)
                        .and_then(|i| usize::try_from(i).ok())
                        .filter(|&i| roh.get(i).is_some_and(Option::is_some)),
                    _ => None,
                };
                let mut neu: Vec<Option<u32>> = vec![None; roh.len()];
                let mut punkte = Vec::new();
                let dreiecke: Vec<[u32; 3]> = match ia.get(3) {
                    Some(V::List(ts)) => ts
                        .iter()
                        .filter_map(|t| match t {
                            V::List(k) if k.len() == 3 => {
                                let k = [nummer(&k[0])?, nummer(&k[1])?, nummer(&k[2])?];
                                Some(k.map(|i| {
                                    *neu[i].get_or_insert_with(|| {
                                        punkte.push(roh[i].unwrap_or_default());
                                        (punkte.len() - 1) as u32
                                    })
                                }))
                            }
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if !dreiecke.is_empty() {
                    netze.push(Netz {
                        farbe: self.farbe(item),
                        punkte,
                        dreiecke,
                    });
                }
            }
        }
        if netze.is_empty() && fremd {
            None
        } else {
            Some(netze)
        }
    }
}

/// Einheit der Länge in mm aus `IfcUnitAssignment` des Projekts.
fn laenge_mm(f: &File) -> f64 {
    for (_, e) in f.all("IFCSIUNIT") {
        if e.args.get(1) == Some(&V::Enum("LENGTHUNIT".into())) {
            return match e.args.get(2) {
                Some(V::Enum(p)) if p == "MILLI" => 1.0,
                Some(V::Enum(p)) if p == "CENTI" => 10.0,
                Some(V::Enum(p)) if p == "DECI" => 100.0,
                _ => 1000.0,
            };
        }
    }
    1.0
}

/// Vorschau aus einer gelesenen Datei.
pub fn vorschau(f: &File) -> Vorschau {
    let mut l = Leser {
        f,
        mm: laenge_mm(f),
        lagen: HashMap::new(),
        farben: HashMap::new(),
    };
    l.farben_lesen();

    // Bauteile: Produkte mit Platzierung und Darstellung, außer Gliederung
    let mut v = Vorschau {
        schema: f.schema().unwrap_or("").to_string(),
        ..Vorschau::default()
    };
    let mut index: HashMap<u32, usize> = HashMap::new();
    for (&id, e) in &f.data {
        if RAEUMLICH[..4].contains(&e.name.as_str()) {
            continue;
        }
        let Some(sh) = verweis(e.args.get(6)) else {
            continue;
        };
        if l.name(sh) != "IFCPRODUCTDEFINITIONSHAPE" {
            continue;
        }
        // ohne lesbare Platzierung keine Lage: zählt wie eine fremde Form
        let pl = verweis(e.args.get(5)).filter(|&p| l.name(p) == "IFCLOCALPLACEMENT");
        let Some(pl) = pl else {
            v.ohne_form += 1;
            continue;
        };
        let lage = l.lage(pl, 0);
        let Some(netze) = l.netze(sh, lage) else {
            v.ohne_form += 1;
            continue;
        };
        index.insert(id, v.teile.len());
        v.teile.push(Teil {
            guid: text(e.args.first()),
            klasse: schreibweise(&e.name),
            name: text(e.args.get(2)),
            beschreibung: text(e.args.get(3)),
            art: text(e.args.get(4)),
            netze,
            ..Teil::default()
        });
    }

    // Typen, Baustoffe, Klassifikation, Eigenschaften
    let mut typ_von: HashMap<u32, u32> = HashMap::new();
    for (_, e) in f.all("IFCRELDEFINESBYTYPE") {
        let Some(t) = verweis(e.args.get(5)) else {
            continue;
        };
        for x in verweise(e.args.get(4)) {
            typ_von.insert(x, t);
            if let Some(&i) = index.get(&x) {
                v.teile[i].typ = text(l.args(t).get(2));
            }
        }
    }
    let mut stoff_von: HashMap<u32, u32> = HashMap::new();
    for (_, e) in f.all("IFCRELASSOCIATESMATERIAL") {
        if let Some(m) = verweis(e.args.get(5)) {
            for x in verweise(e.args.get(4)) {
                stoff_von.insert(x, m);
            }
        }
    }
    for (&id, &i) in &index {
        let m = stoff_von
            .get(&id)
            .or_else(|| typ_von.get(&id).and_then(|t| stoff_von.get(t)));
        let Some(&m) = m else { continue };
        let (schichten, baustoffe) = baustoffe(&l, m, 0);
        v.teile[i].schichten = schichten;
        v.teile[i].baustoffe = baustoffe;
    }
    for (_, e) in f.all("IFCRELASSOCIATESCLASSIFICATION") {
        let Some(c) = verweis(e.args.get(5)) else {
            continue;
        };
        let ca = l.args(c);
        let quelle = verweis(ca.get(3)).map_or(String::new(), |q| text(l.args(q).get(3)));
        let kennung = text(ca.get(1));
        let name = text(ca.get(2));
        let mut t = if quelle.is_empty() {
            kennung
        } else {
            format!("{quelle} · {kennung}")
        };
        if !name.is_empty() {
            t = format!("{t} {name}");
        }
        for x in verweise(e.args.get(4)) {
            if let Some(&i) = index.get(&x) {
                v.teile[i].klassen.push(t.clone());
            }
        }
    }
    for (_, e) in f.all("IFCRELDEFINESBYPROPERTIES") {
        let Some(ps) = verweis(e.args.get(5)) else {
            continue;
        };
        let pa = l.args(ps);
        let satz = text(pa.get(2));
        let mut werte = Vec::new();
        for p in verweise(pa.get(4)) {
            let a = l.args(p);
            let w = match l.name(p) {
                "IFCPROPERTYSINGLEVALUE" => a.get(2).map(wert).unwrap_or_default(),
                // Mengen: Name, Beschreibung, Einheit, Wert
                n if n.starts_with("IFCQUANTITY") => a.get(3).map(wert).unwrap_or_default(),
                _ => continue,
            };
            werte.push((text(a.first()), w));
        }
        for x in verweise(e.args.get(4)) {
            if let Some(&i) = index.get(&x) {
                v.teile[i].saetze.push((satz.clone(), werte.clone()));
            }
        }
    }
    for t in &mut v.teile {
        t.saetze.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Gliederung
    let mut kinder: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (_, e) in f.all("IFCRELAGGREGATES") {
        if let Some(g) = verweis(e.args.get(4)) {
            for k in verweise(e.args.get(5)) {
                kinder.entry(g).or_default().push(k);
            }
        }
    }
    let mut enthalten: HashMap<u32, Vec<usize>> = HashMap::new();
    for (_, e) in f.all("IFCRELCONTAINEDINSPATIALSTRUCTURE") {
        if let Some(s) = verweis(e.args.get(5)) {
            for x in verweise(e.args.get(4)) {
                if let Some(&i) = index.get(&x) {
                    enthalten.entry(s).or_default().push(i);
                }
            }
        }
    }
    /// Ein Ort mit allem darunter. `besucht` hält jeden Ort nur einmal im
    /// Baum, auch wenn die Datei Zyklen oder doppelte Verweise hat.
    fn knoten(
        l: &Leser,
        id: u32,
        kinder: &BTreeMap<u32, Vec<u32>>,
        enthalten: &HashMap<u32, Vec<usize>>,
        index: &HashMap<u32, usize>,
        besucht: &mut HashSet<u32>,
    ) -> Knoten {
        let a = l.args(id);
        let name = l.name(id).to_string();
        let mut k = Knoten {
            hoehe: if name == "IFCBUILDINGSTOREY" {
                zahl(a.get(9))
                    .map(|h| h * l.mm)
                    .filter(|h| h.is_finite())
                    .unwrap_or(0.0)
            } else {
                0.0
            },
            klasse: schreibweise(&name),
            name: text(a.get(2)),
            langname: text(a.get(7)),
            kinder: Vec::new(),
            teile: enthalten.get(&id).cloned().unwrap_or_default(),
        };
        for &c in kinder.get(&id).into_iter().flatten() {
            if !besucht.insert(c) {
                continue;
            }
            if let Some(&i) = index.get(&c) {
                // Räume mit Form: als Teil unter dem Geschoss
                k.teile.push(i);
            }
            if RAEUMLICH.contains(&l.name(c)) {
                k.kinder
                    .push(knoten(l, c, kinder, enthalten, index, besucht));
            }
        }
        if name == "IFCBUILDINGSTOREY" {
            k.kinder.sort_by(|a, b| a.name.cmp(&b.name));
        }
        k
    }
    if let Some((p, e)) = f.all("IFCPROJECT").next() {
        v.projekt = text(e.args.get(2));
        let mut besucht = HashSet::from([p]);
        for &c in kinder.get(&p).into_iter().flatten() {
            if besucht.insert(c) {
                let k = knoten(&l, c, &kinder, &enthalten, &index, &mut besucht);
                v.baum.push(k);
            }
        }
    }
    v
}

/// Schreibweise im Schema („IFCWALL“ → „IfcWall“) für die Klassen, die
/// Skizzeo schreibt, und die räumlichen; andere wie in der Datei.
pub fn schreibweise(name: &str) -> String {
    const RAEUME: [&str; 5] = [
        "IfcProject",
        "IfcSite",
        "IfcBuilding",
        "IfcBuildingStorey",
        "IfcSpace",
    ];
    crate::klassen::lookup(name)
        .map(|k| k.name)
        .or_else(|| RAEUME.into_iter().find(|r| r.eq_ignore_ascii_case(name)))
        .map_or_else(|| name.to_string(), str::to_string)
}

/// Schichten oder Baustoffe einer Baustoffzuordnung; `tiefe` begrenzt
/// Verweisketten (eine Nutzung, die auf sich selbst zeigt).
fn baustoffe(l: &Leser, m: u32, tiefe: u32) -> (Vec<(String, f64)>, Vec<String>) {
    let stoffname = |id: Option<u32>| id.map_or(String::new(), |i| text(l.args(i).first()));
    let a = l.args(m);
    match l.name(m) {
        "IFCMATERIALLAYERSETUSAGE" if tiefe < 4 => {
            verweis(a.first()).map_or_else(Default::default, |s| baustoffe(l, s, tiefe + 1))
        }
        "IFCMATERIALLAYERSET" => {
            let schichten = verweise(a.first())
                .into_iter()
                .map(|s| {
                    let sa = l.args(s);
                    (
                        stoffname(verweis(sa.first())),
                        zahl(sa.get(1)).unwrap_or(0.0) * l.mm,
                    )
                })
                .collect();
            (schichten, Vec::new())
        }
        "IFCMATERIALCONSTITUENTSET" => {
            let teile = verweise(a.get(2))
                .into_iter()
                .map(|c| stoffname(verweis(l.args(c).get(2))))
                .collect();
            (Vec::new(), teile)
        }
        "IFCMATERIAL" => (Vec::new(), vec![text(a.first())]),
        "IFCMATERIALLIST" => (
            Vec::new(),
            verweise(a.first())
                .into_iter()
                .map(|x| stoffname(Some(x)))
                .collect(),
        ),
        _ => Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step;

    const DATEI: &str = "ISO-10303-21;\nHEADER;FILE_SCHEMA(('IFC4'));ENDSEC;\nDATA;\n\
        #1=IFCPROJECT('p',$,'Haus',$,$,$,$,$,#2);\n\
        #2=IFCUNITASSIGNMENT((#3));\n\
        #3=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n\
        #10=IFCBUILDING('b',$,'Haus 1',$,$,$,$,$,.ELEMENT.,$,$,$);\n\
        #11=IFCBUILDINGSTOREY('s',$,'EG',$,$,#21,$,'Erdgeschoss',.ELEMENT.,3.);\n\
        #12=IFCRELAGGREGATES('a',$,$,$,#1,(#10));\n\
        #13=IFCRELAGGREGATES('a2',$,$,$,#10,(#11));\n\
        #20=IFCCARTESIANPOINT((0.,0.,3.));\n\
        #22=IFCAXIS2PLACEMENT3D(#20,$,$);\n\
        #21=IFCLOCALPLACEMENT($,#22);\n\
        #23=IFCCARTESIANPOINT((1.,0.,0.));\n\
        #24=IFCDIRECTION((0.,1.,0.));\n\
        #25=IFCAXIS2PLACEMENT3D(#23,$,#24);\n\
        #26=IFCLOCALPLACEMENT(#21,#25);\n\
        #30=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(0.,1.,0.)));\n\
        #31=IFCTRIANGULATEDFACESET(#30,$,$,((1,2,3)),$);\n\
        #32=IFCSHAPEREPRESENTATION($,'Body','Tessellation',(#31));\n\
        #33=IFCPRODUCTDEFINITIONSHAPE($,$,(#32));\n\
        #34=IFCCOLOURRGB($,1.,0.5,0.);\n\
        #35=IFCSURFACESTYLESHADING(#34,0.);\n\
        #36=IFCSURFACESTYLE($,.BOTH.,(#35));\n\
        #37=IFCSTYLEDITEM(#31,(#36),$);\n\
        #40=IFCWALL('w',$,'AW-001','Au\\X2\\00DF\\X0\\enwand',$,#26,#33,'AW-001',.NOTDEFINED.);\n\
        #41=IFCRELCONTAINEDINSPATIALSTRUCTURE('c',$,$,$,(#40),#11);\n\
        #50=IFCMATERIAL('Kalksandstein',$,$);\n\
        #51=IFCMATERIALLAYER(#50,0.175,.F.,$,$,$,$);\n\
        #52=IFCMATERIALLAYERSET((#51),'AW 17,5',$);\n\
        #53=IFCRELASSOCIATESMATERIAL('m',$,$,$,(#40),#52);\n\
        #60=IFCPROPERTYSINGLEVALUE('IsExternal',$,IFCBOOLEAN(.T.),$);\n\
        #61=IFCPROPERTYSET('ps',$,'Pset_WallCommon',$,(#60));\n\
        #62=IFCRELDEFINESBYPROPERTIES('d',$,$,$,(#40),#61);\n\
        ENDSEC;\nEND-ISO-10303-21;";

    #[test]
    fn datei_als_vorschau() {
        let f = step::read(DATEI).unwrap();
        let v = vorschau(&f);
        assert_eq!(v.projekt, "Haus");
        assert_eq!(v.teile.len(), 1);
        let t = &v.teile[0];
        assert_eq!((t.klasse.as_str(), t.name.as_str()), ("IfcWall", "AW-001"));
        assert_eq!(t.beschreibung, "Außenwand");
        // Meter → mm; Kette: Geschoss z=3 m, Wand bei x=1 m, gedreht um 90°
        let p = &t.netze[0].punkte;
        assert!((p[0] - vec3(1000.0, 0.0, 3000.0)).length() < 1e-6);
        assert!((p[1] - vec3(1000.0, 1000.0, 3000.0)).length() < 1e-6);
        assert!((p[2] - vec3(0.0, 0.0, 3000.0)).length() < 1e-6);
        assert_eq!(t.netze[0].farbe, [1.0, 0.5, 0.0]);
        assert_eq!(t.schichten, vec![("Kalksandstein".to_string(), 175.0)]);
        assert_eq!(
            t.saetze[0].1,
            vec![("IsExternal".to_string(), "ja".to_string())]
        );
        // Baum: Gebäude › Geschoss mit der Wand
        let eg = &v.baum[0].kinder[0];
        assert_eq!(
            (eg.name.as_str(), eg.langname.as_str()),
            ("EG", "Erdgeschoss")
        );
        assert_eq!(eg.hoehe, 3000.0);
        assert_eq!(eg.teile, vec![0]);
    }

    #[test]
    fn fremde_form_wird_gezaehlt() {
        let t = DATEI.replace(
            "#31=IFCTRIANGULATEDFACESET(#30,$,$,((1,2,3)),$);",
            "#31=IFCEXTRUDEDAREASOLID($,$,$,1.);",
        );
        let v = vorschau(&step::read(&t).unwrap());
        assert_eq!((v.teile.len(), v.ohne_form), (0, 1));
    }

    /// Review B1 zu #83: eine Baustoffnutzung, die auf sich selbst zeigt,
    /// hält den Leser nicht an.
    #[test]
    fn zyklischer_baustoff() {
        let t = DATEI.replace(
            "#53=IFCRELASSOCIATESMATERIAL('m',$,$,$,(#40),#52);",
            "#53=IFCRELASSOCIATESMATERIAL('m',$,$,$,(#40),#54);\n\
             #54=IFCMATERIALLAYERSETUSAGE(#54,.AXIS2.,.POSITIVE.,0.,$);",
        );
        let v = vorschau(&step::read(&t).unwrap());
        assert!(v.teile[0].schichten.is_empty());
    }

    /// Review B2 zu #83: Zyklen und doppelte Verweise in der Gliederung
    /// geben jeden Ort nur einmal aus.
    #[test]
    fn zyklische_gliederung() {
        let t = DATEI.replace(
            "#13=IFCRELAGGREGATES('a2',$,$,$,#10,(#11));",
            "#13=IFCRELAGGREGATES('a2',$,$,$,#10,(#10,#10,#10,#11,#11));\n\
             #14=IFCRELAGGREGATES('a3',$,$,$,#11,(#10,#1,#11));",
        );
        let v = vorschau(&step::read(&t).unwrap());
        fn zahl(k: &[Knoten]) -> usize {
            k.iter().map(|k| 1 + zahl(&k.kinder)).sum()
        }
        assert_eq!(zahl(&v.baum), 2);
        assert_eq!(v.baum[0].kinder[0].teile, vec![0]);
    }

    /// Review B7 zu #83: Werte aus fremden Dateien. Nicht endliche Punkte
    /// fallen mit ihren Dreiecken weg, eine Nullrichtung gilt als Vorgabe,
    /// zu große Nummern laufen nicht über, ein Bauteil ohne Platzierung
    /// zählt ohne Form.
    #[test]
    fn kaputte_werte() {
        let t = DATEI
            .replace(
                "#30=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(0.,1.,0.)));",
                "#30=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(0.,1.,0.),(1.E999,0.,0.)));",
            )
            .replace(
                "((1,2,3)),$);",
                "((1,2,3),(1,2,4),(1,2,4294967297),(1,2,-9223372036854775808)),$);",
            )
            .replace(
                "#24=IFCDIRECTION((0.,1.,0.));",
                "#24=IFCDIRECTION((0.,0.,0.));",
            );
        let v = vorschau(&step::read(&t).unwrap());
        let n = &v.teile[0].netze[0];
        assert_eq!((n.punkte.len(), n.dreiecke.len()), (3, 1));
        // x-Richtung null: Vorgabe x, also nicht gedreht
        assert!((n.punkte[1] - vec3(2000.0, 0.0, 3000.0)).length() < 1e-6);
        assert!(n.punkte.iter().all(|p| endlich(*p)));

        let t = DATEI.replace(",#26,#33,'AW-001'", ",$,#33,'AW-001'");
        let v = vorschau(&step::read(&t).unwrap());
        assert_eq!((v.teile.len(), v.ohne_form), (0, 1));
    }
}
