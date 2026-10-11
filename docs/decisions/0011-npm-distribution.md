# 0011. npm distribution

Status: accepted

## Context

The Node addon is a native binary built for each operating system, architecture and C library; each weighs about 1.5 MB, 0.65 MB compressed. Publishing one package per platform as optional dependencies saves downloads, but adds a package to publish and register per platform, and installs can miss the binary when a lockfile written on one platform is installed on another. Linux binaries built on a recent distribution require its recent glibc. Platforms without a prebuilt binary can still run the engine, since the package carries a WebAssembly build. Publishing from CI with a long-lived npm token leaves a secret to protect and rotate.

## Decision

- `fast-publicodes` carries the addon for every supported platform: Linux x64 and arm64 with glibc and with musl, macOS x64 and arm64, and Windows x64. The loader picks the one matching the machine.
- On Node, the package loads the addon when one matches the platform and the WebAssembly module otherwise.
- Linux glibc binaries are built against glibc 2.17, and musl binaries are cross-compiled with Zig.
- Pushing a `vX.Y.Z` tag runs the tests, builds every binary, runs the test suite on each platform, then publishes the package. npm authenticates the workflow through trusted publishing, without a token.

## Consequences

- Every installation downloads every binary: about 4.5 MB instead of under 1 MB.
- Adding a platform adds a binary to the package, not a package.
- The package must be registered once for trusted publishing on npmjs.com, after a first publication by hand.
- Machines without a prebuilt binary run the slower WebAssembly engine without notice.
