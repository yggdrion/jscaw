# Releasing

Releases are built and published exclusively by [`.github/workflows/CI.yml`](../.github/workflows/CI.yml).
Never run `npm publish` from a local machine.

## One-time setup

1. **Authorize publishing.** The workflow uses npm trusted publishing (OIDC) — no secret
   needed, the `publish` job already requests `id-token: write` and runs on Node 24 (npm
   trusted publishing needs npm ≥11.5.1; Node 22 only bundles npm 10.x). Trusted publishers
   are configured per package, so on npmjs.com, for **each** of the three packages —
   `jscaw`, `jscaw-win32-x64-msvc`, and `jscaw-win32-arm64-msvc` — go to its **Settings →
   Trusted Publisher → Add trusted publisher**, choose GitHub Actions, and fill in:
   - Repository: `yggdrion/jscaw`
   - Workflow filename: `CI.yml`
   - Environment: leave blank
   A package must already exist on npm before you can add a trusted publisher for it, so
   this can only be done after the first release below. Until then, `CI.yml`'s `publish`
   job carries a temporary `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}` job env (see the
   `TODO(bootstrap)` comment there) so that first release can publish without a trusted
   publisher. Once all three packages exist and have a trusted publisher configured,
   remove that env block and the `NPM_TOKEN` secret — this is a one-time setup step, not
   needed again per release.
2. Make sure the repository's Actions settings allow the `publish` job to create GitHub
   releases (default `GITHUB_TOKEN` permissions are sufficient; the workflow requests
   `contents: write`).

## Before cutting a release

Optional but recommended: `pnpm smoke-test` builds `main`, packs it into real tarballs, and
installs+tests them via `bun add` in a scratch consumer project — a deeper check than the dry
runs below, since it actually installs and runs the package rather than just inspecting what
would be packed. See "Pre-Release Smoke Test" in [`AGENTS.md`](../AGENTS.md).

## Cutting a release

From a clean, up-to-date `main`:

```bash
pnpm release patch   # or: minor / major
```

[`scripts/release.mjs`](../scripts/release.mjs) does the whole bump in one shot: pulls
`main` (fast-forward only — it refuses to run on a stale local branch, which is what broke
`v0.1.1`'s first tag), bumps `package.json`, syncs both `npm/*/package.json` platform
packages and `Cargo.toml`/`Cargo.lock` to match, then commits, tags, and pushes. Don't
hand-edit versions in any of those files — `napi pre-publish` copies build artifacts into
the platform packages at publish time, but it does **not** rewrite their committed version,
so they'd drift out of sync with `package.json` otherwise.

Pushing the `v*` tag triggers `CI.yml`: it builds both Windows targets, runs the Node and
Bun test jobs, and only if those pass does the `publish` job run `napi pre-publish` and
`npm publish --provenance`.

## Local dry runs

Before the first real release, verify the packaging without publishing anything:

```bash
pnpm napi pre-publish -t npm --dry-run
npm pack --dry-run --ignore-scripts
```
