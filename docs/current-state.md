# Estado actual — feature/ars-brokers

_Última actualización: 2026-06-09 (sesión 2)_

## Cambios realizados

### Feature: display currency propagado a toda la app

El selector de moneda (ARS / USD Oficial / USD MEP / USD CCL) de la sidebar ahora
afecta todas las vistas de valores monetarios, no solo el dashboard.

**Patrón aplicado** en cada archivo:
```ts
const { convert, displayCurrencyCode } = useCurrencyConversion();
const c = (v: number) => convert(v, sourceCurrency) ?? v;
// <PrivacyAmount value={c(rawValue)} currency={displayCurrencyCode()} />
```

**Archivos modificados**:

| Archivo | Qué se convierte |
|---------|-----------------|
| `holdings/components/holdings-table.tsx` | Columnas "Total Value" y "Unrealized Gain" en modo base. Portfolios mono-moneda forzados a modo base mediante `effectiveShowConverted = showConvertedValues \|\| !hasMultipleCurrencies` |
| `holdings/components/cash-holdings-widget.tsx` | Total cash balance (los breakdowns por moneda local se mantienen en moneda nativa) |
| `asset/asset-profile-page.tsx` | market value, cost basis, average price, todaysReturn, totalReturn del instrumento |
| `account/account-metrics.tsx` | investmentMarketValue, costBasis, unrealizedPnL, netContribution, cashBalance |
| `account/account-page.tsx` | Total portfolio (`currentValuation?.totalValue`) y period gain (`frontendGainLossAmount`) en el header de la cuenta |
| `net-worth/net-worth-content.tsx` | Net worth total, gain/loss, y todos los ítems del BalanceSheet |
| `income/income-page.tsx` | Total income, monthly average, dividends, interest, top stocks. El pie chart `byCurrency` no se toca (distribución original) |

**Test fix** (`accounts-summary.test.tsx`):
- Agregado `DisplayCurrencyProvider` al wrapper de render
- Mockeado `useCurrencyConversion` con `convert: (v) => v` y `displayCurrencyCode: () => "USD"` para aislar del contexto de tasas

### Fix: comisiones PPI no contabilizadas (API sync + XLSX)

**Root cause**: El mapper de PPI pasaba el `importe` total (qty × precio + comisión) en el campo `amount`, pero dejaba `fee = None`. El motor de snapshots calcula el cash como `depósitos - qty×precio - fees`, ignorando la comisión. Resultado: cash sobredeclarado en ~86k ARS.

**Fix aplicado en dos lugares**:
- `crates/connect/src/platform/ppi/mapper.rs`: para BUY/SELL, calcula `fee = |amount| - qty × price` y lo pone en el campo `fee` del `AccountUniversalActivity`.
- `crates/core/src/activities/ppi_xlsx_importer.rs`: misma lógica para el importer XLSX.

**Resultado post re-sync**: diferencia con la plataforma PPI ahora mínima.

### Feature: importación XLSX PPI en el wizard

**Frontend**:
- `default-activity-template.ts`: `PPI_XLSX_TEMPLATE_ID = "system_ppi_xlsx"`, `isPpiXlsxTemplateId()`, `createPpiXlsxTemplate()`. `prependDefaultActivityTemplate()` incluye ambos templates (Balanz y PPI).
- `upload-step.tsx`: `applyTemplate` hace early-return también para PPI. Prop `xlsxBrokerName: string | null` en `TemplateSelector` muestra el nombre del broker correcto.

**Backend**:
- `xlsx_parser.rs`: auto-detecta PPI por presencia de hoja "Instrumentos".
- `ppi_xlsx_importer.rs`: parsea las 8 hojas del XLSX de PPI.

### Feature: importación XLSX Balanz completa (Tauri + Web)

- `activity-import-page.tsx`: `handleBack` desde "assets" salta el paso "mapping" para imports XLSX.
- `apps/server/src/api/activities.rs`: endpoint `POST /activities/import/parse-xlsx`.
- `adapters/web/activities.ts`: `parseXlsx` vía fetch.

### Feature: cauciones bursátiles en importer Balanz

`balanz_importer.rs`: pre-pass `caucion_principals`, `APCOLCON` → skip, `APCOLFUT` → INTEREST neto.

### Fix: period_return sign-consistent con period_gain (dashboard card)

**Root cause**: en `compute_account_performance` (`crates/core/src/portfolio/performance/performance_service.rs`), `period_gain` y `period_return` medían cosas distintas y podían tener signos opuestos:

- **HOLDINGS mode**: `period_return = end_unrealized_pnl / end_cost_basis` (ratio all-time) vs `period_gain` = delta del período. Cuando el portfolio estaba underwater al inicio pero subió, `period_gain > 0` y `period_return < 0`.
- **TRANSACTIONS mode** (Balanz XLSX con DEPOSITs): `period_return = cumulative_mwr` (MWR compuesto diario). Pérdidas tempranas en bonos ARS producen factores pequeños que los depósitos posteriores no compensan en el producto compuesto, dando MWR ~ -95% con gain nominal positivo.

