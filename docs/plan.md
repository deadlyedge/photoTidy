# photoTidy Development Plan

## Guiding Principles
- Adopt SQLite-backed persistence for scan and planning data to support incremental updates and crash-safe recovery.
- Prioritize observability, incremental processing, and safe file operations for end users.

## Milestone Breakdown
### M1 – Project Foundation
- Scaffold the Tauri 2.x project with React + TypeScript, configure Zustand state store, and adopt Vite tooling.
- Establish cross-platform build pipeline, code formatting, linting, and baseline tests.
- Port configuration defaults from config/config.json; document local environment setup.

### M2 – Platform Services & Config
- Implement a Rust ConfigService to resolve home-relative paths once at startup, persist schema version, and expose the flattened config to the UI.
- Define SQLite schema (media inventory, plan entries, operation logs) and prepare migration tooling.
- Migrate utility helpers: path formatting, JSON IO, timestamps, directory walking, and file filters.
- Add structured logging with tracing, unify error types, and define app-wide event channels.

### M3 – Media Scanning Pipeline
- Build scan_media worker that enumerates media using configured extensions with platform-safe path handling.
- Persist scan results in SQLite, using cached fileHash/fileSize/mtime to skip unchanged files.
- Implement hashing with BLAKE3 + Rayon while retaining MD5 for backwards compatibility.
- Extract EXIF metadata with fallbacks, normalize timestamp format, and persist deterministic ordering.
- Emit progress events for scan -> diff -> hash stages to keep the front-end responsive.

### M4 – Planning & Execution Engine
- Port makeNewPath into plan_targets, replicating naming conventions and duplicate routing semantics with SQLite-backed storage.
- Design operation plan metadata (destination paths, duplicate bucket) with validation and schema versioning.
- Implement execute_plan supporting copy/move, writing per-item transaction logs and rollback hooks in the database.
- Support dry-run preview and undoMoves by replaying the operation log.

### M5 – Frontend Experience
- Rebuild workflow screens: configuration bootstrap, scan progress, plan review, and execution summary.
- Integrate real-time progress via Tauri events, highlight duplicates, and provide a preview of the target folder tree.
- Add safeguards (confirmation dialogs, disk space checks) before executing file operations.

### M6 – Quality & Tooling
- Create unit tests for utilities (hashing, EXIF parsing, timestamp formatting, JSON serialization).
- Build integration tests comparing Python snapshots to Rust outputs on sampleImages, including Unicode and large media cases.
- Add Playwright E2E flow covering import → plan → execute; extend regression suite for known failure modes.
- Document logging/export instructions and a troubleshooting guide for support.

### M7 – Packaging & Release
- Configure Tauri bundling for Windows/macOS/Linux, verifying permissions and EXIF dependencies compile cleanly.
- Automate versioning, changelog generation, and artifact signing where required.
- Conduct release dry-runs, gather beta feedback, and document upgrade/migration steps.

## Cross-Cutting Deliverables
- API documentation for each Tauri command with inputs, outputs, and error semantics.
- SQLite schema reference with migration strategy and sample queries.
- Performance benchmarks for hashing throughput, scan latency, and database operations.

## Risks & Mitigations
- **Hash compatibility**: store both MD5 and BLAKE3 during transition and gate removal behind schema version bump.
- **Large library latency**: incremental diffing and progress streaming reduce perceived wait times.
- **Rollback complexity**: enforce transaction logs and test undo paths under failure scenarios.
- **Database integrity**: enable SQLite WAL mode, schedule backup/export options, and monitor for long-running locks.
- **Path edge cases**: cover mixed separators and Unicode paths in automated suites.

## Definition of Done
- Rust/React implementation functionally matches the Python app across core workflows.
- Automated test suite passes, including diff comparisons against legacy outputs and database-focused checks.
- Packaging scripts produce installable artifacts on all target platforms with documented instructions.
- Observability supports AI-assisted debugging (structured logs and export capability).
