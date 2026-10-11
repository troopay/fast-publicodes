# 0006. API surface

Status: superseded by 0009

## Context

`publicodes` exposes the engine (`Engine` and its options, situation, evaluation, rules, possibilities, copies, warnings through a logger), helpers (`utils`, units, `formatValue`, `parseExpression`, errors) and its AST: `evaluate()` and `getRule()` return node trees, and `parsePublicodes`, `reduceAST`, `transformAST`, `traverseASTNode`, `serializeEvaluation` and `simplifyNodeUnit` work on them. Its nodes are the objects its parser and evaluator build, spreads and in-place mutations included, so matching them means reproducing that JavaScript object handling mechanism by mechanism, while most integrations only read values and rule metadata.

## Decision

Everything outside the AST matches `publicodes`: every `Engine` method and option, warnings and errors with the same text, and the helpers. `evaluate()` returns `nodeValue`, `unit` and `missingVariables`; rule nodes carry `dottedName`, `title`, `private`, `virtualRule`, `rawNode` and `possibilities`, without `explanation`. The AST utilities, evaluating AST nodes and the `utils` functions that take AST nodes are not provided.

The functions an engine is given or relies on, `getUnitKey` and the current date, are called from JavaScript when `publicodes` calls them: at every unit and every `durée` it parses.

Helpers that are pure functions of their arguments (`utils`, units, `formatValue`) are written in TypeScript, since crossing the binding would cost more than they do; anything that needs the rules or the parser goes through Rust.

## Consequences

- Compatibility is measured on values, units, missing variables, rule metadata, errors and warnings.
- Tools that render explanations, such as publicodes-react, cannot use fast-publicodes.
- Unit parsing and serialization exist in both Rust and TypeScript, both checked against `publicodes`.
- An engine given `getUnitKey` calls into JavaScript for each unit it parses.
