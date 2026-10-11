# 0012. Rule types and suggestions

Status: superseded by 0013

## Context

Integrations building forms read two parts of the `publicodes` AST. `engine.context.nodesTypes`, a `WeakMap` keyed by node and marked internal, gives each rule's inferred type, `{ type, isNullable }`, which chooses the field of a question; working it out from values fails for questions without an answer. `rule.suggestions` holds each suggestion parsed in the rule's context, which they pass to `evaluate()`; the raw expressions of `rawNode.suggestions`, evaluated from the root, resolve their references differently. The engine infers these types with the same rules and parses suggestions in the rule's context, for its own needs.

## Decision

- `engine.context` exists with `nodesTypes` alone. `getRule()` receives the rule's type from the engine with the rule, and records it in `nodesTypes` under the rule object it returns, which `getParsedRules()` also returns. Copies of an engine share it.
- `rule.suggestions` maps each name to an opaque frozen object. `evaluate()` recognises it and has the engine evaluate that suggestion as `publicodes` evaluates a node: wrapped in `$EVALUATION`, with the current replacements, cache and warnings.

## Consequences

- Code reading `engine.context.nodesTypes.get(rule)` or calling `evaluate(rule.suggestions[name])` works unchanged; any other node, and every other member of `context`, is missing.
- Types and suggestion evaluations are compared with those of `publicodes` on every rule of the compatibility suites.
