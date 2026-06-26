import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import {
  type BillingWalletResponse,
  type BillingUsagePeriod,
  type BillingUsageResponse,
  type ProvisionBillingWalletRequest,
  type TopUpBillingRequest,
  type TopUpBillingResponse,
  billingWalletResponseSchema,
  billingUsageResponseSchema,
  topUpBillingResponseSchema,
} from "@/schemas/billing";

const BILLING_WALLET_KEY = ["billing", "wallet"] as const;
const BILLING_USAGE_KEY = ["billing", "usage"] as const;

export function billingUsagePath(period?: BillingUsagePeriod): string {
  if (!period) {
    return "/billing/usage";
  }
  return `/billing/usage?period=${encodeURIComponent(period)}`;
}

export function useBillingWallet() {
  return useQuery({
    queryKey: BILLING_WALLET_KEY,
    queryFn: async (): Promise<BillingWalletResponse> => {
      const response = await api.get<unknown>("/billing/wallet");
      return billingWalletResponseSchema.parse(response);
    },
    retry: false,
  });
}

export function useProvisionBillingWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: ProvisionBillingWalletRequest = {},
    ): Promise<BillingWalletResponse> => {
      const response = await api.post<unknown>("/billing/wallet", body);
      return billingWalletResponseSchema.parse(response);
    },
    onSuccess: (wallet) => {
      queryClient.setQueryData(BILLING_WALLET_KEY, wallet);
      void queryClient.invalidateQueries({ queryKey: BILLING_USAGE_KEY });
    },
  });
}

export function useTopUpBilling() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (
      body: TopUpBillingRequest,
    ): Promise<TopUpBillingResponse> => {
      const response = await api.post<unknown>("/billing/topup", body);
      return topUpBillingResponseSchema.parse(response);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BILLING_WALLET_KEY });
      void queryClient.invalidateQueries({ queryKey: BILLING_USAGE_KEY });
    },
  });
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
