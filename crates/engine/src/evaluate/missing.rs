use std::rc::Rc;

use crate::ast::NameId;

/// Missing variables with their scores, sorted by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Missing(Option<Rc<[(NameId, f64)]>>);

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

    /// Sums the scores of both.
    pub fn merge(&self, other: &Missing) -> Missing {
        match (&self.0, &other.0) {
            (_, None) => self.clone(),
            (None, _) => other.clone(),
            (Some(a), Some(b)) => {
                let mut merged = Vec::with_capacity(a.len() + b.len());
                let (mut i, mut j) = (0, 0);
                while i < a.len() && j < b.len() {
                    match a[i].0.cmp(&b[j].0) {
                        std::cmp::Ordering::Less => {
                            merged.push(a[i]);
                            i += 1;
                        }
                        std::cmp::Ordering::Greater => {
                            merged.push(b[j]);
                            j += 1;
                        }
                        std::cmp::Ordering::Equal => {
                            merged.push((a[i].0, a[i].1 + b[j].1));
                            i += 1;
                            j += 1;
                        }
                    }
                }
                merged.extend_from_slice(&a[i..]);
                merged.extend_from_slice(&b[j..]);
                Missing(Some(Rc::from(merged)))
            }
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

    /// `missing[name] = score`.
    pub fn with(&self, name: NameId, score: f64) -> Missing {
        let mut entries: Vec<(NameId, f64)> = self.iter().copied().collect();
        match entries.binary_search_by(|(n, _)| n.cmp(&name)) {
            Ok(index) => entries[index].1 = score,
            Err(index) => entries.insert(index, (name, score)),
        }
        Missing(Some(Rc::from(entries)))
    }
}
