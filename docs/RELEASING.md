# Releasing

Releases are built and published exclusively by [`.github/workflows/CI.yml`](../.github/workflows/CI.yml).
Never run `npm publish` from a local machine.

## One-time setup

1. **Create the npm scope.** `@yggdrion` must exist on npmjs.com before the first publish —
   create it at https://www.npmjs.com/org/create if it doesn't exist yet.
2. **Authorize publishing**, either:
   - **npm trusted publishing (recommended)**: on the package's npm settings page, add a
     trusted publisher for this GitHub repository and the `CI.yml` workflow. No secret
     needed — the `publish` job already requests `id-token: write` for OIDC.
   - **Classic token**: create an npm automation token and add it as the `NPM_TOKEN`
     repository secret. The `publish` job reads it via `NODE_AUTH_TOKEN`.
3. Make sure the repository's Actions settings allow the `publish` job to create GitHub
   releases (default `GITHUB_TOKEN` permissions are sufficient; the workflow requests
   `contents: write`).

## Cutting a release

1. Bump `version` in `package.json` (the platform packages under `npm/*/package.json` are
   rewritten automatically by `napi pre-publish` at publish time — don't hand-edit them).
2. Commit the version bump.
3. Tag it and push the tag: `git tag v0.2.0 && git push origin v0.2.0`.
4. Pushing a `v*` tag triggers `CI.yml`: it builds both Windows targets, runs the Node and
   Bun test jobs, and only if those pass does the `publish` job run `napi pre-publish` and
   `npm publish --provenance`.

## Local dry runs

Before the first real release, verify the packaging without publishing anything:

```bash
pnpm napi pre-publish -t npm --dry-run
npm pack --dry-run --ignore-scripts
```
