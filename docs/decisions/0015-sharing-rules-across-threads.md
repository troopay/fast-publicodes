# 0015. Sharing rules across threads

Status: accepted

## Context

Parsing a large rule set such as modele-social takes about 14 MB; copies of an engine share the parsed rules and cost little. A Rust program evaluating on several threads should parse once and share the rules, but reference counting with `Rc` keeps every copy on the thread that created it. Atomic reference counting (`Arc`) lifts that limit at a cost: the lists of missing variables are merged and copied at every step of an evaluation, and counting them atomically makes setting a situation and evaluating about 12% slower. Counting the parsed rules, values and units atomically costs nothing measurable.

## Decision

- Parsed rules, values and units use `Arc`; the `today` and `getUnitKey` functions must be `Send + Sync`.
- Evaluation state, including missing variables, keeps `Rc`, so an `Engine` stays on its thread.
- `Engine::snapshot` gives the engine's rules and situation without its evaluations, as a `Snapshot` that can be sent to another thread; `Engine::from` starts an engine from it there.
- The JavaScript bindings mark the functions JavaScript gives as thread-safe, which holds because they never move an engine off the JavaScript thread that created it.

## Consequences

- Evaluation is as fast as with `Rc` everywhere.
- Each thread holds its own engine; only the parsed rules are shared.
- A binding that moved engines between threads would have to stop marking JavaScript functions as thread-safe.
