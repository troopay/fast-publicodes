# Compatibility testing

`test/compat` checks that fast-publicodes behaves like `publicodes`: for a given rule set and situation, every rule is evaluated by both engines, and their value, unit, missing variables in order, warnings and errors (name and message) must match.

`test/engine.test.ts` and `test/utils.test.ts` compare the rest of the API the same way, call by call: errors, warnings, options, rule metadata and helpers.

`pnpm test:wasm` runs every test against the WebAssembly module instead of the Node addon.

`test/types.ts`, checked by `tsc` during `pnpm lint`, asserts that the package's types are interchangeable with those of `publicodes`, AST fields apart.

## Suites

Each `test/compat/suites/<name>.yaml` is a suite: `rules` holds the rules inline, or the module exporting them, such as `modele-social` or a JSON file in a package; `situations` maps names to situations.

```yaml
rules:
  prix: 10 €
  quantité:
    question: Combien ?
    par défaut: 1
  total: prix * quantité
situations:
  vide: {}
  trois:
    quantité: 3
```

## Tests

- `regression.test.ts` runs every situation of every suite, compares the type both engines infer for each rule, and evaluates every rule suggestion in every situation.
- `fuzz.test.ts` generates random situations from each suite's questions, one per seed. When a run fails, it shrinks the situation to the inputs the first differing rule needs and prints it.

| Variable | Effect |
| --- | --- |
| `FUZZ_SEED` | First seed (default `0`) |
| `FUZZ_RUNS` | Number of seeds per suite (default `20`) |
| `FUZZ_SAVE` | Add each new shrunk situation to its suite, turning it into a regression case |

Each failure lists the differing rules; errors raised only by fast-publicodes are counted by message, which ranks the mechanisms still to port.

publicodes results are cached in `node_modules/.cache/compat`, keyed by publicodes version, harness code, date, rules and situation, so only the first run of a situation on a given day pays for publicodes.
