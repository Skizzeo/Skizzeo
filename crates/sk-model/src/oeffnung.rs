//! Öffnungen in Außenwänden (Paket O2, Jörn 11.10.): Wandöffnung (Fenster,
//! Terrassentür) und Türöffnung (Haustür) sind dasselbe Bauteil; die Art
//! bleibt für Nummern, Mengen, Kosten und AVA gespeichert.
//!
//! Die Öffnung liegt als Pfad auf der Außenfläche der Wand: Anfang, ggf. die
//! Außenecke, Ende. Sie trifft jede Außenwand, deren Außenfläche auf dem
//! Pfad liegt und deren Höhe sie überdeckt. So schneidet eine
//! geschossübergreifende Öffnung EG- und OG-Wand, eine Ecköffnung beide
//! Wände an der Ecke, und Teilen oder Vereinen einer Wand ändert nichts an
//! ihr. Unter- und Oberkante hängen an den Höhenlinien eines Geschosses
//! (Brüstung, Sturz) oder stehen frei über OK Rohdecke.

use crate::{LevelRef, StoreyId};
use sk_math::Vec3;

/// Art der Öffnung (Jörn 14:04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    /// Wandöffnung: Fenster, Terrassentür.
    Window,
    /// Türöffnung: Haustür.
    Door,
}

impl OpeningKind {
    pub const ALL: [OpeningKind; 2] = [OpeningKind::Window, OpeningKind::Door];

    /// Schlüsselwort in der Datei.
    pub fn word(self) -> &'static str {
        match self {
            OpeningKind::Window => "window",
            OpeningKind::Door => "door",
        }
    }

    /// Name für Menschen.
    pub fn name(self) -> &'static str {
        match self {
            OpeningKind::Window => "Wandöffnung",
            OpeningKind::Door => "Türöffnung",
        }
    }
}

/// Höhenlinie eines Geschosses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightLine {
    Sill,
    Lintel,
    /// Oberkante der Innentüren (Jörn 12.10. 05:26).
    Door,
}

/// Höhe einer Öffnungskante: an eine Höhenlinie gebunden (sie wandert mit,
/// wenn die Linie im Geschossmanager gezogen wird) oder frei.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeightRef {
    Line(StoreyId, HeightLine),
    Level(LevelRef),
}

impl HeightRef {
    /// Geschoss, auf das sich die Höhe bezieht.
    pub fn storey(&self) -> StoreyId {
        match self {
            HeightRef::Line(s, _) => *s,
            HeightRef::Level(r) => r.storey,
        }
    }
}

/// Kleinste Breite und Höhe einer Öffnung (mm).
pub const MIN_OPENING: f64 = 100.0;
/// Größter Anschlag je Seite (mm).
pub const MAX_REVEAL: f64 = 250.0;
/// Anschlag vor einem Verblender (links, rechts, oben, unten; mm): Rahmen
/// im Anschlag der Innenschale, der Verblender deckt ihn (Recherche 14:15).
pub const REVEAL_FACING: [f64; 4] = [60.0, 60.0, 60.0, 0.0];
/// Anschlag hinter einem WDVS (mm): die Laibungsdämmung überdeckt den
/// Rahmen um etwa 3 cm (DIN 4108 Bbl. 2).
pub const REVEAL_ETICS: [f64; 4] = [30.0, 30.0, 30.0, 0.0];
/// Äußere Einbautiefe in einer monolithischen Wand: Außenseite des
/// Elements hinter der Außenseite der Wand (mm, Jörn 11.10. 17:08).
pub const INSTALL_MONO: f64 = 120.0;
/// Mit WDVS oder Verblendschale sitzt das Element so weit hinter der
/// Innenseite der vorgesetzten Schale (mm, Jörn 17:08).
pub const INSTALL_BEHIND_SHELL: f64 = 20.0;
/// Bautiefe des Elements, solange es keine Fenster-, Terrassentür- und
/// Außentürelemente gibt (mm, Jörn 17:09); später vom Element.
pub const ELEMENT_DEPTH: f64 = 80.0;
/// Größte äußere Einbautiefe und Bautiefe (mm).
pub const MAX_INSTALL: f64 = 1000.0;

