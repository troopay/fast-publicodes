# 0002. API scope

Status: superseded by 0004

## Context

`publicodes` returns its whole evaluated AST from `evaluate()` and exposes the parsed AST through `getParsedRules()` and `getRule()`; documentation tools such as publicodes-react walk it. Building JS objects for every node on each call would cost more than the Rust evaluation saves, while most integrations only read values.

## Decision

The first compatible release matches `publicodes` on `new Engine(rules)`, `setSituation`, `evaluate`, `getSituation` and `getPossibilitiesFor`. `evaluate` returns `nodeValue`, `unit` and `missingVariables`, without the AST. The AST is exposed later, built lazily when accessed.

## Consequences

- Compatibility is measured on values, units, missing variables and errors.
- Tools that render explanations cannot use fast-publicodes until the AST is exposed.