**Fix**:
- HOLDINGS: `period_gain / end_cost_basis` — siempre sign-consistent (dividir por positivo no cambia signo).
- TRANSACTIONS: `gain_loss_amount / (start_value + net_cash_flow)` — gain relativo al capital total desplegado. Evita explosión por tiny-start-value Y evita distorsión del MWR compuesto.
- Eliminado parámetro `_is_all_time` nunca usado de `compute_holdings_period_return`.
- Test nuevo: `perf_holdings_mode_gain_and_return_same_sign_when_recovering_from_loss`.
- Test renombrado: `perf_holdings_mode_period_return_sign_consistent`.

### Bugs resueltos (histórico)

- **Comisiones PPI invisibles** → `fee = amount - qty×price` en mapper y xlsx importer.
- MEP/dólar-cable phantom SELL → FEE cuando `precio == -1` AND `importe < 0`.
- Cuenta PPI creada como HOLDINGS → `force_tracking_mode: Transactions`.
- 0 actividades PPI → ID determinístico.
- Freeze servidor → `add_rates_to_converter()` incremental.
- Sin `dateFrom` → fallback `2010-01-01`.
- `effective_holdings_mode` auto-fallback sin modificar `tracking_mode`.
- Dashboard card "+$X / -Y%": `period_return` sign-consistent con `period_gain` (sesión 2).

---

## Decisiones tomadas

- **Display currency**: modo Local (multi-moneda) = moneda del instrumento, sin conversión. Modo Base = display currency. Portfolios mono-moneda siempre tratan como modo Base.
- **Pie chart `byCurrency` en Income**: no convertir, muestra distribución original por moneda.
- **`EditableBalance` en account-metrics**: no convertir — es input de edición en moneda nativa.
- **Comisión implícita en `fee`**: no modificar `unit_price` ni `amount` — extraer diferencia a `fee`.
- **PPI XLSX template separado de Balanz**: IDs, funciones y hint text propios.
- **`xlsxBrokerName: string | null`**: más expresivo que booleano.
- **APCOLCON skip / APCOLFUT net interest**: capital de caución no abandona el portfolio.
- **MEP sentinel `precio == -1` AND `importe < 0` → FEE**.
- **`add_rates_to_converter` incremental**: evita O(n) reload.
- **`period_return` = `gain / (start + net_cash_flow)`**: no MWR compuesto — evita distorsión en ARS volátil y mantiene sign-consistency con el monto mostrado en la UI.

---

## Pendientes

- **Tasas ARS históricas para Balanz-only**: `sync_dolar_ars_rates()` solo se llama desde sync PPI. Plan en `/home/daniel/.claude/plans/snug-wandering-wave.md` (Fix 2).
- **Mobile holdings table** (`holdings-table-mobile.tsx`): aún muestra valores en `localCurrency` sin conversión — explícitamente fuera de scope por el usuario.
- **`HoldingsGroupedTable`** y **`NetWorthWidget`**: componentes con valores sin convertir, pero actualmente sin usages en la app.
- **Tests de integración PPI**: ningún test cubre el flujo completo sync → actividades → valuación.
- **2 tests pre-existentes fallando**: `adapter-command-parity.test.ts` (1) y `sell-form.test.tsx` (1) — no relacionados con display currency.

---

## Riesgos

- **Re-sync con "Desde" vacío reimporta todo**: IDs determinísticos evitan duplicados, pero registros huérfanos posibles si PPI cambió datos históricos.
- **Boletos no consecutivos en cauciones Balanz**: fallback emite el importe completo como interés (error conservador, visible en UI).
- **Tasas FX no disponibles al cargar**: `convert()` retorna `undefined` → fallback al valor raw en la moneda original. El usuario ve valores sin convertir hasta que las tasas cargan (comportamiento aceptable, no crashea).
- **`period_return` nuevo vs account-detail page**: la página de detalle de cuenta usa `periodReturn` del mismo campo — con el nuevo cálculo ambas vistas muestran el mismo número, pero ya no es MWR. Si en el futuro se quiere MWR en la página de detalle, habría que separar los campos.

---

## Próxima tarea recomendada

**Verificar que el % de Balanz ahora es razonable** post-rebuild:
- Reconstruir con `pnpm tauri dev`.
- Confirmar que la tarjeta Balanz muestra `period_gain` y `period_return` con el mismo signo.
- Si el % sigue llamando la atención (ej. 18% en un período donde los precios bajaron), evaluar si el denominador `start_value + net_cash_flow` es el correcto para el contexto ARS.

**Tasas ARS históricas para cuentas Balanz-only** (Fix 2 del plan `snug-wandering-wave.md`):

1. `sync_dolar_ars_rates()` ya está en `apps/tauri/src/commands/argentina.rs` como `pub(crate)`.
2. Llamarla después de confirmar un ImportRun de Balanz XLSX — detectar `source_system == "BALANZ"` en el comando de confirm.
3. Verificar: `SELECT count(*) FROM exchange_rates WHERE from_currency='ARS'` antes y después.
