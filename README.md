# fast-publicodes

A port of [publicodes](https://github.com/publicodes/publicodes) 1.x with its engine rewritten in Rust: the same API and the same behaviour, faster. Node loads it as a native addon, prebuilt for Linux, macOS and Windows; browsers, other runtimes and other platforms load it as WebAssembly, from the same package.

> **Not affiliated with the publicodes project.** fast-publicodes is an independent port. The language, its features and every discussion about them belong to [publicodes](https://github.com/publicodes/publicodes): propose features and report language issues there, not here. If publicodes is useful to you, support its authors: star the repository, contribute, and credit them.

## When to use it

Use fast-publicodes where evaluating publicodes rules is a bottleneck:

- **On a server**: APIs, batch jobs or simulations that evaluate many situations, where each evaluation costs CPU time and latency.
- **In a client**: large rule sets recomputed on every input, or slower devices, where evaluation delays the interface.

Stay with publicodes when its speed is enough for you, or when you need what fast-publicodes does not provide (see [Limitations](#limitations)), such as the explanations publicodes-react renders. In a browser, also weigh the size of the WebAssembly module against the time it saves.

## Usage

```bash
npm install fast-publicodes
```

Then replace the import; nothing else changes.

```ts
import Engine from "fast-publicodes";

const engine = new Engine(rules);
engine.setSituation({ "salaire brut": 3000 }).evaluate("salaire net");
```

## Performance

On the [modele-social](https://www.npmjs.com/package/modele-social) rules (`pnpm bench`, Node, one machine):

| Operation | publicodes | fast-publicodes | Speed-up |
| --- | --- | --- | --- |
| Parse the rules | 366 ms | 68 ms | 5× |
| Set a situation and evaluate one rule | 23.7 ms | 0.75 ms | 32× |
| Set a situation and evaluate every rule | 313 ms | 18 ms | 18× |
| `shallowCopy()` | 6.2 ms | 0.42 ms | 15× |

## Limitations

- **No AST.** `evaluate()` returns `nodeValue`, `unit` and `missingVariables` without `explanation`, and rules returned by `getRule()` and `getParsedRules()` carry their metadata without their node tree; their `suggestions` are opaque values that only `evaluate()` reads. `parsePublicodes`, `reduceAST`, `transformAST`, `traverseASTNode`, `utils.cyclicDependencies`, `utils.disambiguateReferenceNode` and `utils.updateReferencesMapsFromReferenceNode` are not provided, and `evaluate()` accepts no AST node other than these suggestions.
- **Internal fields.** Of the members `publicodes` marks as internal, only `context.nodesTypes` exists, with the type inferred for each rule `getRule()` returns; `baseContext`, `publicParsedRules`, `cache`, `evaluateNode` and the rest of `context` do not.
- **`undefined` inside rules.** A property of a rule set to `undefined` counts as absent, where `publicodes` reads some of them as an empty value.

## Development

### Requirements

- Node 22+
- pnpm 10.33
- Rust via rustup (the pinned toolchain installs itself)
- `wasm-bindgen-cli` 0.2.129, for the WebAssembly module: `cargo install wasm-bindgen-cli --version 0.2.129 --locked`

### Setup

```bash
pnpm install
pnpm build:native
```

### Commands

| Command | Purpose |
| --- | --- |
| `pnpm build:native` | Build the debug addon, needed before `pnpm test` |
| `pnpm build:wasm` | Build the WebAssembly module, needed before `pnpm test:wasm` |
| `pnpm build` | Release addon, WebAssembly module and compiled package |
| `pnpm lint` | Lint and type check Rust and TypeScript |
| `pnpm format` | Format Rust and TypeScript |
| `pnpm test` | TypeScript tests, including the comparison against publicodes |
| `pnpm test:wasm` | The same tests against the WebAssembly module |
| `FUZZ_SEED=100 pnpm test test/compat/fuzz.test.ts` | Fuzz new seeds; see [docs/testing.md](docs/testing.md) |
| `cargo test` | Rust tests |
| `pnpm bench` | Benchmarks against publicodes |

To publish a version, see [docs/releasing.md](docs/releasing.md).

## License

[MIT](LICENSE), including the notice of publicodes, from which this port derives.
