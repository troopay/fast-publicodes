# 0014. crates.io distribution

Status: accepted

## Context

The engine crate is pure Rust, so Rust programs can evaluate publicodes rules without JavaScript. Published, its public items become an interface bound by semver, and so do the types of its dependencies that appear in it, such as `serde_json::Value`. A library's exact dependency pins break the builds of programs that need another patch version. A library's build profile does not apply to the programs that use it.

## Decision

- `fast-publicodes` is published to crates.io with the npm package: same version, same `vX.Y.Z` tag, same release workflow, authenticated through crates.io trusted publishing.
- Its public items are documented, which `missing_docs` enforces; it re-exports `serde_json`.
- Its dependencies take caret requirements; `Cargo.lock` still pins what the project builds and tests.
- Its README, which is also its crate documentation, recommends the release profile that gives full speed.

## Consequences

- A breaking change to the Rust API is a major version of the npm package too.
- The crate must be published once by hand before trusted publishing can be configured on crates.io.
- The engine's tests and the README example run with `cargo test`; the compatibility suites, run through the addon, cover the same code.
