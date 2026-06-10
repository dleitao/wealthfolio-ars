import { useQuery } from "@tanstack/react-query";
import { getExchangeRates } from "@/adapters";

const STALE_TIME = 30 * 60 * 1000; // 30 minutes

export interface ArsFxRates {
  arsOficial: number; // ARS per 1 USD (official BNA)
  arsMep: number; // ARS per 1 USD (MEP/electronic)
  arsCcl: number; // ARS per 1 USD (CCL/contado con liquidación)
  hasData: boolean; // true when at least one rate is non-zero (rates have been synced)
}

export function useArsFxRates() {
  return useQuery({
    queryKey: ["ars-fx-rates"],
    queryFn: async (): Promise<ArsFxRates> => {
      const rates = await getExchangeRates();

      // Per pair: track the best value seen so far and its timestamp.
      // The rate with the most recent timestamp wins regardless of array order.
      let arsOficial = 0, arsOficialTs = 0;
      let arsMep = 0,     arsMepTs = 0;
      let arsCcl = 0,     arsCclTs = 0;

      const tsOf = (r: { timestamp?: string }) =>
        r.timestamp ? new Date(r.timestamp).getTime() : 0;

      for (const r of rates) {
        const from = r.fromCurrency?.toUpperCase();
        const to   = r.toCurrency?.toUpperCase();
        const t    = tsOf(r);

        // ARS_* → USD direction (market-data): invert to get ARS per USD
        if (from === "ARS_OFICIAL" && to === "USD" && r.rate > 0 && t > arsOficialTs) {
          arsOficial = 1 / r.rate; arsOficialTs = t;
        }
        if (from === "ARS_MEP" && to === "USD" && r.rate > 0 && t > arsMepTs) {
          arsMep = 1 / r.rate; arsMepTs = t;
        }
        if (from === "ARS_CCL" && to === "USD" && r.rate > 0 && t > arsCclTs) {
          arsCcl = 1 / r.rate; arsCclTs = t;
        }

        // USD → ARS_* direction (PPI/DolarAPI): direct value
        if (from === "USD" && to === "ARS_OFICIAL" && r.rate > 0 && t > arsOficialTs) {
          arsOficial = r.rate; arsOficialTs = t;
        }
        if (from === "USD" && to === "ARS_MEP" && r.rate > 0 && t > arsMepTs) {
          arsMep = r.rate; arsMepTs = t;
        }
        if (from === "USD" && to === "ARS_CCL" && r.rate > 0 && t > arsCclTs) {
          arsCcl = r.rate; arsCclTs = t;
        }
      }

      return { arsOficial, arsMep, arsCcl, hasData: arsOficial > 0 || arsMep > 0 || arsCcl > 0 };
    },
    staleTime: STALE_TIME,
  });
}