/// Einbau des Elements in der Öffnung (Jörn 17:08–17:09); `None`: nach
/// Wandtyp bzw. [`ELEMENT_DEPTH`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Install {
    /// Äußere Einbautiefe: Außenseite der Wand bis Außenseite des Elements.
    pub outer: Option<f64>,
    /// Bautiefe des Elements.
    pub element: Option<f64>,
}

/// Aufgelöster Einbau in einem Wandzug (mm): außen bis zum Element, das
/// Element, innen vom Element bis zur Rohbau-Innenseite der Wand (ohne
/// Putz und Bekleidung innen vom Kern; das Raumbuch rechnet den Putz
/// seiner Wandseite dazu). `inner` ist die innere Einbautiefe, so tief
/// wird die Laibung von innen verputzt, tapeziert oder gestrichen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InstallDepth {
    pub outer: f64,
    pub element: f64,
    pub inner: f64,
}

/// Eine Öffnung.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub kind: OpeningKind,
    /// 2 oder 3 Punkte (x, y) auf der Außenfläche: Anfang, ggf. Außenecke,
    /// Ende. Die von außen sichtbare Breite.
    pub path: Vec<Vec3>,
    pub bottom: HeightRef,
    pub top: HeightRef,
    /// Anschlag links, rechts, oben, unten (mm), von außen gesehen; `None`:
    /// nach Wandtyp ([`REVEAL_FACING`], [`REVEAL_ETICS`], monolithisch
    /// keiner).
    pub reveal: Option<[f64; 4]>,
    /// Einbautiefen; leer nach Wandtyp.
    pub install: Install,
    /// Aufschlag und Türhand; nur an einer Tür mit geradem Pfad, `None`
    /// ohne Blatt (Durchgang) und an Fenstern.
    pub swing: Option<Swing>,
    /// Breitenschlösser: je eigenes Pfadende (erstes oder letztes) das Ende
    /// einer Öffnung im Geschoss darüber oder darunter, das mitgeht
    /// ([`crate::Model::end_locks`]); leer ohne Schloss.
    pub lock: Vec<(usize, EndLock)>,
}

/// Gekoppeltes Ende einer anderen Öffnung (Breitenschloss, Jörn 12.10.
/// 05:38): Index ihres Pfadpunkts, erstes oder letztes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndLock {
    pub partner: crate::ElementId,
    pub end: usize,
}

/// Seite quer zu einer Richtung im Grundriss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceSide {
    Left,
    Right,
}

impl FaceSide {
    pub fn other(self) -> FaceSide {
        match self {
            FaceSide::Left => FaceSide::Right,
            FaceSide::Right => FaceSide::Left,
        }
    }

    /// Normale zur Richtung `d` auf dieser Seite (Grundriss, Länge von `d`).
    pub fn normal(self, d: Vec3) -> Vec3 {
        let r = sk_math::vec3(d.y, -d.x, 0.0);
        match self {
            FaceSide::Right => r,
            FaceSide::Left => r * -1.0,
        }
    }
}

/// Türhand nach DIN 107: von der Öffnungsfläche aus gesehen, also aus dem
/// Raum, in den das Blatt aufschlägt, sitzen die Bänder links bzw. rechts
/// (Jörn 12.10. 05:55). In der Ausschreibung das Merkmal
/// „Öffnungsrichtung“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Din {
    Links,
    Rechts,
}

impl Din {
    pub const ALL: [Din; 2] = [Din::Links, Din::Rechts];

    pub fn other(self) -> Din {
        match self {
            Din::Links => Din::Rechts,
            Din::Rechts => Din::Links,
        }
    }

    /// Schlüsselwort in der Datei.
    pub fn word(self) -> &'static str {
        match self {
            Din::Links => "links",
            Din::Rechts => "rechts",
        }
    }

    /// Name für Menschen und Ausschreibung.
    pub fn name(self) -> &'static str {
        match self {
            Din::Links => "DIN links",
            Din::Rechts => "DIN rechts",
        }
    }
}

/// Aufschlag einer Tür (Jörn 12.10. 05:50–05:55): Das Blatt sitzt auf der
/// Seite des Raums, in den es aufschlägt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swing {
    /// Seite des Pfads (Anfang → Ende), zu der das Blatt aufschlägt. Am
    /// Pfad statt am Wandzug: kehrt sich der Zug um, bleibt der Aufschlag
    /// im selben Raum (Integration 05:54).
    pub side: FaceSide,
    pub din: Din,
}

