# Repository Guidelines

## Project Structure & Module Organization

The native addon lives in `src/`: `lib.rs` exposes the Node-API surface and `core_audio.rs` implements Windows Core Audio access. `build.rs` configures napi-rs builds. JavaScript integration tests are in `__test__/addon.test.mjs`, and `examples/basic.ts` demonstrates the public API. Platform package metadata lives under `npm/win32-*-msvc/`. Release documentation and automation are in `docs/RELEASING.md`, `scripts/release.mjs`, and `.github/workflows/CI.yml`.

## Build, Test, and Development Commands

- `pnpm install` installs the pinned JavaScript tooling.
- `pnpm build` creates an optimized native addon for the current Windows platform.
- `pnpm build:debug` builds faster with debug symbols for local development.
- `pnpm test` runs the Node.js test suite against the built addon.
- `bun test` runs the same integration tests under Bun.
- `cargo fmt --check` verifies Rust formatting; use `cargo fmt` to apply it.

Run builds and tests on Windows: this package depends on Windows Core Audio and MSVC targets.

## Coding Style & Naming Conventions

Use Rust 2021 conventions and rustfmt defaults (four-space indentation, `snake_case` functions/modules, `PascalCase` types). Keep unsafe Windows COM calls narrowly scoped and document non-obvious safety assumptions. JavaScript uses ES modules, two-space indentation, single quotes, semicolons, and `camelCase`. Preserve the small public API and reuse shared session-enumeration logic rather than adding parallel paths.

## Testing Guidelines

Tests use the built-in `node:test` runner with `node:assert/strict`. Name files `*.test.mjs` and test observable API behavior, including invalid inputs and unmatched processes. Tests must not mutate real sessions by default. The live check requires `WIN_AUDIO_SESSIONS_TEST_PROCESS` and must restore volume and mute state in `finally`. No coverage threshold is configured; add focused regression tests for changed behavior.

## Commit & Pull Request Guidelines

Follow the repository's Conventional Commit history: `feat:`, `fix:`, `ci:`, and `chore(release):`. Keep commits focused and do not add co-author trailers. Pull requests should explain user-visible behavior, supported Windows targets, and commands run. Link related issues; include logs instead of screenshots unless a visual change genuinely requires one.

## Release Safety

Never run `npm publish` locally. From a clean, current `main`, use `pnpm release patch|minor|major`; CI builds, tests, and publishes tagged releases. Do not hand-edit version fields across package manifests.
