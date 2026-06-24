import { describe, expect, it } from "vitest";
import {
  BILLING_USAGE_PERIODS,
  billingReadOnlyBlockSchema,
  billingUsageResponseSchema,
} from "./billing";

describe("billing schemas", () => {
  it("accepts the read-only usage response shape returned by `/billing/usage`", () => {
    const result = billingUsageResponseSchema.safeParse({
      owner_id: "owner-1",
      period: "30d",
      rows: [
        {
          service_slug: "openai",
          service_id: "svc-1",
          metric: "tokens",
          lago_metric_code: "openai.tokens",
          layer: "resale",
          quantity: 42,
          requests: 0,
          bytes: 0,
          events: 2,
          lago_acked: true,
          estimated_credits_micros: 4200,
        },
      ],
      totals: {
        quantity: 42,
        requests: 0,
        bytes: 0,
        events: 2,
        estimated_credits_micros: 4200,
      },
      billing: {
        charging_enabled: false,
        lago_configured: true,
        source: "usage_meter",
        rates_are_approximate: true,
      },
    });

    expect(result.success).toBe(true);
  });

  it("keeps the display contract read-only and approximate", () => {
    expect(
      billingReadOnlyBlockSchema.safeParse({
        charging_enabled: true,
        lago_configured: true,
        source: "lago_wallet",
        rates_are_approximate: false,
      }).success,
    ).toBe(false);
  });

  it("lists only backend-supported usage periods", () => {
    expect(BILLING_USAGE_PERIODS).toEqual(["24h", "1d", "7d", "30d", "90d", "all"]);
  });
});