impl Swing {
    /// Aufschlag in den anderen Raum: das Blatt wechselt die Wandseite,
    /// die Bänder bleiben am selben Pfosten und sind von der neuen
    /// Öffnungsfläche aus die andere Hand.
    pub fn flipped(self) -> Swing {
        Swing {
            side: self.side.other(),
            din: self.din.other(),
        }
    }

    /// Bänder am anderen Pfosten, Aufschlag bleibt.
    pub fn other_hand(self) -> Swing {
        Swing {
            din: self.din.other(),
            ..self
        }
    }

    /// Sitzen die Bänder am Anfang des Pfads? Aus dem Aufschlagraum zur
    /// Wand geschaut liegt der Pfadanfang links, wenn das Blatt rechts des
    /// Pfads aufschlägt.
    pub fn hinge_at_start(self) -> bool {
        (self.side == FaceSide::Right) == (self.din == Din::Links)
    }
}

/// Lage des Türblatts in der Wand ([`crate::Model::door_leaf`]): für
/// Raumbuch (Belag, Fuge) und Grundriss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoorLeaf {
    pub run: crate::RunId,
    pub seg: usize,
    /// Lichte längs des Segments, ab seinem Anfang (mm).
    pub u: (f64, f64),
    /// Wandseite des Aufschlags, zur Zeichenrichtung des Segments.
    pub face: FaceSide,
    /// Blattlinie im Grundriss (z = 0): Rohbauflucht der Aufschlagseite,
    /// zugleich die Belagsfuge. Belagsgrenze fürs Raumbuch, nicht für die
    /// Wohnfläche (Türnischen zählen nach WoFlV nicht, Integration 05:54).
    pub line: (Vec3, Vec3),
    /// Einheitsrichtung in den Raum, in den die Tür aufschlägt.
    pub swing_into: Vec3,
    /// Bandpfosten auf der Blattlinie.
    pub hinge: Vec3,
}

impl Opening {
    /// Sichtbare Breite längs des Pfads (mm).
    pub fn width(&self) -> f64 {
        self.path.windows(2).map(|w| (w[1] - w[0]).length()).sum()
    }

    /// Ist `k` ein Pfadende (erster oder letzter Punkt)?
    pub fn is_end(&self, k: usize) -> bool {
        k == 0 || (k > 0 && k + 1 == self.path.len())
    }
}

/// Außenfläche eines Außenwandsegments, auf der eine Öffnung aufgezogen
/// wird (Werkzeug, Paket O3). Punkte auf ihr: `a + d·u + z`.
#[derive(Clone, Debug, PartialEq)]
pub struct OuterFace {
    pub run: crate::RunId,
    pub seg: usize,
    /// Geschoss der Wand: seine Höhenlinien gelten auf der Fläche.
    pub storey: StoreyId,
    /// Punkt der Außenfläche bei `u = 0` (Segmentanfang der Bezugslinie), z = 0.
    pub a: Vec3,
    /// Richtung längs der Wand und Normale nach außen (waagerecht).
    pub d: Vec3,
    pub n: Vec3,
    /// Lage der Ecken der Außenfläche längs `d`.
    pub u: (f64, f64),
    /// Bereich längs `d`, in dem ein freies Ende einer Öffnung liegen darf:
    /// samt Anschlag innerhalb aller Schichtflächen des Segments, also nicht
    /// an der Ecke ([`crate::Model::opening_fault`]).
    pub free: (f64, f64),
    /// Wandfuß und Wandkrone (z absolut).
    pub z: (f64, f64),
}

impl OuterFace {
    /// Lage längs der Fläche.
    pub fn u_of(&self, p: Vec3) -> f64 {
        (p - self.a).dot(self.d)
    }

    /// Punkt der Fläche bei `u` in Höhe `z`.
    pub fn at(&self, u: f64, z: f64) -> Vec3 {
        let p = self.a + self.d * u;
        sk_math::vec3(p.x, p.y, z)
    }

    /// Liegt `p` (waagerecht) auf der Linie der Fläche?
    pub fn on_line(&self, p: Vec3) -> bool {
        (sk_math::vec3(p.x, p.y, 0.0) - self.a).dot(self.n).abs() < 1.0
    }
}

/// Höhenlinie auf einer Fläche: Höhe und woran eine dort gefangene Kante
/// gebunden wird.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceLine {
    pub z: f64,
    pub bind: HeightRef,
}
