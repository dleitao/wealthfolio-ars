import { useQuery } from "@tanstack/react-query";
import { getExchangeRates } from "@/adapters";

const STALE_TIME = 30 * 60 * 1000; // 30 minutes

export interface ArsFxRates {
  arsOficial: number; // ARS per 1 USD (official BNA)
  arsMep: number; // ARS per 1 USD (MEP/electronic)
  arsCcl: number; // ARS per 1 USD (CCL/contado con liquidación)
}

export function useArsFxRates() {
  return useQuery({
    queryKey: ["ars-fx-rates"],
    queryFn: async (): Promise<ArsFxRates> => {
      const rates = await getExchangeRates();
      let arsOficial = 0;
      let arsMep = 0;
      let arsCcl = 0;

      // First pass: market-data assets store rates as ARS→USD (rate = USD per ARS),
      // so we invert them to get ARS per USD. Lower priority than PPI rates.
      for (const r of rates) {
        const from = r.fromCurrency?.toUpperCase();
        const to = r.toCurrency?.toUpperCase();
        if (from === "ARS_OFICIAL" && to === "USD" && r.rate > 0) arsOficial = 1 / r.rate;
        if (from === "ARS_MEP" && to === "USD" && r.rate > 0) arsMep = 1 / r.rate;
        if (from === "ARS_CCL" && to === "USD" && r.rate > 0) arsCcl = 1 / r.rate;
        // Seed asset is ARS_CCL; treat it as CCL fallback
        if (from === "ARS_CCL" && to === "USD" && r.rate > 0 && arsCcl === 0) arsCcl = 1 / r.rate;
      }

      // Second pass: PPI stores rates as USD→ARS_MEP (rate = ARS per USD). Overwrite.
      for (const r of rates) {
        const from = r.fromCurrency?.toUpperCase();
        const to = r.toCurrency?.toUpperCase();
        if (from === "USD" && to === "ARS_OFICIAL" && r.rate > 0) arsOficial = r.rate;
        if (from === "USD" && to === "ARS_MEP" && r.rate > 0) arsMep = r.rate;
        if (from === "USD" && to === "ARS_CCL" && r.rate > 0) arsCcl = r.rate;
      }

      return { arsOficial, arsMep, arsCcl };
    },
    staleTime: STALE_TIME,
  });
}
