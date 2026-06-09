## Project Overview

Wealthfolio — local-first desktop investment tracker. AGPL-3.0. Fork with
Argentine broker/market-data support (PPI, Balanz, DolarApi, ArgentinaDatos).

Two runtime targets from the same codebase:

- **Desktop**: Tauri v2 + Rust backend, SQLite on disk, IPC over Tauri commands
- **Web**: Axum HTTP server, SQLite, REST API

## Stack

| Layer | Technology |
|---|---|
| Frontend | React 18 + Vite + TypeScript, React Router, TanStack Query |
| UI components | shadcn/ui (Radix + Tailwind) |
| Desktop shell | Tauri v2 (Rust) |
| Web server | Axum (Rust) |
| Business logic | Rust workspace crates |
| Storage | SQLite via Diesel ORM + r2d2 pool |
| Numbers | `rust_decimal` (never `f64` for monetary values) |
| Async | Tokio |
| AI | `rig-core`, streaming `AiStreamEvent` |
| Monorepo tooling | pnpm workspaces |

## Main Modules

```
apps/
  frontend/       React SPA — shared by both targets via adapter injection
  tauri/          Tauri shell — commands/, context/, scheduler/, listeners/
  server/         Axum HTTP server — api.rs registers all routes

crates/
  core/           Domain: entities, service traits, business rules (DB-agnostic)
  storage-sqlite/ Diesel repositories — only crate allowed to import Diesel
  market-data/    Provider-agnostic market data (Yahoo, AlphaVantage, PPI, …)
  connect/        Cloud sync: broker ingest, token lifecycle, PPI/Balanz API
  device-sync/    E2EE multi-device sync via Wealthfolio Connect cloud
  ai/             AI chat orchestration (rig-core, tools, streaming)

packages/
  addon-sdk/      Public API for third-party addons
  ui/             Shared React component library
  addon-dev-tools/ Addon scaffolding/dev utilities

addons/           First-party addon plugins (goals, fees, swingfolio)
```

## Design Constraints

- `crates/core` is database-agnostic. It defines traits; `storage-sqlite` implements them. No Diesel in `core`.
- `unsafe_code = "forbid"` workspace-wide.
- Monetary values: always `rust_decimal::Decimal`, never `f64`.
- Frontend adapter pattern: `@/adapters` alias resolves to `adapters/tauri` or `adapters/web` at build time (`BUILD_TARGET` env var). All backend calls go through this interface — never call Tauri IPC or fetch directly in pages.
- Domain events flow from Rust → frontend via Tauri `emit` / SSE (web). Frontend listens and invalidates queries.
- Addons can register dynamic routes; the router subscribes to `subscribeToNavigationUpdates`.

## Conventions

- Rust modules follow the pattern: `*_model.rs`, `*_service.rs`, `*_traits.rs` per domain.
- Repository traits live in `core`; implementations live in `storage-sqlite`.
- Services in `core` receive trait objects (dependency injection), not concrete DB types.
- Frontend pages import from `@/adapters`, not directly from `@/adapters/tauri` or `@/adapters/web`.
- Argentine-specific providers are in `crates/market-data/src/provider/` and `crates/connect/src/platform/`.

## Quick Commands

- Dev desktop: `pnpm tauri dev`
- Dev web: `pnpm run dev:web`
- Tests: `pnpm test` | `cargo test`
- Type check: `pnpm type-check`
- Lint: `pnpm lint`

## Plan Mode

- Make the plan extremely concise. Sacrifice grammar for the sake of concision.
- At the end of each plan, give me a list of unresolved questions to answer, if
  any.

---

## Behavioral Guidelines

**Tradeoff:** These guidelines bias toward caution over speed. For trivial
tasks, use judgment.

### 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:

- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes,
simplify.

### 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:

- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:

```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it
work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer
rewrites due to overcomplication, and clarifying questions come before
implementation rather than after mistakes.
