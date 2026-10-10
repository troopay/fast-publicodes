# 0003. Observable behaviour as the contract

Status: accepted

## Context

`publicodes` results depend on more than the semantics of each mechanism: missing-variable scores accumulate along the evaluation path, evaluation is cached by node identity and some mechanisms read that cache, inferred node types decide whether a parent disables its children, and most mechanisms are macros expanded into a few primitives at parse time. An engine designed independently would diverge on these details, yet copying the JS implementation as is would keep its costs.

## Decision

The contract is what a developer observes through the API: values, units, missing variables with their scores, which calls throw, and how results depend on earlier calls. It matches `publicodes` exactly. Everything beneath it is free to change for speed, provided the compatibility tests show identical results.

Internals that leak into results are reproduced by their effects, not their mechanics. The engine therefore keeps the `publicodes` model where results depend on it: the same primitive node kinds, macro expansion at parse time, reference resolution and type inference as passes over the parsed rules, and evaluation with cache hits, evaluation order and missing-variable rules that match. It uses Rust structures: nodes in an arena addressed by index, references resolved to rule indices at load time, situation and evaluated expressions parsed into an overlay reset by each `setSituation`, and missing variables as values rather than shared mutable objects.

A deliberate difference in observable behaviour needs its own ADR.

## Consequences

- Each `publicodes` mechanism maps to one Rust module, which keeps porting and review mechanical.
- Optimizations such as resolving rule names directly or building `missingVariables` on read are allowed because they leave results unchanged.
- Behaviour that relies on JS object aliasing is not reproduced; the compatibility tests reveal where that matters.
