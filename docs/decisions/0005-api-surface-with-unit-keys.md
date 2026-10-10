# 0005. API surface, with getUnitKey

Status: superseded by 0006

## Context

`publicodes` exposes the engine (`Engine` and its options, situation, evaluation, rules, possibilities, copies, warnings through a logger), helpers (`utils`, units, `formatValue`, `parseExpression`, errors) and its AST: `evaluate()` and `getRule()` return node trees, and `parsePublicodes`, `reduceAST`, `transformAST`, `traverseASTNode`, `serializeEvaluation` and `simplifyNodeUnit` work on them. Building JS objects for every node on each call would cost more than the Rust evaluation saves, while most integrations only read values and rule metadata.

The `getUnitKey` option maps each base unit written in a unit or a number to the key the engine uses. `publicodes` calls it at every occurrence, while parsing; the Rust parser would have to call into JavaScript as often, though a rule set has only a few dozen distinct base units.

## Decision

Everything outside the AST matches `publicodes`: every `Engine` method and option, warnings and errors with the same text, and the helpers. `evaluate()` returns `nodeValue`, `unit` and `missingVariables`; rule nodes carry `dottedName`, `title`, `private`, `virtualRule`, `rawNode` and `possibilities`, without `explanation`. The AST utilities, evaluating AST nodes and the `utils` functions that take AST nodes are not provided.

`getUnitKey` is called once per distinct base unit and engine, and its result reused. A value it throws that is not an `Error` is reported like an `Error` with that value as message, instead of propagating unchanged.

Helpers that are pure functions of their arguments (`utils`, units, `formatValue`) are written in TypeScript, since crossing the binding would cost more than they do; anything that needs the rules or the parser goes through Rust.

## Consequences

- Compatibility is measured on values, units, missing variables, rule metadata, errors and warnings.
- Tools that render explanations, such as publicodes-react, cannot use fast-publicodes until the AST is exposed; exposing it lazily is a later decision.
- Unit parsing and serialization exist in both Rust and TypeScript, both checked against `publicodes`.
- A `getUnitKey` with side effects or results that vary between calls behaves differently than with `publicodes`.
