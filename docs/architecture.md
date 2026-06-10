# Architecture

## Runtime Targets

Wealthfolio runs in two modes from the same Rust + React codebase:

```
Desktop (Tauri)                    Web (Self-hosted)
─────────────────                  ─────────────────
React SPA                          React SPA
    │ Tauri IPC                        │ HTTP/REST
apps/tauri (commands)              apps/server (Axum routes)
    │                                  │
    └──────────┬────────────────────────┘
               │
        crates/core  (business logic)
               │
        crates/storage-sqlite  (SQLite / Diesel)
```

---

## Crate Dependency Graph

```
apps/tauri ──────────────────────────────────────────────────────┐
apps/server ─────────────────────────────────────────────────────┤
                                                                  ↓
crates/ai ────────────────────────────────────→ crates/core
crates/connect ──────────────────────────────→ crates/core
crates/device-sync ──────────────────────────→ (standalone)
crates/market-data ──────────────────────────→ (standalone)
crates/core ─────────────────────────────────→ crates/market-data
crates/storage-sqlite ───────────────────────→ crates/core
                                             → crates/connect
```

Only `storage-sqlite` imports Diesel. All other crates are DB-agnostic.

---

## Module Responsibilities

### `crates/core`

Domain layer. No Diesel, no HTTP.

| Module | Responsibility |
|---|---|
| `accounts` | Account CRUD, tracking modes (manual/sync) |
| `activities` | Trade/transaction records, import pipeline, XLSX/CSV parsers |
| `portfolio` | Holdings, performance, net worth, income, snapshots, allocation, FIRE |
| `quotes` | Quote storage model + `MarketDataClient` facade to `market-data` |
| `assets` | Asset metadata, provider profiles |
| `fx` | Exchange rate storage and lookup |
| `inflation` | Inflation data (CPI/ARS) |
| `goals` | Goal tracking and retirement planning |
| `settings` | App-wide settings |
| `limits` | Contribution limit tracking |
| `taxonomies` | Custom asset classification |
| `secrets` | Encrypted secret storage (ChaCha20-Poly1305) |
| `custom_provider` | User-defined market data providers (JSON/script) |
| `addons` | Addon manifest and function permission model |
| `sync` | Sync-related domain types |
| `events` | Domain event definitions |

### `crates/storage-sqlite`

Diesel ORM repository implementations. One module per `core` domain module.
Exports `DbPool`, `create_pool`, `run_migrations`, `init`.

### `crates/market-data`

Provider-agnostic market data fetching.

```
InstrumentId → ResolverChain → ProviderInstrument → MarketDataProvider → Quote
```

Providers: `yahoo`, `alpha_vantage`, `finnhub`, `boerse_frankfurt`,
`metal_price_api`, `marketdata_app`, `openfigi`, `us_treasury_calc`,
`tradingview`, `ppi`, `balanz_fci`, `dolar_api`, `argentina_datos`.

### `crates/connect`

Cloud integration with Wealthfolio Connect service.

| Module | Responsibility |
|---|---|
| `broker` | Broker sync orchestration (accounts + activities) |
| `broker_ingest` | Import run pipeline, review modes, ingestion state |
| `platform` | Platform-specific broker API clients (PPI, Balanz) |
| `client` | Wealthfolio Connect HTTP client |
| `token_lifecycle` | OAuth token refresh and lifecycle management |

### `crates/device-sync`

E2EE multi-device synchronization via Wealthfolio Connect.

| Module | Responsibility |
|---|---|
| `engine` | Sync engine: upload/download/reconcile cycles |
| `crypto` | X25519 key exchange, ChaCha20 payload encryption |
| `enroll_service` | Device enrollment and pairing flow |
| `client` | Device sync API client |

### `crates/ai`

AI assistant using `rig-core`.

| Module | Responsibility |
|---|---|
| `chat` | Streaming chat loop (model ↔ tools ↔ model) |
| `providers` | Provider catalog, rig client factory |
| `tools` | Tool registry and bounded tool implementations |
| `provider_service` | AI provider settings management |
| `prompt_template` | Versioned system prompt templates |
| `title_generator` | Auto-generates thread titles |

