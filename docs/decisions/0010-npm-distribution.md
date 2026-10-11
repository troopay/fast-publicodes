# 0010. npm distribution

Status: superseded by 0011

## Context

The Node addon is a native binary built for each operating system, architecture and C library. Bundling every binary in one package would make each installation download all of them. Linux binaries built on a recent distribution require its recent glibc. Platforms without a prebuilt binary can still run the engine, since the package carries a WebAssembly build. Publishing from CI with a long-lived npm token leaves a secret to protect and rotate.

## Decision

- Each supported platform gets its own npm package holding its binary, named `fast-publicodes-<platform>`, which `fast-publicodes` lists as optional dependencies: npm installs only the one matching the machine. The platforms are Linux x64 and arm64 with glibc and with musl, macOS x64 and arm64, and Windows x64.
- On Node, the package loads the addon when one is installed for the platform and the WebAssembly module otherwise.
- Linux glibc binaries are built against glibc 2.17, and musl binaries are cross-compiled with Zig.
- Pushing a `vX.Y.Z` tag runs the tests, builds every binary, runs the test suite on each platform a hosted runner provides and loads the others under emulation, then publishes every package. npm authenticates the workflow through trusted publishing, without a token.

## Consequences

- A release publishes eight packages at the same version; adding a platform adds a package.
- Each package must be registered once for trusted publishing on npmjs.com, after a first publication with a token.
- Machines without a prebuilt binary run the slower WebAssembly engine without notice.
- ARM Linux binaries are only loaded, not tested, before publication.
