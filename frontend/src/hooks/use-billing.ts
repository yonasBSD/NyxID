import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import {
  type BillingUsagePeriod,
  type BillingUsageResponse,
  billingUsageResponseSchema,
} from "@/schemas/billing";

const BILLING_USAGE_KEY = ["billing", "usage"] as const;

export function billingUsagePath(period?: BillingUsagePeriod): string {
  if (!period) {
    return "/billing/usage";
  }
  return `/billing/usage?period=${encodeURIComponent(period)}`;
}

export function useBillingUsage(period?: BillingUsagePeriod) {
  return useQuery({
    queryKey: period ? [...BILLING_USAGE_KEY, period] : BILLING_USAGE_KEY,
    queryFn: async (): Promise<BillingUsageResponse> => {
      const response = await api.get<unknown>(billingUsagePath(period));
      return billingUsageResponseSchema.parse(response);
    },
  });
}
