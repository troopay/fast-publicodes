# 0013. Rule types and suggestions

Status: accepted

## Context

Integrations building forms read two parts of the `publicodes` AST. `engine.context.nodesTypes`, a `WeakMap` keyed by node and marked internal, gives each rule's inferred type, `{ type, isNullable }`, which chooses the field of a question; working it out from values fails for questions without an answer. `rule.suggestions` holds each suggestion parsed in the rule's context, which they evaluate alone or inside an expression, such as `{ valeur: suggestion, unité }`, or give as an answer; `publicodes` takes a node anywhere in an expression or a situation. The raw expressions of `rawNode.suggestions`, evaluated from the root, resolve their references differently. The engine infers these types with the same rules and parses suggestions in the rule's context, for its own needs.

## Decision

- `engine.context` exists with `nodesTypes` alone. `getRule()` receives the rule's type from the engine with the rule, and records it in `nodesTypes` under the rule object it returns, which `getParsedRules()` also returns. Copies of an engine share it.
- `rule.suggestions` maps each name to an opaque frozen object. Wherever `evaluate()` or `setSituation()` meets one in a value, it is sent to the engine as a reference to the suggestion, which the engine replaces with the suggestion's node, as `publicodes` places a node in an expression; a suggestion alone is wrapped in `valeur`, as `publicodes` wraps a node.
- Any other AST node in these values is refused with an error, as fast-publicodes has no AST to evaluate.

## Consequences

- Code reading `engine.context.nodesTypes.get(rule)`, or passing suggestions to `evaluate()` and `setSituation()`, works unchanged; any other node, and every other member of `context`, is missing.
- Every object in a value given to `evaluate()` or `setSituation()` is checked for being a node.
- Types and suggestion evaluations are compared with those of `publicodes` on every rule of the compatibility suites.