### `apps/tauri`

Tauri shell. Registers Rust functions as IPC commands.

| Module | Responsibility |
|---|---|
| `commands/` | One file per domain — wraps `core` services as `#[tauri::command]` |
| `context/` | `ServiceContext` — creates and holds all service instances |
| `scheduler` | Background job scheduler (quote refresh, sync) |
| `listeners` | Tauri event listeners |
| `domain_events` | Bridges core domain events → Tauri emit |

### `apps/server`

Axum HTTP server. `api.rs` registers all routes.

Modules mirror the domain: `accounts`, `activities`, `assets`, `holdings`,
`portfolio`, `performance`, `market_data`, `ai_chat`, `connect`, `device_sync`,
`goals`, `settings`, `secrets`, `taxonomies`, `limits`, `health`.

### `apps/frontend`

React SPA. Same source for both Tauri and web targets.

| Directory | Responsibility |
|---|---|
| `adapters/tauri` | Calls `invoke()` (Tauri IPC) |
| `adapters/web` | Calls `fetch()` (REST) |
| `pages/` | Route-level page components |
| `features/` | Self-contained feature slices (goals, ai-assistant, connect) |
| `components/` | Shared UI components |
| `hooks/` | TanStack Query data hooks |
| `addons/` | Addon runtime context and dynamic route registration |

---

## Data Flow

### Quote refresh (desktop)

```
scheduler (Tauri) → quotes::service → MarketDataClient
  → market-data ProviderRegistry → provider HTTP call
  → Quote stored in SQLite
  → domain event emitted → Tauri emit → frontend invalidates query
```

### Activity import (broker sync)

```
connect::broker (SyncOrchestrator)
  → platform::PpiApiClient / BalanzClient  (fetch activities)
  → broker_ingest (ImportRun pipeline)
  → review step (user confirms mapping)
  → core::activities::service (bulk upsert)
  → storage-sqlite
```

### Frontend data fetch

```
Page component
  → TanStack Query hook
  → @/adapters (tauri: invoke / web: fetch)
  → Tauri command or Axum route
  → core service → storage-sqlite → SQLite
  → JSON response → UI render
```

---

## Domain Entities

| Entity | Key fields |
|---|---|
| `Account` | id, name, currency, institution, tracking_mode |
| `Activity` | id, account_id, asset_id, activity_type, date, quantity, unit_price, currency |
| `Asset` | id, symbol, name, asset_type, currency, data_source |
| `Quote` | symbol, date, open, high, low, close, volume, currency |
| `Holding` | account_id, asset_id, quantity, book_value, market_value, gain |
| `Goal` | id, title, target_amount, target_date, account allocations |
| `ImportRun` | id, source, status, review_mode, activities (staged/confirmed) |
| `FxRate` | from_currency, to_currency, rate, date |
| `InflationRate` | country, date, rate |
| `ContributionLimit` | year, account_id, limit_amount, contributed |

`ActivityType` enum: `Buy`, `Sell`, `Dividend`, `Interest`, `Transfer`, `Fee`,
`Tax`, `Split`, `Subscription`, `Redemption` (and variants).

---

## Entry Points

| Target | Entry point |
|---|---|
| Desktop (debug) | `pnpm tauri dev` → `apps/tauri/src/main.rs` |
| Web server | `apps/server/src/main.rs` |
| Frontend SPA | `apps/frontend/src/main.tsx` → `App.tsx` → `AppRoutes` |
| Rust tests | `cargo test -p wealthfolio-core` (etc.) |
| Frontend tests | `pnpm test` |

---

## Feature Flags (Rust)

| Flag | Crate | Effect |
|---|---|---|
| `device-sync` | `apps/tauri`, `apps/server` | Enables E2EE device sync routes/commands |
| `connect-sync` | `apps/server` | Enables broker connect sync routes |
| `broker` | `crates/connect` | Enables broker sync types and orchestrator |
