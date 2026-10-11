//! Raw rules to nodes.

mod inline;
mod possibilities;
mod replacements;
mod sub_engines;

use rustc_hash::FxHashMap;
use std::rc::Rc;

pub use inline::Templates;

use crate::ast::{
    Branch, Constant, Node, NodeId, Operator, Program, Reference, Rule, TextPart, Tranche,
    TrancheMechanism, Type, Value, rule_parents,
};
use crate::error::{Error, ErrorKind, Result};
use crate::expression::{number, parse_expression};
use crate::js::is_js_whitespace;
use crate::js::number_to_string;
use crate::raw::{Entries, Raw};
use crate::units::{parse_unit, parse_unit_with};

/// Mechanisms written alongside a value, in the order they wrap it.
const CHAINABLE: [&str; 14] = [
    "contexte",
    "variable manquante",
    "avec",
    "applicable si",
    "non applicable si",
    "arrondi",
    "unité",
    "simplifier l'unité",
    "plancher",
    "plafond",
    "par défaut",
    "dans la situation",
    "résoudre la référence circulaire",
    "abattement",
];

const OTHER_MECHANISMS: [&str; 24] = [
    "logarithme",
    "inversion numérique",
    "le maximum de",
    "le minimum de",
    "taux progressif",
    "toutes ces conditions",
    "est non défini",
    "est non applicable",
    "est applicable",
    "est défini",
    "une de ces conditions",
    "condition",
    "barème",
    "durée",
    "grille",
    "multiplication",
    "produit",
    "somme",
    "moyenne",
    "texte",
    "valeur",
    "variable",
    "variations",
    "constant",
];

fn is_mechanism_key(key: &str) -> bool {
    Operator::from_key(key).is_some() || CHAINABLE.contains(&key) || OTHER_MECHANISMS.contains(&key)
}

/// Rules defined while parsing one rule set, in definition order.
#[derive(Debug, Default)]
pub struct Batch {
    pub rules: Vec<(crate::ast::NameId, Option<NodeId>)>,
    positions: FxHashMap<crate::ast::NameId, usize>,
    /// The error `publicodes` raises when inferring the types of the rules.
    pub type_error: Option<Error>,
}

impl Batch {
    pub fn get(&self, name: crate::ast::NameId) -> Option<Option<NodeId>> {
        self.positions.get(&name).map(|&i| self.rules[i].1)
    }

    fn reserve(&mut self, name: crate::ast::NameId) {
        if !self.positions.contains_key(&name) {
            self.positions.insert(name, self.rules.len());
            self.rules.push((name, None));
        }
    }

    fn set(&mut self, name: crate::ast::NameId, node: NodeId) {
        let index = self.positions[&name];
        self.rules[index].1 = Some(node);
    }
}

pub struct Parser<'a> {
    pub program: &'a mut Program,
    templates: &'a Templates,
    context: String,
    pub batch: Batch,
    /// Whether to record the source arguments of inline mechanisms.
    pub source_args: bool,
}

impl<'a> Parser<'a> {
    pub fn new(program: &'a mut Program, templates: &'a Templates, context: &str) -> Self {
        Self {
            program,
            templates,
            context: context.to_owned(),
            batch: Batch::default(),
            source_args: false,
        }
    }

    fn push(&mut self, node: Node) -> NodeId {
        self.program.push(node)
    }

    fn syntax(&self, message: impl Into<String>) -> Error {
        Error::syntax(message).in_rule(&self.context)
    }

    /// `parseMecanism`, which reports the JavaScript errors raised while parsing `key` as
    /// syntax errors.
    fn parse_mechanism(&mut self, key: &str, value: &Raw) -> Result<NodeId> {
        self.mechanism(key, value).map_err(|error| {
            if error.is_plain() {
                self.syntax(format!("➡️ Dans le mécanisme {key}\n{}", error.message))
            } else {
                error
            }
        })
    }

