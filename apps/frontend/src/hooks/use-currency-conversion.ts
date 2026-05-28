import { useCallback } from "react";
import { useDisplayCurrency } from "@/context/display-currency-context";
import { useArsFxRates } from "./use-fx-rates";

/**
 * Convert an amount from its native currency to the current display currency.
 *
 * Conversion table (base = ARS; USD as pivot between ARS variants):
 *   ARS  → ARS:         ×1
 *   ARS  → USD_OFICIAL: ÷ arsOficial
 *   ARS  → USD_MEP:     ÷ arsMep
 *   ARS  → USD_CCL:     ÷ arsCcl
 *   USD  → ARS:         × arsMep  (MEP is the market reference for retail investors)
 *   USD  → USD_OFICIAL: ×1
 *   USD  → USD_MEP:     ×1
 *   USD  → USD_CCL:     ×1
 */
export function useCurrencyConversion() {
  const { displayCurrency } = useDisplayCurrency();
  const { data: rates } = useArsFxRates();

  const convert = useCallback(
    (amount: number, fromCurrency: string): number => {
      if (!rates || !amount || isNaN(amount)) return amount;
      const from = fromCurrency?.toUpperCase();
      const arsOficial = rates.arsOficial || 1;
      const arsMep = rates.arsMep || 1;
      const arsCcl = rates.arsCcl || 1;

      if (from === "ARS" || from === "ARS_OFICIAL" || from === "ARS_MEP") {
        if (displayCurrency === "ARS") return amount;
        if (displayCurrency === "USD_OFICIAL") return amount / arsOficial;
        if (displayCurrency === "USD_MEP") return amount / arsMep;
        if (displayCurrency === "USD_CCL") return amount / arsCcl;
      }

      if (from === "USD") {
        if (displayCurrency === "ARS") return amount * arsMep;
        if (displayCurrency === "USD_OFICIAL") return amount;
        if (displayCurrency === "USD_MEP") return amount;
        if (displayCurrency === "USD_CCL") return amount;
      }

      return amount;
    },
    [displayCurrency, rates],
  );

  const displayCurrencyCode = useCallback((): string => {
    if (displayCurrency === "ARS") return "ARS";
    if (displayCurrency === "USD_OFICIAL") return "USD";
    if (displayCurrency === "USD_MEP") return "USD";
    if (displayCurrency === "USD_CCL") return "USD";
    return "ARS";
  }, [displayCurrency]);

  return { convert, displayCurrency, displayCurrencyCode };
}
