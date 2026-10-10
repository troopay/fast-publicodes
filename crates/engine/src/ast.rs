//! Parsed rules: an arena of nodes, the rule names and the table of rule definitions.

use std::collections::HashMap;
use std::rc::Rc;

use crate::units::Unit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// An interned dotted name: a rule, or a parse context such as `INLINE_MECANISM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NameId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Number,
    Boolean,
    String,
    Date,
}

impl Type {
    pub fn from_constant(name: &str) -> Option<Self> {
        match name {
            "number" => Some(Type::Number),
            "boolean" => Some(Type::Boolean),
            "string" => Some(Type::String),
            "date" => Some(Type::Date),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub value: Value,
    pub ty: Option<Type>,
    /// `isNullable` when the constant carries it.
    pub nullable: Option<bool>,
    pub unit: Option<Unit>,
}

impl Constant {
    pub fn new(value: Value) -> Self {
        Self {
            value,
            ty: None,
            nullable: None,
            unit: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reference {
    pub name: String,
    /// The rule the reference is written in.
    pub context: NameId,
    pub target: Option<NameId>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: NameId,
    pub private: bool,
    pub valeur: NodeId,
    pub possibilities: Option<NodeId>,
    /// References to the ancestors, nearest first.
    pub parents: Vec<NodeId>,
    pub suggestions: Vec<NodeId>,
    /// `rend non applicable` then `remplace`.
    pub replacements: Vec<Replacement>,
    pub question: bool,
    pub declared_type: Option<Type>,
    /// Whether an undefined value reports the rule itself as missing.
    pub reports_missing: bool,
}

/// A `remplace` or `rend non applicable` entry; its nodes are references.
#[derive(Debug, Clone)]
pub struct Replacement {
    pub id: u32,
    /// The rule that defines the replacement.
    pub definition: NodeId,
    pub replaced: NodeId,
    pub priority: f64,
    /// `dans`: the rules the replacement is limited to.
    pub whitelist: Vec<NodeId>,
    /// `sauf dans`: the rules the replacement skips.
    pub blacklist: Vec<NodeId>,
    pub non_applicable: bool,
}

impl Replacement {
    pub fn references(&self) -> impl Iterator<Item = NodeId> + '_ {
        [self.definition, self.replaced]
            .into_iter()
            .chain(self.whitelist.iter().copied())
            .chain(self.blacklist.iter().copied())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Times,
    Divide,
    IntegerDivide,
    Power,
    Plus,
    Minus,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

impl Operator {
    pub const ALL: [(&'static str, Operator); 14] = [
        ("*", Operator::Times),
        ("/", Operator::Divide),
        ("//", Operator::IntegerDivide),
        ("**", Operator::Power),
        ("+", Operator::Plus),
        ("-", Operator::Minus),
        ("<", Operator::Less),
        ("<=", Operator::LessOrEqual),
        (">", Operator::Greater),
        (">=", Operator::GreaterOrEqual),
        ("=", Operator::Equal),
        ("!=", Operator::NotEqual),
        ("et", Operator::And),
        ("ou", Operator::Or),
    ];

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().find(|(k, _)| *k == key).map(|(_, op)| *op)
    }
}

impl Rule {
    /// A private `$SITUATION` rule, as a sub-engine defines one.
    pub fn situation(name: NameId, valeur: NodeId) -> Self {
        Rule {
            name,
            private: true,
            valeur,
            possibilities: None,
            parents: Vec::new(),
            suggestions: Vec::new(),
            replacements: Vec::new(),
            question: false,
            declared_type: None,
            reports_missing: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PossibilityKind {
    Number,
    String,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrancheMechanism {
    Bareme,
    Grille,
    TauxProgressif,
}

#[derive(Debug, Clone)]
pub struct Tranche {
    pub taux: Option<NodeId>,
    pub montant: Option<NodeId>,
    pub plafond: NodeId,
}

/// A `contexte` entry: while the mechanism evaluates, `origin` takes the value of `replacement`.
#[derive(Debug, Clone)]
pub struct ContextePair {
    pub origin: NodeId,
    pub replacement: NodeId,
    /// Identifies the replacement as written, to compare situations.
    pub replacement_key: u64,
    /// The `$SITUATION` rule giving `origin` a copy of the replacement, and the copy checked
    /// against the possibilities of `origin`, set once references are resolved.
    pub situation: Option<(NodeId, Option<NodeId>)>,
}

/// The rules an `inversion numérique` evaluates its goal with, set once references are resolved.
#[derive(Debug, Clone)]
pub struct InversionRules {
    /// The `$SITUATION` rule of the inverted rule, valued by `input`.
    pub input_rule: NodeId,
    pub input: NodeId,
    /// For each candidate goal, a `$SITUATION` rule leaving it undefined.
    pub goal_rules: Vec<Option<NodeId>>,
}

#[derive(Debug, Clone)]
pub enum TextPart {
    Text(String),
    Node(NodeId),
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub condition: NodeId,
    pub consequence: NodeId,
}

#[derive(Debug, Clone)]
pub enum Node {
    Constant(Constant),
    Reference(Reference),
    Rule(Box<Rule>),
    Operation {
        operator: Operator,
        operands: [NodeId; 2],
    },
    Condition {
        si: NodeId,
        alors: NodeId,
        sinon: NodeId,
    },
    Variations(Vec<Branch>),
    EstNonDefini(NodeId),
    EstNonApplicable(NodeId),
    VariableManquante {
        variable: NameId,
        valeur: NodeId,
    },
    Unite {
        unit: Unit,
        valeur: NodeId,
    },
    SimplifierUnite(NodeId),
    Arrondi {
        valeur: NodeId,
        arrondi: NodeId,
    },
    Duree {
        depuis: NodeId,
        jusqua: NodeId,
        unit: Unit,
    },
    Logarithme(NodeId),
    Texte(Vec<TextPart>),
    Contexte {
        valeur: NodeId,
        pairs: Vec<ContextePair>,
        /// Shared by every copy of the mechanism, to reuse its sub-engine.
        id: u64,
    },
    Inversion {
        rule: NameId,
        candidates: Vec<NodeId>,
        min: f64,
        max: f64,
        tolerance: f64,
        rules: Option<InversionRules>,
    },
    ResoudreReferenceCirculaire {
        rule: NameId,
        valeur: NodeId,
        /// The `$SITUATION` rule of the rule and its `input`, set once references are resolved.
        rules: Option<(NodeId, NodeId)>,
    },
    /// The value a sub-engine gives a `$SITUATION` rule while iterating.
    SituationInput,
    Tranches {
        mechanism: TrancheMechanism,
        assiette: NodeId,
        multiplicateur: NodeId,
        tranches: Vec<Tranche>,
    },
    UnePossibilite {
        kind: PossibilityKind,
        unit: Option<Unit>,
        /// Constants, or references named as written.
        options: Vec<NodeId>,
    },
}

impl Node {
    /// A copy of the node with each child replaced by `map(child)`.
    pub fn map_children<E>(
        &self,
        mut map: impl FnMut(NodeId) -> Result<NodeId, E>,
    ) -> Result<Node, E> {
        Ok(match self {
            Node::Constant(_) | Node::Reference(_) | Node::SituationInput => self.clone(),
            Node::Contexte { valeur, pairs, id } => {
                let mut mapped = Vec::with_capacity(pairs.len());
                for pair in pairs {
                    mapped.push(ContextePair {
                        origin: map(pair.origin)?,
                        replacement: map(pair.replacement)?,
                        ..pair.clone()
                    });
                }
                Node::Contexte {
                    pairs: mapped,
                    valeur: map(*valeur)?,
                    id: *id,
                }
            }
            Node::Inversion {
                rule,
                candidates,
                min,
                max,
                tolerance,
                rules,
            } => Node::Inversion {
                rule: *rule,
                candidates: candidates
                    .iter()
                    .map(|&c| map(c))
                    .collect::<Result<_, _>>()?,
                min: *min,
                max: *max,
                tolerance: *tolerance,
                rules: rules.clone(),
            },
            Node::ResoudreReferenceCirculaire {
                rule,
                valeur,
                rules,
            } => Node::ResoudreReferenceCirculaire {
                rule: *rule,
                valeur: map(*valeur)?,
                rules: *rules,
            },
            Node::Rule(rule) => {
                let suggestions = rule
                    .suggestions
                    .iter()
                    .map(|&s| map(s))
                    .collect::<Result<_, _>>()?;
                let possibilities = rule.possibilities.map(&mut map).transpose()?;
                let parents = rule
                    .parents
                    .iter()
                    .map(|&p| map(p))
                    .collect::<Result<_, _>>()?;
                let valeur = map(rule.valeur)?;
                Node::Rule(Box::new(Rule {
                    valeur,
                    possibilities,
                    parents,
                    suggestions,
                    ..(**rule).clone()
                }))
            }
            Node::Operation { operator, operands } => Node::Operation {
                operator: *operator,
                operands: [map(operands[0])?, map(operands[1])?],
            },
            Node::Condition { si, alors, sinon } => Node::Condition {
                si: map(*si)?,
                alors: map(*alors)?,
                sinon: map(*sinon)?,
            },
            Node::Variations(branches) => Node::Variations(
                branches
                    .iter()
                    .map(|b| {
                        Ok(Branch {
                            condition: map(b.condition)?,
                            consequence: map(b.consequence)?,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            ),
            Node::EstNonDefini(valeur) => Node::EstNonDefini(map(*valeur)?),
            Node::EstNonApplicable(valeur) => Node::EstNonApplicable(map(*valeur)?),
            Node::VariableManquante { variable, valeur } => Node::VariableManquante {
                variable: *variable,
                valeur: map(*valeur)?,
            },
            Node::Unite { unit, valeur } => Node::Unite {
                unit: unit.clone(),
                valeur: map(*valeur)?,
            },
            Node::SimplifierUnite(valeur) => Node::SimplifierUnite(map(*valeur)?),
            Node::Duree {
                depuis,
                jusqua,
                unit,
            } => Node::Duree {
                depuis: map(*depuis)?,
                jusqua: map(*jusqua)?,
                unit: unit.clone(),
            },
            Node::Logarithme(valeur) => Node::Logarithme(map(*valeur)?),
            Node::Texte(parts) => Node::Texte(
                parts
                    .iter()
                    .map(|part| {
                        Ok(match part {
                            TextPart::Text(text) => TextPart::Text(text.clone()),
                            TextPart::Node(node) => TextPart::Node(map(*node)?),
                        })
                    })
                    .collect::<Result<_, _>>()?,
            ),
            Node::Arrondi { valeur, arrondi } => Node::Arrondi {
                valeur: map(*valeur)?,
                arrondi: map(*arrondi)?,
            },
            Node::Tranches {
                mechanism,
                assiette,
                multiplicateur,
                tranches,
            } => Node::Tranches {
                mechanism: *mechanism,
                assiette: map(*assiette)?,
                multiplicateur: map(*multiplicateur)?,
                tranches: tranches
                    .iter()
                    .map(|t| {
                        let plafond = map(t.plafond)?;
                        let montant = t.montant.map(&mut map).transpose()?;
                        let taux = t.taux.map(&mut map).transpose()?;
                        Ok(Tranche {
                            taux,
                            montant,
                            plafond,
                        })
                    })
                    .collect::<Result<_, _>>()?,
            },
            Node::UnePossibilite {
                kind,
                unit,
                options,
            } => Node::UnePossibilite {
                kind: *kind,
                unit: unit.clone(),
                options: options.iter().map(|&o| map(o)).collect::<Result<_, _>>()?,
            },
        })
    }

    pub fn children(&self) -> Vec<NodeId> {
        let mut children = Vec::new();
        let _ = self.map_children(|child| {
            children.push(child);
            Ok::<_, ()>(child)
        });
        children
    }

    /// The `unit` a parsed node carries before evaluation.
    pub fn static_unit(&self) -> Option<&Unit> {
        match self {
            Node::Unite { unit, .. } | Node::Duree { unit, .. } => Some(unit),
            Node::UnePossibilite { unit, .. } => unit.as_ref(),
            Node::Constant(constant) => constant.unit.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InferredType {
    pub nullable: Option<bool>,
    pub ty: Option<Type>,
}

#[derive(Debug, Default, Clone)]
pub struct Names {
    list: Vec<String>,
    ids: HashMap<String, NameId>,
}

impl Names {
    pub fn intern(&mut self, name: &str) -> NameId {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = NameId(self.list.len() as u32);
        self.list.push(name.to_owned());
        self.ids.insert(name.to_owned(), id);
        id
    }

    pub fn get(&self, name: &str) -> Option<NameId> {
        self.ids.get(name).copied()
    }

    pub fn name(&self, id: NameId) -> &str {
        &self.list[id.0 as usize]
    }
}

/// Every node, name and rule definition of an engine.
#[derive(Debug, Default, Clone)]
pub struct Program {
    pub nodes: Vec<Node>,
    /// Inferred types, set for the nodes type inference visited.
    pub types: Vec<Option<InferredType>>,
    pub names: Names,
    /// Rule node of each name in the base rules.
    base_rules: Vec<Option<NodeId>>,
    /// Rules defined by the current situation and evaluations, shadowing the base ones.
    overlay: HashMap<NameId, NodeId>,
    /// Replacements of the base rules, by replaced rule.
    pub replacements: HashMap<NameId, Vec<Replacement>>,
    /// The node each set of applicable replacements turns a reference into, shared by every
    /// reference it applies to.
    pub replacement_nodes: HashMap<Vec<u32>, NodeId>,
    pub next_replacement_id: u32,
    /// Today as `dd/mm/yyyy`, the default bounds of `durée`.
    pub today: String,
}

impl Program {
    pub fn push(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    pub fn rule_node(&self, name: NameId) -> Option<NodeId> {
        self.overlay
            .get(&name)
            .copied()
            .or_else(|| self.base_rules.get(name.0 as usize).copied().flatten())
    }

    pub fn rule(&self, name: NameId) -> Option<&Rule> {
        match self.node(self.rule_node(name)?) {
            Node::Rule(rule) => Some(rule),
            _ => None,
        }
    }

    pub fn define(&mut self, name: NameId, node: NodeId, overlay: bool) {
        if overlay {
            self.overlay.insert(name, node);
        } else {
            let index = name.0 as usize;
            if self.base_rules.len() <= index {
                self.base_rules.resize(index + 1, None);
            }
            self.base_rules[index] = Some(node);
        }
    }

    pub fn ty(&self, id: NodeId) -> Option<InferredType> {
        self.types.get(id.0 as usize).copied().flatten()
    }

    /// Drops every node from `len` on, with the overlay rules.
    pub fn reset_overlay(&mut self, len: usize) {
        self.truncate(len);
        self.overlay.clear();
    }

    pub fn truncate(&mut self, len: usize) {
        self.nodes.truncate(len);
        self.types.truncate(len);
        self.replacement_nodes
            .retain(|_, node| (node.0 as usize) < len);
    }
}

/// `ruleParents`: the ancestors of a dotted name, nearest first.
pub fn rule_parents(name: &str) -> Vec<&str> {
    let mut parents = Vec::new();
    let mut rest = name;
    while let Some(index) = rest.rfind(" . ") {
        rest = &rest[..index];
        parents.push(rest);
    }
    parents
}
