# Repository Guidelines

## Project Structure & Module Organization

This is a Tauri 2 desktop application. The React/TypeScript UI lives in `src/`; reusable views are in `src/components/`, hooks in `src/hooks/`, and browser-test setup in `src/test/`. Native application wiring and Tauri commands live in `src-tauri/src/`. Domain logic is split into Rust workspace crates under `crates/vtb-*` (for example, `vtb-danmaku`, `vtb-recorder`, and `vtb-highlight`). Static assets belong in `public/` or `src-tauri/icons/`; plans and feature documentation belong in `docs/`. Treat `dist/`, `target/`, recordings, and downloaded models as generated or local-only artifacts.

## Build, Test, and Development Commands

- `npm ci`: install the exact frontend dependency versions from `package-lock.json`.
- `npm run dev`: start the Vite frontend server.
- `npm run tauri dev`: run the desktop app with the Rust backend.
- `npm run build`: type-check TypeScript and create the frontend production bundle.
- `npm test` / `npm run test:watch`: run Vitest once or in watch mode.
- `npm run test:rust`: run all Rust workspace tests.
- `pwsh -File scripts/build-windows.ps1`: produce Windows bundles. Equivalent Linux and macOS scripts are in `scripts/`.

Before submitting Rust changes, run `cargo fmt --all -- --check` and, where practical, `cargo clippy --workspace --all-targets -- -D warnings`.

## Coding Style & Naming Conventions

For TypeScript, use two-space indentation, double quotes, and strict settings. React components and their files use `PascalCase`; hooks use `useCamelCase`; variables and functions use `camelCase`. Keep UI components focused and move persistent state into hooks. Follow standard `rustfmt`: modules and functions are `snake_case`, types are `PascalCase`, and constants are `SCREAMING_SNAKE_CASE`. Put shared domain behavior in the appropriate crate rather than Tauri command handlers.

## Testing Guidelines

Place frontend tests beside implementations as `*.test.ts` or `*.test.tsx`; use Vitest and Testing Library with user-visible assertions. Rust unit tests stay near their modules, while integration tests belong in `crates/<crate>/tests/`. Network- or model-dependent tests are ignored by default; run them explicitly, for example `cargo test -p vtb-danmaku --test live_integration -- --ignored`. No coverage threshold is configured, so add regression tests for changed behavior.

## Commit & Pull Request Guidelines

Recent history favors concise, imperative subjects with a category or subsystem, such as `feat: ...`, `docs: ...`, or `recorder: ...`. Keep commits focused. Pull requests should explain the user-facing effect, list verification commands, link relevant issues or roadmap items, and include screenshots for UI changes. Note platform-specific behavior and never commit credentials, `.env` files, recordings, models, or signing material; CI runs Gitleaks before packaging.
