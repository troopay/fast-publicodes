# fast-publicodes

A drop-in replacement for [publicodes](https://publi.codes) 1.x: the same API and behaviour, with the engine written in Rust and loaded in Node as a native addon.

## Requirements

- Node 22+
- pnpm 10.33
- Rust via rustup (the pinned toolchain installs itself)

## Installation

```bash
pnpm install
pnpm build:native
```

## Commands

| Command | Purpose |
| --- | --- |
| `pnpm build:native` | Build the debug addon, needed before `pnpm test` |
| `pnpm build` | Release addon and compiled package |
| `pnpm lint` | Lint and type check Rust and TypeScript |
| `pnpm format` | Format Rust and TypeScript |
| `pnpm test` | TypeScript tests, including the comparison against publicodes |
| `FUZZ_SEED=100 pnpm test test/compat/fuzz.test.ts` | Fuzz new seeds; see [docs/testing.md](docs/testing.md) |
| `cargo test` | Rust tests |
| `pnpm bench` | Benchmarks against publicodes |
