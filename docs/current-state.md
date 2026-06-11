# Estado actual — feature/ars-brokers-rebase

_Última actualización: 2026-06-10_

## Cambios realizados

### Rebase completo a upstream v3.5.2

La rama `feature/ars-brokers-rebase` ahora es **main (v3.5.2) + 1 commit squash**
(`c4eeae5e`) con todo el trabajo ARS portado. Los 92 commits originales quedaron
preservados en `feature/ars-brokers` (v3.3.0, intacta como respaldo).

Los 30 conflictos se resolvieron en una sola pasada (squash-merge sobre main, no
replay commit-a-commit). Áreas clave:

**Performance service** — upstream reescribió el módulo (1.3k → 6.5k líneas) con
nuevo modelo `PerformanceResult` (`returns.{twr,irr,value_return}`, `attribution`,
`risk`, `data_quality`; ya no existen `period_gain`/`period_return`). El fix de
sign-consistency se re-implementó:
- HOLDINGS: `value_return = pnl_change / end_cost_basis` (en `compute_holdings_value_return`; se eliminó el parámetro `is_all_time` — fórmula unificada)
- TRANSACTIONS: `compute_simple_value_return` = `gain / (start_value + net_cash_flow)` (capital desplegado)
- Mixed-scope: misma fórmula, aplicada también a la serie por día (invariante: último punto de la serie == headline)
- `effective_holdings_mode` (auto-fallback para cuentas sin depósitos) portado usando los helpers flow-basis (`return_net_contribution`, `return_cost_basis`)
- 8 tests de upstream actualizados al nuevo denominador; test de regresión Balanz portado

**Broker sync** — upstream pasó a arquitectura two-phase (`activity_phase.rs`,
`holdings_phase.rs`). Upstream ya había incorporado `override_start_date/end_date`
y `force_full_history` en `SyncConfig`. Se portó: campo `force_tracking_mode`,
método `ensure_tracking_mode` en `BrokerSyncServiceTrait`, y su llamada
post-`sync_accounts` en el orchestrator.

**Display currency** — re-aplicado sobre el frontend rediseñado de upstream:
- `net-worth-content.tsx` (revamp completo): conversión en `parsedData` y en las historias parseadas → MomentumCard/VelocityCard/BreakdownTable heredan valores convertidos
- `accounts-summary.tsx`: nueva API batched `calculatePerformanceSummaries` + `performanceSummaryScopeKey`
- `holdings-table.tsx`: upstream eliminó el toggle `showTotalReturn` (ahora columnas separadas totalPnl/totalReturn/dayPnl); `convert`/`displayCurrencyCode` se pasan como parámetros a `getColumns`
- `dashboard-content.tsx`: usa `performancePeriodPnl`/`performanceHeadlineReturn` de upstream + conversión; chart usa los nuevos campos `totalValueBase`/`netContributionBase`
- `cash-holdings-widget.tsx` eliminado (upstream borró su único consumidor)

**Web adapter / PPI** — upstream agregó tests de paridad de comandos. Las
credenciales PPI ahora van por `shared/ppi.ts` con intercepción en el `invoke()`
web (componen llamadas a secrets); `adapters/web/ppi.ts` eliminado. Los 3 comandos
de credenciales están registrados en `COMMANDS` (paths placeholder).

**Adaptaciones menores**: `supports_dividends: false` en los 4 providers ARS;
`provider_id`/`provider_symbol: None` en los importers Balanz/PPI XLSX (campos
nuevos de `ActivityImport`, fix #855); stubs de métodos ARS
(`save_historical_fx_quotes`, `enrich_xbue_sectors`, `ensure_tracking_mode`) en
mocks de tests de upstream (spending, ai, connect, core).

### Infraestructura

- `origin/main` actualizado a v3.5.2 (fast-forward, 298 commits).
- `pnpm install` requerido tras el rebase (deps nuevas de upstream).

## Decisiones tomadas

- **Squash-merge, no rebase literal**: una pasada de conflictos en vez de 92 replays. Historia granular preservada en `feature/ars-brokers`.
- **Denominador de capital desplegado** (`start + net_cash_flow`) para value_return en TRANSACTIONS/mixed — reemplaza `gain/start_value` de upstream. Sign-consistent, maneja tiny-start-value y zero-start (un test de upstream que esperaba N/A con start=0 ahora espera 0%).
- **La serie mixed-scope usa el mismo denominador** que el headline (invariante preservado). En TRANSACTIONS single-account la serie sigue siendo TWR (mode `TimeWeighted` → el headline del frontend es TWR, no value_return).
- **`is_holdings_mode` reportado = `effective_holdings_mode`** (incluye el fallback).
- **Credenciales PPI web**: composición client-side sobre secrets, interceptada en `invoke()` antes del dispatch HTTP.
- **EditableBalance**: sigue en moneda nativa de la cuenta (input de edición).

## Pendientes

- **Verificación manual en app**: `pnpm tauri dev` — dashboard Balanz/PPI, signo del %, selector de moneda, import XLSX, sync PPI. Nada de esto se probó en runtime aún.
- **2 tests frontend fallan por locale es_AR**: `asset-classification-tool-utils.test.ts` (`33,33%` vs `33.33%`) y `sell-form.test.tsx` (`100.000` vs `100,000`). Tests de upstream que asumen en-US; correr con `LANG=en_US.UTF-8` o parchear.
- **Tasas ARS históricas para Balanz-only**: `sync_dolar_ars_rates()` solo se llama desde sync PPI (plan `snug-wandering-wave.md`, Fix 2).
- **Decidir destino de `feature/ars-brokers`**: borrar o archivar una vez validada la rama rebased.
- **Features v3.5 sin explorar**: spending tracker, lots/disposals, rebalance planner, allocation targets — disponibles pero sin configurar para el caso ARS.

## Riesgos

- **Headline TWR en TRANSACTIONS mode**: el dashboard de cuenta usa `performanceHeadlineReturn` que devuelve TWR (compuesto diario) cuando mode=TimeWeighted — puede divergir en signo del gain en escenarios ARS extremos. El card de accounts-summary usa `SimplePerformanceMetrics` (`gain/net_contribution`, sign-consistent). Si reaparece el bug "+$X / -Y%", revisar qué campo consume esa vista.
- **Migrations ARS corren antes que las de upstream** (timestamps 05-12..05-17 vs 05-19+). Sin colisión detectada, pero una DB ya migrada con la rama vieja no se probó contra las migrations nuevas de upstream (lots, spending, portfolios).
- **Conversión en asset-profile**: `fxEffect` se convierte desde baseCurrency (no localCurrency) — revisar visualmente que el card de detalle muestre valores coherentes.

## Próxima tarea recomendada

**Validar la rama rebased en runtime**:
1. `pnpm tauri dev` con la DB real — confirmar que las migrations de upstream aplican sin error sobre la DB existente.
2. Dashboard: tarjeta Balanz con gain y % del mismo signo; selector ARS/USD MEP/CCL en todas las vistas.
3. Sync PPI end-to-end (two-phase nuevo) — verificar que `force_tracking_mode` sigue forzando TRANSACTIONS.
4. Import XLSX Balanz y PPI por el wizard.
5. Si todo pasa: merge a una rama estable y decidir el destino de `feature/ars-brokers`.
