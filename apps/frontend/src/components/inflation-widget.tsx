import { Card, CardContent, CardHeader, CardTitle } from "@wealthfolio/ui/components/ui/card";
import { Skeleton } from "@wealthfolio/ui/components/ui/skeleton";
import { useInflationData } from "@/hooks/use-inflation-data";

const MONTHS_TO_SHOW = 6;

export function InflationWidget() {
  const { data, isLoading, isError } = useInflationData();

  if (isError || (!isLoading && (!data || data.length === 0))) return null;

  const recent = data ? [...data].slice(-MONTHS_TO_SHOW) : [];

  return (
    <Card>
      <CardHeader className="pb-2 pt-4">
        <CardTitle className="text-sm font-medium">Inflación IPC</CardTitle>
      </CardHeader>
      <CardContent className="pb-4">
        {isLoading ? (
          <div className="space-y-2">
            {Array.from({ length: MONTHS_TO_SHOW }).map((_, i) => (
              <Skeleton key={i} className="h-4 w-full" />
            ))}
          </div>
        ) : (
          <div className="space-y-1">
            {recent.map((point) => (
              <div key={point.period} className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">{point.period}</span>
                <span className="font-medium tabular-nums">
                  {point.monthlyRate >= 0 ? "+" : ""}
                  {point.monthlyRate.toFixed(1)}%
                </span>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
