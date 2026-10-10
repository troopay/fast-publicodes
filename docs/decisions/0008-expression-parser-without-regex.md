# 0008. Expression parser without regular expressions

Status: accepted

## Context

`publicodes` tokenizes expressions with JavaScript regular expressions, matched against the rest of the input, unanchored for strings. The `regex` crate made up about a quarter of the WebAssembly module, and its optimized engines another third; without them, parsing rules was slower than `publicodes`. Its semantics also differ from JavaScript's: `.` matches line terminators other than `\n`, and positions are bytes where JavaScript counts UTF-16 units.

## Decision

Each token is a hand-written function reproducing one `publicodes` regular expression, including the order in which a backtracking engine tries alternatives where it decides the match. Positions shown in errors, and the cursor moved past a match found after it, are counted in UTF-16 units. A lone high surrogate, which an error message can contain in JavaScript but not in Rust, is written as the private use character `U+F0000` plus its offset, which the package turns back into the surrogate.

## Consequences

- The engine has no regular expression dependency.
- A differential test parses random expressions, built from the characters the grammar distinguishes, with both parsers.
