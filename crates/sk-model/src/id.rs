//! Laufzeit-Kennungen: Arena mit Generationszähler.
//!
//! Eine [`Id`] besteht aus Platz und Generation. Wird ein Eintrag entfernt, steigt
//! die Generation seines Platzes; alte Kennungen werden dadurch ungültig, statt
//! auf einen späteren Eintrag am selben Platz zu zeigen. Kennungen gelten nur zur
//! Laufzeit und werden nie gespeichert (dafür gibt es die [`crate::Guid`]).

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Kennung eines Eintrags vom Typ `T` in einer [`Arena`].
pub struct Id<T> {
    index: u32,
    gen: u32,
    _t: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    fn new(index: u32, gen: u32) -> Id<T> {
        Id {
            index,
            gen,
            _t: PhantomData,
        }
    }

    /// Platz in der Arena.
    pub fn index(self) -> u32 {
        self.index
    }

    pub fn generation(self) -> u32 {
        self.gen
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Id<T> {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Id<T>) -> bool {
        (self.index, self.gen) == (other.index, other.gen)
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        (self.index, self.gen).hash(h);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index, self.gen)
    }
}

#[derive(Clone, Debug)]
struct Slot<T> {
    gen: u32,
    value: Option<T>,
    /// Nächste nie vergebene Generation dieses Platzes (größer als jede bisher
    /// ausgegebene). Bleibt auch über [`Arena::keep_generations`] erhalten.
    fresh: u32,
}

/// Speicher mit stabilen Kennungen: Einfügen und Zugriff kosten einen Array-Index.
#[derive(Clone, Debug)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Arena<T> {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }
}

impl<T> Arena<T> {
    pub fn new() -> Arena<T> {
        Arena::default()
    }

    pub fn insert(&mut self, value: T) -> Id<T> {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            return Id::new(index, slot.gen);
        }
        self.slots.push(Slot {
            gen: 0,
            value: Some(value),
            fresh: 1,
        });
        Id::new(self.slots.len() as u32 - 1, 0)
    }

    /// Entfernt den Eintrag; die Kennung und alle Kopien davon werden ungültig.
    pub fn remove(&mut self, id: Id<T>) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.gen != id.gen {
            return None;
        }
        let value = slot.value.take()?;
        // Freier Platz bekommt eine nie vergebene Generation
        slot.gen = slot.fresh;
        slot.fresh = slot.fresh.wrapping_add(1);
        self.free.push(id.index);
        self.len -= 1;
        Some(value)
    }

    pub fn get(&self, id: Id<T>) -> Option<&T> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.gen == id.gen {
            slot.value.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.gen == id.gen {
            slot.value.as_mut()
        } else {
            None
        }
    }

    pub fn contains(&self, id: Id<T>) -> bool {
        self.get(id).is_some()
    }

    /// Belegter Eintrag an Platz `index`, gleich welcher Generation.
    pub fn at_index(&self, index: u32) -> Option<(Id<T>, &T)> {
        let slot = self.slots.get(index as usize)?;
        slot.value.as_ref().map(|v| (Id::new(index, slot.gen), v))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Alle Einträge in Platzreihenfolge.
    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.value.as_ref().map(|v| (Id::new(i as u32, s.gen), v)))
    }

    pub fn ids(&self) -> impl Iterator<Item = Id<T>> + '_ {
        self.iter().map(|(id, _)| id)
    }

    /// Bereitet diesen (früheren) Stand darauf vor, den späteren Stand `later` zu
    /// ersetzen (Rückgängig/Wiederholen): Belegte Plätze behalten ihre Kennung,
    /// freie Plätze bekommen eine Generation, die auch `later` nie vergeben hat.
    /// So zeigt eine Kennung aus dem verworfenen Stand nie auf ein späteres
    /// Bauteil am selben Platz.
    pub fn keep_generations(&mut self, later: &Arena<T>) {
        while self.slots.len() < later.slots.len() {
            self.slots.push(Slot {
                gen: 0,
                value: None,
                fresh: 0,
            });
        }
        self.free.clear();
        for (i, slot) in self.slots.iter_mut().enumerate().rev() {
            let fresh = later.slots.get(i).map_or(0, |l| l.fresh).max(slot.fresh);
            if slot.value.is_some() {
                slot.fresh = fresh;
            } else {
                slot.gen = fresh;
                slot.fresh = fresh.wrapping_add(1);
                self.free.push(i as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alte_kennung_wird_ungueltig() {
        let mut a: Arena<&str> = Arena::new();
        let x = a.insert("x");
        let y = a.insert("y");
        assert_eq!(a.get(x), Some(&"x"));
        assert_eq!(a.remove(x), Some("x"));
        assert_eq!(a.get(x), None);
        assert_eq!(a.remove(x), None);
        // Der freie Platz wird wiederverwendet, die alte Kennung zeigt nicht darauf
        let z = a.insert("z");
        assert_eq!(z.index(), x.index());
        assert_ne!(z, x);
        assert_eq!(a.get(x), None);
        assert_eq!(a.get(z), Some(&"z"));
        assert_eq!(a.len(), 2);
        assert_eq!(a.ids().collect::<Vec<_>>(), vec![z, y]);
    }

    #[test]
    fn rueckgaengig_vergibt_keine_alte_kennung() {
        let mut a: Arena<&str> = Arena::new();
        let x = a.insert("x");
        let earlier = a.clone();
        // Im späteren Stand: neuer Eintrag, x gelöscht, Platz neu belegt
        let y = a.insert("y");
        a.remove(x);
        let z = a.insert("z");
        // Zurück zum früheren Stand
        let mut back = earlier;
        back.keep_generations(&a);
        assert_eq!(back.get(x), Some(&"x"));
        assert_eq!(back.get(y), None);
        assert_eq!(back.get(z), None);
        // Neue Einträge bekommen keine der verworfenen Kennungen
        let n1 = back.insert("n1");
        let n2 = back.insert("n2");
        for old in [y, z] {
            assert_ne!(n1, old);
            assert_ne!(n2, old);
        }
        back.remove(x);
        let n3 = back.insert("n3");
        for old in [x, y, z] {
            assert_ne!(n3, old);
        }
        assert_eq!(back.len(), 3);
    }
}
