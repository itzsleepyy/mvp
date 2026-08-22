# CLI Releases

The `@mvp-play/cli` npm package contains prebuilt MVP binaries for Intel and Apple Silicon macOS, x64 and ARM64 Linux, and x64 Windows. Users do not need Rust installed.

## npm Setup

1. Create the `mvp-play` organization on npm and ensure the package name `@mvp-play/cli` is available.
2. In the npm package settings, add a GitHub Actions trusted publisher for repository `itzsleepyy/mvp` and workflow file `publish-npm.yml`.
3. Protect `main` and require the `Rust`, `Backend`, `Website`, and `Deployment configuration` status checks before merging.

Trusted publishing uses GitHub's short-lived OIDC identity. Do not create or store a long-lived npm token in the repository.

## Publishing

Every push to `main` that changes the Rust CLI, npm wrapper, or release workflow runs `.github/workflows/publish-npm.yml`. It validates and tests the CLI, builds all supported native binaries, and publishes them as one npm package with provenance.

Before merging a release PR, increment the same semantic version in both files:

```text
package/Cargo.toml
npm/package.json
```

The workflow rejects mismatched versions. npm versions are immutable, so a source change without a version increment builds successfully but cannot be published under an existing version.

## Mandatory Updates

The public `GET /v1/client-version` endpoint advertises `minimum_version`, `latest_version`, and the npm update command. The CLI checks it before opening the game. A client older than `minimum_version` displays the required version and update command, then exits. If the API is unavailable, local play remains available.

Roll out an update in this order:

1. Publish and verify the new npm version.
2. Set `LATEST_CLIENT_VERSION` in Coolify to that version and redeploy the backend.
3. When the old build must be blocked, set `MINIMUM_CLIENT_VERSION` to the oldest version still allowed and redeploy.

Never raise `MINIMUM_CLIENT_VERSION` before the replacement package is available from npm.
