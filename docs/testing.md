# Compatibility testing

`test/compat` checks that fast-publicodes behaves like `publicodes`: for a given rule set and situation, every rule is evaluated by both engines, and their value, unit and missing variables must match. Errors match when both engines throw.

`test/engine.test.ts` and `test/utils.test.ts` compare the rest of the API the same way, call by call: errors, warnings, options, rule metadata and helpers.

`pnpm test:wasm` runs every test against the WebAssembly module instead of the Node addon.

## Suites

Each `test/compat/suites/<name>.yaml` is a suite: `rules` holds the rules inline, or the name of a package exporting them (`modele-social`); `situations` maps names to situations.

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

- `regression.test.ts` runs every situation of every suite.
- `fuzz.test.ts` generates random situations from each suite's questions, one per seed. When a run fails, it shrinks the situation to the inputs the first differing rule needs and prints it.

| Variable | Effect |
| --- | --- |
| `FUZZ_SEED` | First seed (default `0`) |
| `FUZZ_RUNS` | Number of seeds per suite (default `20`) |
| `FUZZ_SAVE` | Add each new shrunk situation to its suite, turning it into a regression case |

Each failure lists the differing rules; errors raised only by fast-publicodes are counted by message, which ranks the mechanisms still to port.

publicodes results are cached in `node_modules/.cache/compat`, keyed by publicodes version, harness code, rules and situation, so only the first run of a situation pays for publicodes.
