import { describe, expect, it } from "vitest";
import type { UserCapabilities } from "@/types/api";
import { shouldRedirectFromBilling } from "./billing-availability";

function userWithBilling(billingAvailable: boolean): {
  readonly capabilities: UserCapabilities;
} {
  return {
    capabilities: {
      billing_available: billingAvailable,
    },
  };
}

describe("shouldRedirectFromBilling", () => {
  it("waits while auth and capabilities are still loading", () => {
    expect(
      shouldRedirectFromBilling({
        isLoading: true,
        user: null,
      }),
    ).toBe(false);
  });

  it("redirects when billing is unavailable after auth settles", () => {
    expect(
      shouldRedirectFromBilling({
        isLoading: false,
        user: userWithBilling(false),
      }),
    ).toBe(true);
  });

  it("does not redirect when billing is available", () => {
    expect(
      shouldRedirectFromBilling({
        isLoading: false,
        user: userWithBilling(true),
      }),
    ).toBe(false);
  });
});
