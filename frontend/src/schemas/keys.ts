import { z } from "zod";
import { credentialSourceSchema } from "./orgs";

export { credentialSourceSchema } from "./orgs";
export type {
  CredentialSource,
  CredentialSourcePersonal,
  CredentialSourceOrg,
} from "./orgs";

/**
 * Wire-format response for `GET /api/v1/user-services` entries.
 *
 * Every item carries a `credential_source` tag so the client can group
 * personal credentials vs. org-inherited ones and disable write/proxy
 * actions on items the user may see but cannot modify (viewer role or
 * scope-excluded).
 *
 * Note: we only validate the fields the frontend actually reads. Extra
 * fields from the backend pass through unchanged because z.object is
 * permissive by default on unknown keys in Zod 4 when used without
 * `.strict()`.
 */
export const userServiceResponseSchema = z.object({
  id: z.string(),
  slug: z.string(),
  endpoint_id: z.string(),
  api_key_id: z.string().nullable().optional(),
  auth_method: z.string(),
  auth_key_name: z.string(),
  catalog_service_id: z.string().nullable().optional(),
  node_id: z.string().nullable().optional(),
  node_priority: z.number().int(),
  is_active: z.boolean(),
  identity_propagation_mode: z.string(),
  identity_include_user_id: z.boolean(),
  identity_include_email: z.boolean(),
  identity_include_name: z.boolean(),
  identity_jwt_audience: z.string().nullable().optional(),
  forward_access_token: z.boolean(),
  inject_delegation_token: z.boolean(),
  delegation_token_scope: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
  credential_source: credentialSourceSchema,
});

export type UserServiceResponse = z.infer<typeof userServiceResponseSchema>;

export const userServiceListResponseSchema = z.object({
  services: z.array(userServiceResponseSchema),
});

export type UserServiceListResponse = z.infer<
  typeof userServiceListResponseSchema
>;
