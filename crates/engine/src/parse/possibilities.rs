//! `une possibilité`: the options a rule accepts.

use super::Parser;
use crate::ast::{Node, NodeId, PossibilityKind, Type, Value};
use crate::error::Result;
use std::rc::Rc;

use crate::raw::{Entries, Raw};
use crate::units::{UnitRef, convert_unit, serialize_unit};

struct Possibility {
    node: NodeId,
    kind: PossibilityKind,
    unit: Option<UnitRef>,
    value: Value,
    publicodes_value: String,
}

impl Parser<'_> {
    /// Parses the possibilities of a rule, adding the rules they define to `avec`.
    pub(super) fn parse_une_possibilite(
        &mut self,
        raw: &mut Raw,
        avec: &mut Entries,
    ) -> Result<Option<NodeId>> {
        let mut possibilities = None;
        for key in ["valeur", "formule"] {
            if let Some(Raw::Map(entries)) = raw.get_mut(key)
                && let Some(index) = entries
                    .iter()
                    .position(|(k, v)| &**k == "une possibilité" && v.is_truthy())
            {
                possibilities = Some(Rc::make_mut(entries).remove(index).1);
            }
        }
        if let Some(value) = raw.get("une possibilité") {
            if !value.is_truthy() {
                return Err(self.syntax("La clé 'une possibilité' ne peut pas être vide"));
            }
            possibilities = Some(value.clone());
        }
        let Some(mut list) = possibilities else {
            return Ok(None);
        };
        if let Some(inner) = list.get("possibilités") {
            list = inner.clone();
        }
        if !list.is_truthy() {
            return Ok(None);
        }
        let items = match list {
            Raw::List(items) if !items.is_empty() => items,
            _ => {
                return Err(self.syntax(
                    "La clé 'une possibilité' doit être un tableau contenant au moins un élément",
                ));
            }
        };

        let mut options: Vec<Possibility> = Vec::with_capacity(items.len());
        // The option others are checked against: the first, then the latest number with a unit.
        let mut reference = 0;
        for item in items.iter() {
            let option = self.parse_option(item, avec)?;
            if let Some(reference_option) = options.get(reference) {
                if option.kind != reference_option.kind {
                    return Err(self.syntax(format!(
                        "Les possibilités doivent être toutes du même type.\n{} et {} ne sont pas du même type.",
                        reference_option.publicodes_value, option.publicodes_value
                    )));
                }
                if option.kind == PossibilityKind::Number && reference_option.unit.is_some() {
                    if convert_unit(option.unit.as_deref(), reference_option.unit.as_deref(), 0.)
                        .is_err()
                    {
                        return Err(self.syntax(format!(
                            "Les possibilités doivent être toutes dans la même classe d'unité.\n{} n'est pas convertible en {}.",
                            serialize_unit(option.unit.as_deref()).unwrap_or_default(),
                            serialize_unit(reference_option.unit.as_deref()).unwrap_or_default()
                        )));
                    }
                    reference = options.len();
                }
            }
            options.push(option);
        }
        let (kind, unit) = (options[reference].kind, options[reference].unit.clone());

        let mut seen: Vec<Value> = Vec::with_capacity(options.len());
        let mut duplicates = Vec::new();
        for option in &options {
            let value = match (kind, &option.value) {
                (PossibilityKind::Number, Value::Number(n)) => {
                    Value::Number(convert_unit(option.unit.as_deref(), unit.as_deref(), *n)?)
                }
                _ => Value::String(option.publicodes_value.as_str().into()),
            };
            let duplicate = seen.iter().any(|v| match (v, &value) {
                (Value::Number(a), Value::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
                (a, b) => a == b,
            });
            if duplicate {
                duplicates.push(option.publicodes_value.clone());
            }
            seen.push(value);
        }
        if !duplicates.is_empty() {
            return Err(self.syntax(format!(
                "Il ne doit pas y avoir de doublons parmi les possibilités.\nLes valeurs suivantes sont en double : {}.",
                duplicates.join(", ")
            )));
        }
        Ok(Some(self.push(Node::UnePossibilite {
            kind,
            unit,
            options: options.iter().map(|o| o.node).collect(),
        })))
    }

    fn parse_option(&mut self, item: &Raw, avec: &mut Entries) -> Result<Possibility> {
        let possibility = match item {
            Raw::Map(entries) => {
                let [(key, value)] = entries.as_slice() else {
                    return Err(self.syntax(
                        "Il ne peut y avoir qu'une seule définition de règle par possibilité.",
                    ));
                };
                if avec.iter().any(|(k, _)| k == key) {
                    return Err(self.syntax(format!("La règle \"{key}\" est déjà définie")));
                }
                avec.push((key.clone(), value.clone()));
                Raw::String(key.clone())
            }
            other => other.clone(),
        };
        let node = self.parse(&possibility)?;
        match self.program.node(node).clone() {
            Node::Reference(reference) => {
                let publicodes_value = format!("'{}'", reference.name);
                let value = Value::String(reference.name.as_str().into());
                Ok(Possibility {
                    node: self.push(Node::Reference(reference)),
                    kind: PossibilityKind::Reference,
                    unit: None,
                    value,
                    publicodes_value,
                })
            }
            Node::Constant(constant) => {
                let kind = match constant.ty {
                    Some(Type::String) => PossibilityKind::String,
                    Some(Type::Number) => PossibilityKind::Number,
                    _ => {
                        return Err(self.syntax(
                            "Les choix possibles doivent être des nombres ou des chaînes de caractères.",
                        ));
                    }
                };
                let publicodes_value = match (&constant.value, &possibility) {
                    (Value::String(s), _) => format!("'{s}'"),
                    (_, Raw::String(text)) => text.to_string(),
                    (_, Raw::Number(n)) => crate::js::number_to_string(*n),
                    _ => String::new(),
                };
                Ok(Possibility {
                    node,
                    kind,
                    unit: constant.unit,
                    value: constant.value,
                    publicodes_value,
                })
            }
            _ => {
                Err(self
                    .syntax("Les choix possibles doivent être des constantes ou des références."))
            }
        }
    }
}
