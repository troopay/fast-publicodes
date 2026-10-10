use rustc_hash::FxHashMap;

use super::Evaluated;
use crate::ast::NodeId;

/// Evaluated nodes. A sub-engine can read the entries of the engines it was lent by.
#[derive(Debug, Default, Clone)]
pub struct Cache {
    entries: FxHashMap<NodeId, Evaluated>,
    /// Entries lent by enclosing engines, innermost last.
    lent: Vec<FxHashMap<NodeId, Evaluated>>,
}

impl Cache {
    pub fn get(&self, id: NodeId) -> Option<&Evaluated> {
        self.entries
            .get(&id)
            .or_else(|| self.lent.iter().rev().find_map(|entries| entries.get(&id)))
    }

    /// The entry of `id`, unless it was lent.
    pub fn get_own_mut(&mut self, id: NodeId) -> Option<&mut Evaluated> {
        self.entries.get_mut(&id)
    }

    pub fn insert(&mut self, id: NodeId, evaluated: Evaluated) {
        self.entries.insert(id, evaluated);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// An empty cache reading this one's entries, which are unavailable here until `restore`.
    pub fn lend(&mut self) -> Cache {
        let mut lent = std::mem::take(&mut self.lent);
        lent.push(std::mem::take(&mut self.entries));
        Cache {
            entries: FxHashMap::default(),
            lent,
        }
    }

    /// Takes back the entries lent to `borrower`.
    pub fn restore(&mut self, borrower: &mut Cache) {
        self.entries = borrower.lent.pop().unwrap_or_default();
        self.lent = std::mem::take(&mut borrower.lent);
    }

    /// A copy of every entry this cache reads.
    pub fn snapshot(&self) -> Cache {
        let mut entries = FxHashMap::default();
        for lent in &self.lent {
            entries.extend(lent.iter().map(|(id, e)| (*id, e.clone())));
        }
        entries.extend(self.entries.iter().map(|(id, e)| (*id, e.clone())));
        Cache {
            entries,
            lent: Vec::new(),
        }
    }
}
