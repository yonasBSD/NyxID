import { z } from "zod";

export const BILLING_USAGE_PERIODS = ["24h", "1d", "7d", "30d", "90d", "all"] as const;

export type BillingUsagePeriod = (typeof BILLING_USAGE_PERIODS)[number];

export const billingMetricSchema = z.enum(["tokens", "requests", "bytes"]);

export const billingReadOnlyBlockSchema = z.object({
  charging_enabled: z.boolean(),
  lago_configured: z.boolean(),
  source: z.literal("usage_meter"),
  rates_are_approximate: z.literal(true),
});

export const billingUsageRowSchema = z.object({
  service_slug: z.string().nullable().optional(),
  service_id: z.string().nullable().optional(),
  metric: billingMetricSchema,
  lago_metric_code: z.string(),
  layer: z.string(),
  quantity: z.number().int(),
  requests: z.number().int(),
  bytes: z.number().int(),
  events: z.number().int().nonnegative(),
  lago_acked: z.boolean(),
  estimated_credits_micros: z.number().int().nullable().optional(),
});

export const billingUsageTotalsSchema = z.object({
  quantity: z.number().int(),
  requests: z.number().int(),
  bytes: z.number().int(),
  events: z.number().int().nonnegative(),
  estimated_credits_micros: z.number().int().nullable().optional(),
});

export const billingUsageResponseSchema = z.object({
  owner_id: z.string().min(1),
  period: z.string().min(1),
  rows: z.array(billingUsageRowSchema),
  totals: billingUsageTotalsSchema,
  billing: billingReadOnlyBlockSchema,
});

export type BillingMetric = z.infer<typeof billingMetricSchema>;
export type BillingReadOnlyBlock = z.infer<typeof billingReadOnlyBlockSchema>;
export type BillingUsageRow = z.infer<typeof billingUsageRowSchema>;
export type BillingUsageTotals = z.infer<typeof billingUsageTotalsSchema>;
export type BillingUsageResponse = z.infer<typeof billingUsageResponseSchema>;
