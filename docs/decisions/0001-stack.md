# 0001. Stack

Status: accepted

## Context

fast-publicodes is an npm package that exposes the same API and behaviour as `publicodes` 1.x, with the engine reimplemented in Rust for speed. Only Node is targeted for now; browsers may follow. The package has no external services and persists nothing.

## Decision

- **Engine:** Rust, toolchain pinned in `rust-toolchain.toml`, in a Cargo workspace. The engine crate is pure Rust and knows nothing about JavaScript.
- **Binding:** napi-rs, in its own crate, compiled to a native Node addon. Native code avoids the WebAssembly speed penalty on Node.
- **Package:** TypeScript compiled with `tsc`, managed with pnpm. It wraps the addon behind the `publicodes` API and types.
- **Quality:** clippy and rustfmt for Rust; Biome for TypeScript lint and format.
- **Tests:** `cargo test` for the engine; Vitest for the package. Compatibility is proven by differential tests that run the official `publicodes` on the same rules and situations and compare results.

## Consequences

- Parsed rules and evaluation state stay in Rust; the binding crosses the JS↔Rust boundary in bulk, since conversion cost there dominates per-call overhead.
- Releasing means publishing prebuilt addons per platform.
- Browser support needs a second binding crate (wasm-bindgen) over the same engine, without restructuring.
