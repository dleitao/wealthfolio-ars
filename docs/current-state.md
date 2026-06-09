# Estado actual — feature/ars-brokers

_Última actualización: 2026-06-09_

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

### Bugs resueltos (histórico)

- **Comisiones PPI invisibles** → `fee = amount - qty×price` en mapper y xlsx importer.
- MEP/dólar-cable phantom SELL → FEE cuando `precio == -1` AND `importe < 0`.
- Cuenta PPI creada como HOLDINGS → `force_tracking_mode: Transactions`.
- 0 actividades PPI → ID determinístico.
- Freeze servidor → `add_rates_to_converter()` incremental.
- Sin `dateFrom` → fallback `2010-01-01`.
- Rendimiento negativo con ganancia positiva → `end_unrealized_pnl / end_cost_basis`.
- `effective_holdings_mode` auto-fallback sin modificar `tracking_mode`.

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

---

## Próxima tarea recomendada

**Tasas ARS históricas para cuentas Balanz-only** (Fix 2 del plan `snug-wandering-wave.md`):

1. `sync_dolar_ars_rates()` ya está en `apps/tauri/src/commands/argentina.rs` como `pub(crate)`.
2. Llamarla después de confirmar un ImportRun de Balanz XLSX — detectar `source_system == "BALANZ"` en el comando de confirm.
3. Verificar: `SELECT count(*) FROM exchange_rates WHERE from_currency='ARS'` antes y después.
4. Confirmar que el rendimiento de la cuenta Balanz muestra valores razonables en ARS.
