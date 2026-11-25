# Releasing

A release publishes `fast-publicodes` to npm, with the addon built for every supported platform, and its engine to crates.io under the same name and version.

## Publishing a version

1. Set the new version in `package.json` and `crates/engine/Cargo.toml`, and commit them.
2. Tag the commit `v<version>`, for example `v0.2.0`, and push the tag.

The `Release` workflow then runs the tests, builds the addon for every platform, tests it, and publishes the package and the crate. It refuses a tag that does not match both versions.

Running `Release` by hand from the Actions tab goes through every step but publishes nothing: npm and Cargo only show what they would publish.
