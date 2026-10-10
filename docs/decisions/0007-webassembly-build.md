# 0007. WebAssembly build

Status: accepted

## Context

Browsers, and runtimes without a prebuilt addon, cannot load the Node addon. The engine crate is pure Rust and compiles to WebAssembly unchanged. napi-rs can target WebAssembly through WASI with threads, which needs `SharedArrayBuffer` and so cross-origin isolation headers on every site using the package. wasm-bindgen targets the browser directly. The two bindings share the logic that is not about their JavaScript values: JSON in, errors as JSON, warnings sent once then by index, and the functions JavaScript gives the engine. Size matters on the web, where `publicodes` weighs about 40 kB compressed.

## Decision

- A `wasm` crate binds the engine with wasm-bindgen and exposes the same interface as the Node addon; a `js` crate holds what both bindings share.
- The same npm package carries both: conditional exports give Node the addon and every other environment the WebAssembly module. The TypeScript layer is shared; each entry point sets its binding before anything runs.
- The browser entry instantiates the module with a top-level `await`, so that `new Engine()` stays synchronous once the package is imported.
- The WebAssembly build uses `opt-level = "s"`: about 210 kB compressed, with evaluations close to those of `opt-level = 3`, which weighs a quarter more.
- The whole test suite runs against both builds.

## Consequences

- Building the module needs `wasm-bindgen-cli` at the exact version of the `wasm-bindgen` crate.
- Bundlers and runtimes must support top-level `await` to load the browser entry.
- The WebAssembly engine is slower than the addon, and remains several times faster than `publicodes`.
