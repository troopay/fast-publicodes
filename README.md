# fast-publicodes

A drop-in replacement for [publicodes](https://publi.codes) 1.x: the same API and behaviour, with the engine written in Rust. Node loads it as a native addon; browsers and other runtimes load it as WebAssembly, from the same package.

## Requirements

- Node 22+
- pnpm 10.33
- Rust via rustup (the pinned toolchain installs itself)
- `wasm-bindgen-cli` 0.2.129, for the WebAssembly module: `cargo install wasm-bindgen-cli --version 0.2.129 --locked`

## Installation

```bash
pnpm install
pnpm build:native
```

## Commands

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
