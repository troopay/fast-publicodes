//! `remplace` and `rend non applicable`.

use super::Parser;
use crate::ast::{NodeId, Replacement};
use crate::error::Result;
use crate::raw::Raw;

impl Parser<'_> {
    pub(super) fn parse_replacements(
        &mut self,
        raw: Option<&Raw>,
        non_applicable: bool,
    ) -> Result<Vec<Replacement>> {
        let items = match raw {
            Some(raw) if !raw.is_truthy() => return Ok(Vec::new()),
            None => return Ok(Vec::new()),
            Some(Raw::List(items)) => items.as_ref().clone(),
            Some(single) => vec![single.clone()],
        };
        items
            .iter()
            .map(|item| {
                let item = match item {
                    Raw::String(name) => Raw::map([("références à", Raw::String(name.clone()))]),
                    other => other.clone(),
                };
                let replaced = self.parse(item.get("références à").unwrap_or(&Raw::Null))?;
                let whitelist = self.parse_names(item.get("dans"))?;
                let blacklist = self.parse_names(item.get("sauf dans"))?;
                let priority = match item.get("priorité") {
                    None | Some(Raw::Null) => 0.,
                    Some(Raw::Number(n)) if *n >= 0. => *n,
                    Some(_) => {
                        return Err(
                            self.syntax("La priorité du remplacement doit être un nombre positif")
                        );
                    }
                };
                let definition = self.parse(&Raw::str(&self.context))?;
                let id = self.program.next_replacement_id;
                self.program.next_replacement_id += 1;
                Ok(Replacement {
                    id,
                    definition,
                    replaced,
                    priority,
                    whitelist,
                    blacklist,
                    non_applicable,
                })
            })
            .collect()
    }

    fn parse_names(&mut self, raw: Option<&Raw>) -> Result<Vec<NodeId>> {
        match raw {
            None => Ok(Vec::new()),
            Some(Raw::List(items)) => items.iter().map(|item| self.parse(item)).collect(),
            Some(single) => Ok(vec![self.parse(single)?]),
        }
    }
}
