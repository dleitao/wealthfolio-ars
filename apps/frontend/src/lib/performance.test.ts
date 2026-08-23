import { describe, expect, it } from "vitest";
import { performanceHeadlineReturn, performancePeriodPnl } from "./performance";
import type { PerformanceResult, PerformanceReturns, ReturnMethod } from "./types";

function makeResult(mode: ReturnMethod, returns: PerformanceReturns): PerformanceResult {
  return {
    scope: { id: "account:balanz", currency: "ARS" },
    period: { startDate: "2026-01-01", endDate: "2026-06-01" },
    mode,
    returns,
    attribution: {
      contributions: 0,
      distributions: 0,
      income: 0,
      realizedPnl: 0,
      unrealizedPnlChange: 0,
      fxEffect: 0,
      fees: 0,
      taxes: 0,
      residual: 0,
    },
    risk: {},
    dataQuality: { status: "ok" },
    series: [],
  };
}

describe("performanceHeadlineReturn", () => {
  it("prefers valueReturn over TWR in timeWeighted mode (Balanz sign-consistency regression)", () => {
    // Early large losses followed by large deposits: TWR compounds to -95%
    // while the simple gain over deployed capital is +5%.
    const result = makeResult("timeWeighted", { twr: -0.95, valueReturn: 0.05 });

    expect(performanceHeadlineReturn(result)).toBe(0.05);
  });

  it("falls back to TWR when valueReturn is unavailable in timeWeighted mode", () => {
    const result = makeResult("timeWeighted", { twr: 0.12, valueReturn: null });

    expect(performanceHeadlineReturn(result)).toBe(0.12);
  });

  it("returns valueReturn in valueReturn mode", () => {
    const result = makeResult("valueReturn", { twr: null, valueReturn: 0.1 });

    expect(performanceHeadlineReturn(result)).toBe(0.1);
  });

  it("returns null in notApplicable mode", () => {
    const result = makeResult("notApplicable", {});

    expect(performanceHeadlineReturn(result)).toBeNull();
  });

  it("returns null when result is missing", () => {
    expect(performanceHeadlineReturn(null)).toBeNull();
    expect(performanceHeadlineReturn(undefined)).toBeNull();
  });
});

describe("performancePeriodPnl", () => {
  it("sums attribution components", () => {
    const result = makeResult("timeWeighted", { twr: 0 });
    result.attribution = {
      contributions: 0,
      distributions: 0,
      income: 10,
      realizedPnl: 20,
      unrealizedPnlChange: 30,
      fxEffect: 5,
      fees: 3,
      taxes: 2,
      residual: 1,
    };

    expect(performancePeriodPnl(result)).toBe(61);
  });

  it("returns null when there is no data", () => {
    const result = makeResult("timeWeighted", { twr: 0 });
    result.dataQuality = { status: "noData" };

    expect(performancePeriodPnl(result)).toBeNull();
  });
});
