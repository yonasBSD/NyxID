import { z } from "zod";

export const poolStrategySchema = z.enum(["round_robin", "weighted"]);
export type PoolStrategy = z.infer<typeof poolStrategySchema>;

const slugSchema = z
  .string()
  .trim()
  .min(1, "Slug is required")
  .max(80, "Slug must be 80 characters or fewer")
  .regex(
    /^[a-z0-9]+(?:-[a-z0-9]+)*$/,
    "Use lowercase letters, numbers, and single hyphens",
  );

export const poolMemberSchema = z.object({
  user_service_id: z.string().min(1, "Select a service"),
  weight: z.number().int().min(1).max(1000),
  enabled: z.boolean(),
});

export const servicePoolSchema = z.object({
  id: z.string(),
  user_id: z.string(),
  slug: z.string(),
  name: z.string(),
  description: z.string().nullable().optional(),
  strategy: poolStrategySchema,
  members: z.array(poolMemberSchema),
  rr_counter: z.number().int(),
  is_active: z.boolean(),
  created_at: z.string(),
  updated_at: z.string(),
});

export const servicePoolListResponseSchema = z.object({
  pools: z.array(servicePoolSchema),
});

export const createServicePoolSchema = z.object({
  slug: slugSchema,
  name: z
    .string()
    .trim()
    .min(1, "Name is required")
    .max(128, "Name must be 128 characters or fewer"),
  description: z
    .string()
    .max(1024, "Description must be 1024 characters or fewer")
    .optional(),
  strategy: poolStrategySchema,
  members: z.array(poolMemberSchema).max(50),
  is_active: z.boolean().optional(),
  org_id: z.string().optional(),
});

export const updateServicePoolSchema = createServicePoolSchema
  .omit({ org_id: true })
  .partial();

export const setPoolMembersSchema = z.object({
  members: z.array(poolMemberSchema).max(50),
});

export type ServicePoolMember = z.infer<typeof poolMemberSchema>;
export type ServicePool = z.infer<typeof servicePoolSchema>;
export type ServicePoolListResponse = z.infer<
  typeof servicePoolListResponseSchema
>;
export type CreateServicePoolInput = z.infer<typeof createServicePoolSchema>;
export type UpdateServicePoolInput = z.infer<typeof updateServicePoolSchema>;
export type SetPoolMembersInput = z.infer<typeof setPoolMembersSchema>;
