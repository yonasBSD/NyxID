import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { billingUsagePath, useBillingUsage } from "./use-billing";

const { mockGet } = vi.hoisted(() => ({
  mockGet: vi.fn(),
}));

vi.mock("@/lib/api-client", () => ({
  api: { get: mockGet },
}));

function wrapperFactory() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
  return ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("billingUsagePath", () => {
  it("targets only the read-only billing usage endpoint", () => {
    expect(billingUsagePath()).toBe("/billing/usage");
    expect(billingUsagePath("7d")).toBe("/billing/usage?period=7d");
  });
});

describe("useBillingUsage", () => {
  it("fetches and validates read-only billing usage", async () => {
    mockGet.mockResolvedValue({
      owner_id: "owner-1",
      period: "7d",
      rows: [],
      totals: {
        quantity: 0,
        requests: 0,
        bytes: 0,
        events: 0,
        estimated_credits_micros: null,
      },
      billing: {
        charging_enabled: false,
        lago_configured: false,
        source: "usage_meter",
        rates_are_approximate: true,
      },
    });

    const { result } = renderHook(() => useBillingUsage("7d"), {
      wrapper: wrapperFactory(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mockGet).toHaveBeenCalledWith("/billing/usage?period=7d");
    expect(result.current.data?.billing.charging_enabled).toBe(false);
  });

  it("rejects wallet or top-up-shaped payloads", async () => {
    mockGet.mockResolvedValue({
      owner_id: "owner-1",
      period: "7d",
      rows: [],
      totals: {
        quantity: 0,
        requests: 0,
        bytes: 0,
        events: 0,
      },
      billing: {
        charging_enabled: true,
        lago_configured: true,
        source: "lago_wallet",
        rates_are_approximate: false,
      },
    });

    const { result } = renderHook(() => useBillingUsage("7d"), {
      wrapper: wrapperFactory(),
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
  });
});