    pub fn parse(&mut self, raw: &Raw) -> Result<NodeId> {
        match raw {
            Raw::Undefined | Raw::Null => Err(self.syntax(
                "\n\tUne des valeurs de la formule est vide.\n\tVérifiez que tous les champs à droite des deux points sont remplis",
            )),
            Raw::Bool(_) => Err(self.syntax(
                "\nLes valeurs booléennes true / false ne sont acceptées.\nUtilisez leur contrepartie française : 'oui' / 'non'",
            )),
            Raw::Number(value) => self.parse_chained(&number(*value)),
            Raw::String(text) => {
                let expression = parse_expression(text, &self.context)?;
                self.parse_chained(&expression)
            }
            Raw::Node(id) => Ok(*id),
            Raw::List(_) | Raw::Map(_) => self.parse_chained(raw),
        }
    }

    fn parse_chained(&mut self, raw: &Raw) -> Result<NodeId> {
        if let Raw::Map(entries) = raw
            && let Some(name) = CHAINABLE.iter().find(|name| raw.get(name).is_some())
        {
            let param = raw.get(name).cloned().unwrap_or(Raw::Null);
            let rest: Entries = entries
                .iter()
                .filter(|(k, _)| &**k != *name)
                .cloned()
                .collect();
            let values = Raw::map([("valeur", Raw::Map(Rc::new(rest))), (*name, param)]);
            return self.parse_mechanism(name, &values);
        }
        match raw {
            Raw::List(items) => Err(self.syntax(format!(
                "\nIl manque le nom du mécanisme pour le tableau : [{}]\nLes mécanisme possibles sont : 'somme', 'le maximum de', 'le minimum de', 'toutes ces conditions', 'une de ces conditions'.\n\t\t",
                items
                    .iter()
                    .map(|item| format!("'{}'", item.to_js_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
            Raw::Map(entries) if entries.len() > 1 => Err(self.syntax(format!(
                "\nLes mécanismes suivants se situent au même niveau : {}\nCela vient probablement d'une erreur dans l'indentation\n\t",
                entries
                    .iter()
                    .map(|(k, _)| format!("'{k}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
            Raw::Map(entries) => match entries.first() {
                None => Ok(self.push(Node::Constant(Constant::new(Value::Undefined)))),
                Some((key, value)) => self.parse_mechanism(key, value),
            },
            _ => self.parse(raw),
        }
    }

    fn mechanism(&mut self, key: &str, value: &Raw) -> Result<NodeId> {
        if let Some(operator) = Operator::from_key(key) {
            return self.parse_operation(operator, value);
        }
        match key {
            "variable manquante" => {
                let variable = value
                    .get("variable manquante")
                    .and_then(Raw::as_str)
                    .unwrap_or("");
                let variable = self.program.names.intern(variable);
                let valeur = self.parse_field(value, "valeur")?;
                Ok(self.push(Node::VariableManquante { variable, valeur }))
            }
            "avec" => {
                if let Some(rules) = value.get("avec") {
                    self.parse_rules(rules)?;
                }
                self.parse_field(value, "valeur")
            }
            "unité" => {
                let valeur = self.parse_field(value, "valeur")?;
                let text = match value.get("unité") {
                    Some(Raw::String(text)) => text,
                    other => return Err(not_a_string(other, "string2", "includes")),
                };
                let unit = parse_unit_with(text, self.program.unit_key.as_ref())?;
                Ok(self.push(Node::Unite {
                    unit: Rc::new(unit),
                    valeur,
                }))
            }
            "simplifier l'unité" => {
                let valeur = self.parse_field(value, "valeur")?;
                Ok(self.push(Node::SimplifierUnite(valeur)))
            }
            "est non défini" => {
                let valeur = self.parse(value)?;
                Ok(self.push(Node::EstNonDefini(valeur)))
            }
            "est non applicable" => {
                let valeur = self.parse(value)?;
                Ok(self.push(Node::EstNonApplicable(valeur)))
            }
            "arrondi" => {
                let valeur = self.parse_field(value, "valeur")?;
                let arrondi = self.parse_field(value, "arrondi")?;
                Ok(self.push(Node::Arrondi { valeur, arrondi }))
            }
            "contexte" => self.parse_contexte(value),
            "inversion numérique" => self.parse_inversion(value),
            "résoudre la référence circulaire" => {
                let valeur = self.parse_field(value, "valeur")?;
                let rule = self.program.names.intern(&self.context);
                Ok(self.push(Node::ResoudreReferenceCirculaire {
                    rule,
                    valeur,
                    rules: None,
                }))
            }
            "durée" => self.parse_duree(value),
            "logarithme" => {
                let valeur = self.parse(value)?;
                Ok(self.push(Node::Logarithme(valeur)))
            }
            "texte" => self.parse_texte(value),
            "barème" => self.parse_tranches(TrancheMechanism::Bareme, value),
            "grille" => self.parse_tranches(TrancheMechanism::Grille, value),
            "taux progressif" => self.parse_tranches(TrancheMechanism::TauxProgressif, value),
            "condition" => {
                let si = self.parse_field(value, "si")?;
                let alors = self.parse_field(value, "alors")?;
                let sinon = self.parse_field(value, "sinon")?;
                Ok(self.push(Node::Condition { si, alors, sinon }))
            }
            "variations" => self.parse_variations(value),
            "valeur" => self.parse(value),
            "variable" => self.parse_reference(value),
            "constant" => self.parse_constant(value),
            _ => {
                if let Some(node) = self.parse_inline(key, value)? {
                    return Ok(node);
                }
                Err(self.syntax(format!(
                    "Le mécanisme \"{key}\" est inconnu.\nVérifiez qu'il n'y ait pas d'erreur dans l'orthographe du nom."
                )))
            }
        }
    }

    fn parse_field(&mut self, value: &Raw, key: &str) -> Result<NodeId> {
        match value.get(key) {
            Some(field) => self.parse(field),
            None => self.parse(&Raw::Null),
        }
    }

    /// Parses every operand; the operation reads the first two.
    fn parse_operation(&mut self, operator: Operator, value: &Raw) -> Result<NodeId> {
        let items = match value {
            Raw::List(items) => items,
            Raw::Null => return Err(type_error("Cannot read properties of null (reading 'map')")),
            _ => return Err(type_error("v.map is not a function")),
        };
        let operands = items
            .iter()
            .map(|item| self.parse(item))
            .collect::<Result<Vec<_>>>()?;
        let operands = match operands[..] {
            [left, right, ..] => [left, right],
            _ => {
                self.batch.type_error.get_or_insert_with(|| {
                    type_error("Cannot read properties of undefined (reading 'nodeKind')")
                });
                let undefined = self.push(Node::Constant(Constant::new(Value::Undefined)));
                [undefined, undefined]
            }
        };
        Ok(self.push(Node::Operation { operator, operands }))
    }

    fn parse_duree(&mut self, value: &Raw) -> Result<NodeId> {
        let mut bound = |key: &str| match value.get(key) {
            Some(raw) if !matches!(raw, Raw::Null) => self.parse(raw),
            _ => {
                let today = (self.program.today)()?;
                Ok(self.push(Node::Constant(Constant::default_of(Value::String(
                    today.into(),
                )))))
            }
        };
        let depuis = bound("depuis")?;
        let jusqua = bound("jusqu'à")?;
        let unit = match value.get("unité") {
            Some(raw) if raw.is_truthy() => parse_unit(raw.as_str().unwrap_or(""))?,
            _ => parse_unit("jour")?,
        };
        const UNITS: [&str; 6] = [
            "mois",
            "jour",
            "an",
            "trimestre",
            "trimestre civil",
            "année civile",
        ];
        if !unit.denominators.is_empty()
            || unit.numerators.len() != 1
            || !UNITS.contains(&unit.numerators[0].as_str())
        {
            return Err(self.syntax(format!(
                "Seules les unités suivantes sont acceptées pour une durée : {}.\n\tL'unité fournie est: {}",
                UNITS.join(", "),
                unit.numerators.first().map_or("undefined", String::as_str)
            )));
        }
        Ok(self.push(Node::Duree {
            depuis,
            jusqua,
            unit: Rc::new(unit),
        }))
    }

    /// `texte`: literal text with `{{ expression }}` interpolations, matched as
    /// `/{{(.|\n)*?}}/g`.
    fn parse_texte(&mut self, value: &Raw) -> Result<NodeId> {
        let Raw::String(text) = value else {
            return Err(Error::new(
                ErrorKind::Type,
                match value {
                    Raw::Null => "Cannot read properties of null (reading 'matchAll')",
                    _ => "texte.matchAll is not a function or its return value is not iterable",
                },
            ));
        };
        let mut parts = Vec::new();
        let mut literal = 0;
        let mut search = 0;
        while let Some(found) = text[search..].find("{{") {
            let start = search + found;
            let inner = &text[start + 2..];
            let end = inner
                .find("}}")
                .filter(|&end| !inner[..end].contains(['\r', '\u{2028}', '\u{2029}']));
            let Some(end) = end else {
                search = start + 1;
                continue;
            };
            parts.push(TextPart::Text(text[literal..start].to_owned()));
            let expression = inner[..end].trim_matches(is_js_whitespace);
            parts.push(TextPart::Node(self.parse(&Raw::str(expression))?));
            literal = start + 2 + end + 2;
            search = literal;
        }
        parts.push(TextPart::Text(text[literal..].to_owned()));
        Ok(self.push(Node::Texte(parts)))
    }

    fn parse_tranches(&mut self, mechanism: TrancheMechanism, value: &Raw) -> Result<NodeId> {
        let assiette = self.parse_field(value, "assiette")?;
        let multiplicateur = match value.get("multiplicateur") {
            Some(raw) if raw.is_truthy() => self.parse(raw)?,
            _ => self.push(Node::Constant(Constant::default_of(Value::Number(1.)))),
        };
        let items = match value.get("tranches") {
            Some(Raw::List(items)) => items,
            other => return Err(not_a_list(other, "tranches")),
        };
        let mut tranches = Vec::with_capacity(items.len());
        for item in items.iter() {
            let taux = item.get("taux").map(|raw| self.parse(raw)).transpose()?;
            let montant = item.get("montant").map(|raw| self.parse(raw)).transpose()?;
            let plafond = match item.get("plafond") {
                Some(raw) => self.parse(raw)?,
                None => self.push(Node::Constant(Constant {
                    value: Value::Number(f64::INFINITY),
                    ty: Some(Type::Number),
                    nullable: Some(false),
                    unit: None,
                })),
            };
            tranches.push(Tranche {
                taux,
                montant,
                plafond,
            });
        }
        Ok(self.push(Node::Tranches {
            mechanism,
            assiette,
            multiplicateur,
            tranches,
        }))
    }

    fn parse_variations(&mut self, value: &Raw) -> Result<NodeId> {
        let Raw::List(items) = value else {
            return Err(not_a_list(Some(value), "v"));
        };
        let mut branches = Vec::with_capacity(items.len());
        for item in items.iter() {
            branches.push(match item.get("sinon") {
                Some(sinon) => Branch {
                    consequence: self.parse(sinon)?,
                    condition: self.push(Node::Constant(Constant::default_of(Value::Bool(true)))),
                },
                None => {
                    let consequence = self.parse_field(item, "alors")?;
                    let condition = self.parse_field(item, "si")?;
                    Branch {
                        condition,
                        consequence,
                    }
                }
            });
        }
        Ok(self.push(Node::Variations(branches)))
    }

    fn parse_reference(&mut self, value: &Raw) -> Result<NodeId> {
        if self.context.is_empty() {
            return Err(Error::new(
                ErrorKind::Internal,
                "Une référence ne peut pas exister en dehors d'une règle (`context.dottedName` est vide)",
            ));
        }
        let name = value.as_str().unwrap_or("");
        if name.is_empty() {
            return Err(self.syntax("Une référence ne peut pas être vide"));
        }
        let context = self.program.names.intern(&self.context);
        Ok(self.push(Node::Reference(Reference {
            name: name.to_owned(),
            context,
            target: None,
        })))
    }

    fn parse_constant(&mut self, value: &Raw) -> Result<NodeId> {
        let ty = value
            .get("type")
            .and_then(Raw::as_str)
            .and_then(Type::from_constant);
        let node_value = match value.get("nodeValue") {
            Some(Raw::Number(n)) => Value::Number(*n),
            Some(Raw::Bool(b)) => Value::Bool(*b),
            Some(Raw::String(s)) => Value::String(s.clone()),
            Some(Raw::Null) => Value::Null,
            _ => Value::Undefined,
        };
        let unit = match (ty, value.get("rawUnit").and_then(Raw::as_str)) {
            (Some(Type::Number), Some(raw_unit)) => Some(Rc::new(parse_unit_with(
                raw_unit,
                self.program.unit_key.as_ref(),
            )?)),
            _ => None,
        };
        let nullable = Some(matches!(node_value, Value::Null | Value::Undefined));
        Ok(self.push(Node::Constant(Constant {
            value: node_value,
            ty,
            nullable,
            unit,
        })))
    }

    /// `parseRules`: defines each entry of `rules` in the current context.
    pub fn parse_rules(&mut self, rules: &Raw) -> Result<()> {
        let Raw::Map(entries) = rules else {
            return Ok(());
        };
        for (name, value) in entries.iter() {
            let rule = match value {
                Raw::String(text) => Raw::map([("valeur", Raw::String(text.clone()))]),
                Raw::Number(n) => Raw::map([("valeur", Raw::str(&number_to_string(*n)))]),
                Raw::Map(_) => value.clone(),
                Raw::Null | Raw::List(_) => Raw::Map(Rc::default()),
                Raw::Undefined | Raw::Bool(_) | Raw::Node(_) => {
                    return Err(Error::syntax(format!(
                        "Rule {name} is incorrectly written. Please give it a proper value.\n {}",
                        value.to_js_string()
                    ))
                    .in_rule(&**name));
                }
            };
            self.parse_rule(name, &rule)?;
        }
        Ok(())
    }

    fn parse_rule(&mut self, written_name: &str, raw: &Raw) -> Result<()> {
        let private =
            raw.get("privé").is_some_and(Raw::is_oui) || written_name.starts_with("[privé] ");
        let local_name = written_name
            .strip_prefix("[privé] ")
            .unwrap_or(written_name);
        let dotted_name = match (self.context.is_empty(), local_name.is_empty()) {
            (true, _) => local_name.to_owned(),
            (false, true) => self.context.clone(),
            (false, false) => format!("{} . {local_name}", self.context),
        };
        let name = self.program.names.intern(&dotted_name);
        if matches!(self.batch.get(name), Some(Some(_))) {
            return Err(Error::evaluation(format!(
                "La référence '{dotted_name}' a déjà été définie"
            ))
            .in_rule(&dotted_name));
        }
        self.batch.reserve(name);
        let saved_context = std::mem::replace(&mut self.context, dotted_name.clone());
        let result = self.parse_rule_body(name, &dotted_name, private, raw);
        self.context = saved_context;
        let node = result?;
        self.batch.set(name, node);
        Ok(())
    }

    fn parse_rule_body(
        &mut self,
        name: crate::ast::NameId,
        dotted_name: &str,
        private: bool,
        raw: &Raw,
    ) -> Result<NodeId> {
        let leaf = dotted_name.rsplit(" . ").next().unwrap_or("");
        let title = title(raw.get("titre"), leaf)?;
        let mut avec: Entries = match raw.get("avec") {
            Some(Raw::Map(entries)) => entries.as_ref().clone(),
            _ => Vec::new(),
        };
        let mut raw = raw.clone();
        let possibilities = self.parse_une_possibilite(&mut raw, &mut avec)?;
        let raw = &raw;
        let mut value: Entries = Vec::new();
        if let Raw::Map(entries) = raw {
            for (key, field) in entries.iter() {
                if &**key != "avec" && is_mechanism_key(key) && !value.iter().any(|(k, _)| k == key)
                {
                    value.push((key.clone(), field.clone()));
                }
            }
        }
        if let Some(formule) = raw.get("formule") {
            set_entry(&mut value, "valeur", formule.clone());
        }
        if !private && !dotted_name.ends_with("$SITUATION") {
            set_entry(
                &mut value,
                "dans la situation",
                Raw::str(&format!("{dotted_name} . $SITUATION")),
            );
            let situation = self.push(Node::Constant(Constant {
                value: Value::Undefined,
                ty: None,
                nullable: Some(
                    raw.get("possiblement non applicable")
                        .is_some_and(Raw::is_oui),
                ),
                unit: None,
            }));
            set_entry(
                &mut avec,
                "[privé] $SITUATION",
                Raw::map([("valeur", Raw::Node(situation))]),
            );
            if let Some((_, default)) = value.iter_mut().find(|(k, _)| &**k == "par défaut")
                && !matches!(default, Raw::Null)
            {
                *default = Raw::map([
                    ("valeur", default.clone()),
                    ("variable manquante", Raw::str(dotted_name)),
                ]);
            }
        }
        value.insert(0, ("avec".into(), Raw::Map(Rc::new(avec))));
        let valeur = self.parse(&Raw::Map(Rc::new(value)))?;

        let ancestors = if self.program.options.automatic_namespace_disabling {
            rule_parents(dotted_name)
        } else {
            Vec::new()
        };
        let parents = ancestors
            .into_iter()
            .map(|parent| {
                let target = self.program.names.intern(parent);
                Node::Reference(Reference {
                    name: parent.to_owned(),
                    context: name,
                    target: Some(target),
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|node| self.push(node))
            .collect();
        let mut suggestions = Vec::new();
        if let Some(Raw::Map(entries)) = raw.get("suggestions") {
            for (name, suggestion) in entries.iter() {
                suggestions.push((name.clone(), self.parse(suggestion)?));
            }
        }
        let mut replacements = self.parse_replacements(raw.get("rend non applicable"), true)?;
        replacements.extend(self.parse_replacements(raw.get("remplace"), false)?);
        let declared_type = match raw.get("type").and_then(Raw::as_str) {
            Some("nombre") => Some(Type::Number),
            Some("texte") => Some(Type::String),
            Some("date") => Some(Type::Date),
            Some("booléen") => Some(Type::Boolean),
            _ => None,
        };
        let question = raw.get("question").is_some_and(Raw::is_truthy);
        let is_situation = dotted_name.ends_with("$SITUATION");
        let situation = (!private && !is_situation).then(|| {
            self.program
                .names
                .intern(&format!("{dotted_name} . $SITUATION"))
        });
        Ok(self.push(Node::Rule(Box::new(Rule {
            name,
            private,
            valeur,
            possibilities,
            parents,
            suggestions,
            replacements,
            question,
            declared_type,
            reports_missing: false,
            is_situation,
            names_evaluation: !is_situation && !dotted_name.contains("$INTERNAL"),
            situation,
            title,
            experimental: raw.get("experimental").is_some_and(Raw::is_oui),
            raw: raw.clone(),
        }))))
    }
}

fn type_error(message: &str) -> Error {
    Error::new(ErrorKind::Type, message)
}

/// The `TypeError` JavaScript raises calling `variable.method` on `value`, which is not a string.
fn not_a_string(value: Option<&Raw>, variable: &str, method: &str) -> Error {
    type_error(&match value {
        Some(Raw::Null) => format!("Cannot read properties of null (reading '{method}')"),
        None => format!("Cannot read properties of undefined (reading '{method}')"),
        Some(_) => format!("{variable}.{method} is not a function"),
    })
}

/// The `TypeError` JavaScript raises calling `variable.map` on `value`, which is not a list.
fn not_a_list(value: Option<&Raw>, variable: &str) -> Error {
    not_a_string(value, variable, "map")
}

/// `capitalise0(titre ?? name)`.
fn title(titre: Option<&Raw>, name: &str) -> Result<String> {
    let reading = |of: &str| {
        type_error(&format!(
            "Cannot read properties of {of} (reading 'toUpperCase')"
        ))
    };
    match titre {
        None | Some(Raw::Null) => Ok(capitalise0(name)),
        Some(Raw::String(text)) => Ok(capitalise0(text)),
        Some(Raw::List(items)) => match items.first() {
            Some(Raw::String(first)) => {
                Ok(first.to_uppercase() + &Raw::list(items[1..].to_vec()).to_js_string())
            }
            Some(Raw::Null) => Err(reading("null")),
            None => Err(reading("undefined")),
            Some(_) => Err(type_error("name[0].toUpperCase is not a function")),
        },
        Some(other) if !other.is_truthy() => Ok(other.to_js_string()),
        Some(_) => Err(reading("undefined")),
    }
}

/// `capitalise0`: the text with its first letter in uppercase.
pub fn capitalise0(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn set_entry(entries: &mut Entries, key: &str, value: Raw) {
    match entries.iter_mut().find(|(k, _)| &**k == key) {
        Some((_, existing)) => *existing = value,
        None => entries.push((key.into(), value)),
    }
}
