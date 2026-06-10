import { useQuery } from "@tanstack/react-query";
import { getInflationData } from "@/adapters";

export function useInflationData() {
  return useQuery({
    queryKey: ["inflation-data"],
    queryFn: getInflationData,
    staleTime: 24 * 60 * 60 * 1000,
    retry: false,
  });
}
