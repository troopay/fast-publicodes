# Releasing

A release publishes `fast-publicodes` at the version in `package.json`, with the addon built for every supported platform.

## Publishing a version

1. Set the new version in `package.json` and commit it.
2. Tag the commit `v<version>`, for example `v0.2.0`, and push the tag.

The `Release` workflow then runs the tests, builds the addon for every platform, tests it, and publishes the package. It refuses a tag that does not match `package.json`.

Running `Release` by hand from the Actions tab goes through every step but publishes nothing: npm only shows what it would publish.
