# Local Development Setup

Follow these steps to get the Tauri + React workspace running locally.

## Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) toolchain (1.80 or newer) with the `wasm32-unknown-unknown` target.
- Node.js 20+ (or Bun 1.0+) and a package manager (`pnpm` is recommended).
- SQLite is bundled through `rusqlite`, no system packages required.

## Bootstrap Commands
1. Install Node dependencies:
   ```bash
   pnpm install
   ```
2. Verify the toolchain and formats:
   ```bash
   pnpm lint
   pnpm test
   cargo fmt -- --check
   cargo test
   ```
3. Launch the desktop shell:
   ```bash
   pnpm tauri dev
   ```

The front-end automatically fetches the bootstrap configuration via the `bootstrap_paths` command. Database files and JSON artifacts are written under your OS home directory following the defaults defined in `config/config.json`.

## Project Structure Highlights
- `src-tauri/` contains Rust commands, the `ConfigService`, and SQLite migrations.
- `src/` hosts the React UI scaffold with Zustand state for configuration.
- `docs/` captures the migration plan, architecture layout, and this setup guide.

## Common Tasks
- Apply formatting: `pnpm format` (JS/TS) and `cargo fmt` (Rust).
- Static analysis: `pnpm lint` for React/TypeScript.
- Coverage-enabled tests: `pnpm test -- --coverage` (uses `@vitest/coverage-v8`).
- Rebuild database schema: delete the file at `%LOCALAPPDATA%/photoTidy/phototidy.sqlite3` (Windows) or the platform equivalent; it will be recreated on next launch.
