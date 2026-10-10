//! The rules sub-engines evaluate with, created once the references they need are resolved.

use rustc_hash::FxHashSet;

use crate::ast::{Constant, InversionRules, NameId, Node, NodeId, Program, Rule, Type, Value};

fn situation_name(program: &mut Program, rule: NameId) -> NameId {
    let name = format!("{} . $SITUATION", program.names.name(rule));
    program.names.intern(&name)
}

fn target(program: &Program, reference: NodeId) -> Option<NameId> {
    match program.node(reference) {
        Node::Reference(reference) => reference.target,
        _ => None,
    }
}

/// Copies the tree under `id`, sharing references and constants.
fn copy_tree(program: &mut Program, id: NodeId) -> NodeId {
    match program.node(id).clone() {
        Node::Reference(_) | Node::Constant(_) => id,
        node => {
            let copy = node
                .map_children(|child| Ok::<_, std::convert::Infallible>(copy_tree(program, child)))
                .unwrap_or_else(|never| match never {});
            program.push(copy)
        }
    }
}

fn input_rule(program: &mut Program, rule: NameId) -> (NodeId, NodeId) {
    let input = program.push(Node::SituationInput);
    let name = situation_name(program, rule);
    (
        program.push(Node::Rule(Box::new(Rule::situation(name, input)))),
        input,
    )
}

/// Prepares the sub-engine mechanisms of the trees under `roots`; returns the rules created.
pub fn prepare_sub_engines(
    program: &mut Program,
    roots: impl IntoIterator<Item = NodeId>,
) -> Vec<NodeId> {
    let mut created = Vec::new();
    let mut visited = FxHashSet::default();
    let mut stack: Vec<NodeId> = roots.into_iter().collect();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = program.node(id).clone();
        stack.extend(node.children());
        match node {
            Node::Contexte {
                mut pairs,
                valeur,
                id: key,
            } => {
                for pair in pairs.iter_mut().filter(|p| p.situation.is_none()) {
                    let Some(origin) = target(program, pair.origin) else {
                        continue;
                    };
                    let name = situation_name(program, origin);
                    let valeur = copy_tree(program, pair.replacement);
                    let rule = program.push(Node::Rule(Box::new(Rule::situation(name, valeur))));
                    let has_possibilities = program
                        .rule(origin)
                        .is_some_and(|rule| rule.possibilities.is_some());
                    let checked = has_possibilities.then(|| copy_tree(program, pair.replacement));
                    pair.situation = Some((rule, checked));
                    created.push(rule);
                    created.extend(checked);
                }
                *program.node_mut(id) = Node::Contexte {
                    valeur,
                    pairs,
                    id: key,
                };
            }
            Node::Inversion {
                rule,
                candidates,
                min,
                max,
                tolerance,
                rules: None,
            } => {
                let (input_rule, input) = input_rule(program, rule);
                created.push(input_rule);
                let mut goal_rules = Vec::with_capacity(candidates.len());
                for &candidate in &candidates {
                    let goal = target(program, candidate).map(|goal| {
                        let undefined = program.push(Node::Constant(Constant {
                            ty: Some(Type::Number),
                            nullable: Some(false),
                            ..Constant::new(Value::Undefined)
                        }));
                        let name = situation_name(program, goal);
                        program.push(Node::Rule(Box::new(Rule::situation(name, undefined))))
                    });
                    created.extend(goal);
                    goal_rules.push(goal);
                }
                *program.node_mut(id) = Node::Inversion {
                    rule,
                    candidates,
                    min,
                    max,
                    tolerance,
                    rules: Some(InversionRules {
                        input_rule,
                        input,
                        goal_rules,
                    }),
                };
            }
            Node::ResoudreReferenceCirculaire {
                rule,
                valeur,
                rules: None,
            } => {
                let rules = input_rule(program, rule);
                created.push(rules.0);
                *program.node_mut(id) = Node::ResoudreReferenceCirculaire {
                    rule,
                    valeur,
                    rules: Some(rules),
                };
            }
            _ => {}
        }
    }
    created
}
