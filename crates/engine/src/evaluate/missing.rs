use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::NameId;

/// Missing variables with their scores, in the order a JavaScript object built the same way
/// lists its keys.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Missing(Option<Rc<[(NameId, f64)]>>);

#[derive(Default)]
struct Scratch {
    /// For each name, one more than its position in `merged`, or 0.
    positions: Vec<u32>,
    merged: Vec<(NameId, f64)>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::default();
}

impl Missing {
    pub fn single(name: NameId, score: f64) -> Self {
        Self(Some(Rc::from([(name, score)])))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(NameId, f64)> {
        self.0.iter().flat_map(|entries| entries.iter())
    }

    /// `mergeMissing`: the entries of `self`, then those of `other` it lacks, scores summed.
    pub fn merge(&self, other: &Missing) -> Missing {
        match (&self.0, &other.0) {
            (_, None) => self.clone(),
            (None, _) => other.clone(),
            (Some(a), Some(b)) => SCRATCH.with(|scratch| {
                let Scratch { positions, merged } = &mut *scratch.borrow_mut();
                merged.clear();
                merged.extend_from_slice(a);
                for (index, &(name, _)) in a.iter().enumerate() {
                    *position(positions, name) = index as u32 + 1;
                }
                for &(name, score) in b.iter() {
                    let slot = position(positions, name);
                    match *slot {
                        0 => {
                            merged.push((name, score));
                            *slot = merged.len() as u32;
                        }
                        index => merged[index as usize - 1].1 += score,
                    }
                }
                for &(name, _) in merged.iter() {
                    *position(positions, name) = 0;
                }
                Missing(Some(Rc::from(&merged[..])))
            }),
        }
    }

    /// `bonus`: every score plus one.
    pub fn bonus(&self) -> Missing {
        Missing(self.0.as_ref().map(|entries| {
            entries
                .iter()
                .map(|&(name, score)| (name, score + 1.))
                .collect()
        }))
    }

    pub fn max_score(&self) -> f64 {
        self.iter()
            .fold(0., |max, &(_, score)| if max > score { max } else { score })
    }

    pub fn contains(&self, name: NameId) -> bool {
        self.iter().any(|&(n, _)| n == name)
    }

    pub fn retain(&self, keep: impl Fn(NameId) -> bool) -> Missing {
        let kept: Vec<(NameId, f64)> = self
            .iter()
            .copied()
            .filter(|&(name, _)| keep(name))
            .collect();
        if kept.is_empty() {
            Missing::default()
        } else {
            Missing(Some(Rc::from(kept)))
        }
    }

    /// `missing[name] = score`.
    pub fn with(&self, name: NameId, score: f64) -> Missing {
        let mut entries: Vec<(NameId, f64)> = self.iter().copied().collect();
        match entries.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = score,
            None => entries.push((name, score)),
        }
        Missing(Some(Rc::from(entries)))
    }
}

fn position(positions: &mut Vec<u32>, name: NameId) -> &mut u32 {
    let index = name.0 as usize;
    if index >= positions.len() {
        positions.resize(index + 1, 0);
    }
    &mut positions[index]
}
