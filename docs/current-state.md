# Estado actual — feature/ars-brokers-rebase

_Última actualización: 2026-06-11_

## Cambios realizados

### Rebase a upstream v3.5.2 (2026-06-10)

`feature/ars-brokers-rebase` = main (v3.5.2) + 1 commit squash (`c4eeae5e`).
Los 92 commits originales quedan en `feature/ars-brokers` (v3.3.0, respaldo).
Áreas re-portadas sobre el código nuevo de upstream:

- **Performance**: modelo nuevo `PerformanceResult` (`returns.{twr,irr,value_return}`,
  `mode`, `attribution`, `series`; ya no existen `period_gain/period_return`).
  Fix sign-consistency re-implementado: HOLDINGS `value_return = pnl_change /
  end_cost_basis`; TRANSACTIONS `compute_simple_value_return = gain /
  (start_value + net_cash_flow)`; `effective_holdings_mode` (auto-fallback sin
  depósitos). 8 tests upstream adaptados + regresión Balanz.
- **Broker sync**: arquitectura two-phase de upstream; portado
  `force_tracking_mode` + `ensure_tracking_mode` en el orchestrator.
- **Display currency**: re-aplicado sobre el frontend rediseñado
  (net-worth, accounts-summary batched, holdings-table, dashboard).
- **PPI web**: credenciales por `shared/ppi.ts` con intercepción en `invoke()`.

### Fix headline sign-consistency (2026-06-11)

Bug "+$X / -Y%" en cuentas Balanz TRANSACTIONS con depósitos: el backend ya
calculaba `value_return` sign-consistent pero reporta `mode = TimeWeighted`, y
`performanceHeadlineReturn` (frontend) elegía `returns.twr` — el TWR compuesto
diario diverge en signo con pérdidas tempranas + depósitos grandes.

Fix: `apps/frontend/src/lib/performance.ts` — en modo `timeWeighted` el
headline prefiere `valueReturn`, fallback a `twr` si es null. Cubre dashboard,
accounts-summary (cuenta/grupo) y account-page (comparten la función).
Tests: `performance.test.ts` (7 casos, incl. regresión Balanz).

### Seeding de quotes desde precios de trade en imports (2026-06-11)

Causa raíz de las métricas distorsionadas de Balanz: posiciones valuadas en $0
durante meses (quotes PPI recién desde 2026-06-08; el import XLSX no generaba
quotes, a diferencia del broker sync por API). El TWR componía días ficticios
de -74%/-64%/+115%.

Implementado (espejo de `connect/src/broker/service.rs:514`):
- `Quote::from_trade_price()` en `crates/core/src/quotes/model.rs`
  (id `{asset_id}_{date}_BROKER`, OHLC = unit_price).
- `ActivityService.with_quote_store(...)` + seeding en `import_activities`:
  BUY/SELL con `asset_id` resuelto y `unit_price > 0`, dedup por asset+día,
  error no aborta el import. Wiring en tauri (`providers.rs`) y server
  (`main_lib.rs`). 2 tests + `MockImportQuoteStore` en
  `activities_service_tests.rs`.

### Datos (DB `db/web-dev.db`)

- Backup `db/web-dev.db.bak-2026-06-11`.
- Borrada quote BROKER espuria YPFD `2024-08-16 @ 27.950` (ningún trade
  coincide; congelaba el market value inicial de Balanz en 335.400).

## Decisiones tomadas

- **Squash-merge, no rebase literal**; historia granular en `feature/ars-brokers`.
- **Denominador capital desplegado** (`start + net_cash_flow`) para value_return
  TRANSACTIONS/mixed; zero-start reporta 0% (test upstream adaptado).
- **Headline % = value_return también en TRANSACTIONS, resuelto en frontend**:
  cambiar `mode` en Rust rompería 3 asserts de upstream y el chart del
  performance page. TWR/IRR siguen en performance page, etiquetados como tal.
- **Seeding de quotes en el pipeline compartido de import** (no gateado a
  Balanz): un precio de trade es válido venga de donde venga; mismo criterio
  que el broker sync. Los parsers XLSX siguen puros.
- **Backfill histórico = mecanismo existente** (`SyncMode::BackfillHistory` vía
  `sync_market_data(refetch_all=true)`); PPI y BALANZ_FCI soportan histórico.
  Descartado fallback de valuación a costo (invasivo en core upstream).
- **`is_holdings_mode` reportado = `effective_holdings_mode`**.
- **Credenciales PPI web**: composición client-side sobre secrets en `invoke()`.
- **EditableBalance**: en moneda nativa de la cuenta.

## Pendientes

- **Commit del trabajo de hoy** (working tree sin commitear: fix headline,
  seeding de quotes, tests, docs).
- **Re-import Balanz + backfill PPI** (usuario): borrar cuenta Balanz,
  re-importar XLSX (siembra quotes BROKER), backfill con
  `sync_market_data(refetch_all=true)` por asset (UI: asset page → refresh
  history). Verificar con el script de métricas (replicar TWR/value_return
  sobre `daily_account_valuation`) que desaparecen los días ficticios.
- **Quotes BROKER huérfanas** (assets sin nombre, fechas 2024-2026 en
  web-dev.db): identificar origen (¿holdings payload del broker sync?) y limpiar.
- **Verificación runtime desktop**: `pnpm tauri dev` con DB real — migrations
  upstream sobre DB existente, sync PPI two-phase, selector ARS/MEP/CCL.
- **2 tests frontend fallan por locale es_AR** (`asset-classification-tool-utils`,
  `sell-form`): asumen en-US; correr con `LANG=en_US.UTF-8` o parchear.
- **Tasas ARS históricas para Balanz-only**: `sync_dolar_ars_rates()` solo se
  llama desde sync PPI (plan `snug-wandering-wave.md`, Fix 2).
- **Destino de `feature/ars-brokers`**: borrar/archivar tras validar la rebased.
- **Features v3.5 sin explorar**: spending, lots, rebalance, allocation targets.

## Riesgos

- **Headline residual**: si `value_return` es None (capital desplegado ≤ 0) el
  headline cae a TWR y puede divergir en signo del gain. Raro, sin reporte.
- **Seeding no cubre assets nuevos sin resolver en review**: el `asset_id`
  definitivo se asigna post-import (enrichment) → primera importación de un
  asset desconocido queda sin quote BROKER hasta el primer sync.
- **Precedencia BROKER vs provider en valuación no verificada**: si un mismo
  día tiene quote BROKER (trade) y quote PPI, no se confirmó cuál usa la
  valuación. Revisar si aparecen valores raros post-backfill.
- **Migrations ARS corren antes que las de upstream** (timestamps 05-12..05-17
  vs 05-19+). DB ya migrada con la rama vieja no probada contra migrations
  nuevas (lots, spending, portfolios).
- **Conversión en asset-profile**: `fxEffect` se convierte desde baseCurrency —
  revisar visualmente el card de detalle.

## Próxima tarea recomendada

**Cerrar el ciclo Balanz end-to-end** (web, `pnpm run dev:web`):
1. Commit del trabajo de hoy.
2. Borrar cuenta Balanz → re-importar XLSX → verificar quotes BROKER sembradas
   (`SELECT source, COUNT(*), MIN(day), MAX(day) FROM quotes ... GROUP BY source`).
3. Backfill PPI (`refetch_all=true`) → quotes desde ~2025-05.
4. Recalcular y validar: punto de partida ≈ capital invertido (~1.5M, no 373K),
   sin días de ±60-115%, gain y % del mismo signo en dashboard y account page.
5. Si pasa: repetir verificación en desktop (`pnpm tauri dev`) y commitear.
