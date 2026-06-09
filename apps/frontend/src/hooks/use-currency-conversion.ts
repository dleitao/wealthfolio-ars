import { useCallback } from "react";
import { useDisplayCurrency } from "@/context/display-currency-context";
import { useArsFxRates } from "./use-fx-rates";

/**
 * Convert an amount from its native currency to the current display currency.
 * Returns undefined when rates are unavailable (loading or fetch failed).
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
  const { data: rates, isLoading, isError } = useArsFxRates();

  const convert = useCallback(
    (amount: number, fromCurrency: string): number | undefined => {
      if (!amount || isNaN(amount)) return amount;
      if (!rates) return undefined;

      const from = fromCurrency?.toUpperCase();
      const { arsOficial, arsMep, arsCcl } = rates;

      if (from === "ARS" || from === "ARS_OFICIAL" || from === "ARS_MEP" || from === "ARS_CCL") {
        if (displayCurrency === "ARS") return amount;
        if (displayCurrency === "USD_OFICIAL") return arsOficial > 0 ? amount / arsOficial : undefined;
        if (displayCurrency === "USD_MEP") return arsMep > 0 ? amount / arsMep : undefined;
        if (displayCurrency === "USD_CCL") return arsCcl > 0 ? amount / arsCcl : undefined;
      }

      if (from === "USD") {
        if (displayCurrency === "ARS") return arsMep > 0 ? amount * arsMep : undefined;
        if (displayCurrency === "USD_OFICIAL") return amount;
        if (displayCurrency === "USD_MEP") return amount;
        if (displayCurrency === "USD_CCL") return amount;
      }

      // Currency not handled by this converter (e.g. EUR, GBP)
      return undefined;
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

  return { convert, displayCurrency, displayCurrencyCode, isLoading, isError };
}
