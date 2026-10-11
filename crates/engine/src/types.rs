//! Type inference over parsed rules.

use crate::ast::{InferredType, Node, NodeId, Operator, PossibilityKind, Program, Type};

fn infer(program: &mut Program, id: NodeId) -> InferredType {
    let index = id.0 as usize;
    if program.types.len() < program.len() {
        program.types.resize(program.len(), None);
    }
    if let Some(ty) = program.types[index] {
        return ty;
    }
    program.types[index] = Some(InferredType::default());
    let ty = compute(program, id);
    program.types[index] = Some(ty);
    ty
}

fn compute(program: &mut Program, id: NodeId) -> InferredType {
    let node = program.node(id).clone();
    let boolean = InferredType {
        nullable: Some(false),
        ty: Some(Type::Boolean),
    };
    match node {
        Node::Constant(constant) => InferredType {
            nullable: constant
                .nullable
                .or(Some(constant.value == crate::ast::Value::Null)),
            ty: constant.ty,
        },
        Node::EstNonDefini(_) | Node::EstNonApplicable(_) => boolean,
        Node::Contexte { valeur, .. } => infer(program, valeur),
        Node::Inversion { .. }
        | Node::ResoudreReferenceCirculaire { .. }
        | Node::SituationInput => InferredType {
            nullable: Some(false),
            ty: Some(Type::Number),
        },
        Node::Duree { .. } => InferredType {
            nullable: Some(false),
            ty: Some(Type::Number),
        },
        Node::Texte(_) => InferredType {
            nullable: Some(false),
            ty: Some(Type::String),
        },
        Node::Logarithme(valeur) => InferredType {
            nullable: infer(program, valeur).nullable,
            ty: Some(Type::Number),
        },
        Node::Tranches { .. } => InferredType {
            nullable: Some(false),
            ty: Some(Type::Number),
        },
        Node::Arrondi { valeur, .. } => InferredType {
            nullable: infer(program, valeur).nullable,
            ty: Some(Type::Number),
        },
        Node::Operation { operator, operands } => {
            use Operator::*;
            let nullable = match operator {
                Less | LessOrEqual | Greater | GreaterOrEqual | Divide | Times => {
                    match infer(program, operands[0]).nullable {
                        Some(true) => Some(true),
                        _ => infer(program, operands[1]).nullable,
                    }
                }
                Minus => infer(program, operands[0]).nullable,
                _ => Some(false),
            };
            let ty = match operator {
                Less | LessOrEqual | Greater | GreaterOrEqual | Equal | NotEqual | And | Or => {
                    Type::Boolean
                }
                _ => Type::Number,
            };
            InferredType {
                nullable,
                ty: Some(ty),
            }
        }
        Node::Rule(rule) => {
            let valeur = infer(program, rule.valeur);
            if let Some(possibilities) = rule.possibilities {
                let possibilities = infer(program, possibilities);
                return InferredType {
                    nullable: match valeur.nullable {
                        Some(true) => Some(true),
                        _ => possibilities.nullable,
                    },
                    ty: possibilities.ty,
                };
            }
            if valeur.ty.is_some() {
                return valeur;
            }
            let ty = rule.declared_type.or(if rule.question {
                Some(Type::Boolean)
            } else {
                None
            });
            InferredType {
                nullable: valeur.nullable,
                ty,
            }
        }
        Node::Unite { valeur, .. } | Node::SimplifierUnite(valeur) => InferredType {
            nullable: infer(program, valeur).nullable,
            ty: Some(Type::Number),
        },
        Node::VariableManquante { valeur, .. } => infer(program, valeur),
        Node::Condition { si, alors, sinon } => {
            let nullable = [si, alors, sinon]
                .into_iter()
                .any(|branch| infer(program, branch).nullable == Some(true));
            let ty = match infer(program, alors).ty {
                Some(ty) => Some(ty),
                None => infer(program, sinon).ty,
            };
            InferredType {
                nullable: Some(nullable),
                ty,
            }
        }
        Node::Variations(branches) => {
            let types: Vec<InferredType> = branches
                .iter()
                .map(|b| infer(program, b.consequence))
                .collect();
            InferredType {
                nullable: Some(types.iter().any(|t| t.nullable == Some(true))),
                ty: types.iter().find_map(|t| t.ty),
            }
        }
        Node::Reference(reference) => match reference.target.and_then(|t| program.rule_node(t)) {
            Some(rule) => infer(program, rule),
            None => InferredType::default(),
        },
        Node::UnePossibilite { kind, options, .. } => match kind {
            PossibilityKind::Reference => InferredType {
                nullable: Some(
                    options
                        .iter()
                        .all(|&o| infer(program, o).nullable == Some(true)),
                ),
                ty: Some(Type::String),
            },
            PossibilityKind::Number | PossibilityKind::String => InferredType {
                nullable: Some(false),
                ty: Some(if kind == PossibilityKind::Number {
                    Type::Number
                } else {
                    Type::String
                }),
            },
        },
    }
}

/// Infers the types of the given nodes.
pub fn infer_types(program: &mut Program, nodes: impl IntoIterator<Item = NodeId>) {
    for node in nodes {
        infer(program, node);
    }
}

/// Infers the types of `rules` and of their parent references.
pub fn infer_rule_types(program: &mut Program, rules: impl IntoIterator<Item = NodeId>) {
    for rule in rules {
        infer(program, rule);
        let parents = match program.node(rule) {
            Node::Rule(rule) => rule.parents.clone(),
            _ => Vec::new(),
        };
        for parent in parents {
            infer(program, parent);
        }
    }
}
