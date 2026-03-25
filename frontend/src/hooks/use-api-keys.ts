import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type { ApiKey, ApiKeyCreateResponse } from "@/types/api";
import type { CreateApiKeyFormData } from "@/schemas/api-keys";

export function useApiKeys() {
  return useQuery({
    queryKey: ["api-keys"],
    queryFn: async (): Promise<readonly ApiKey[]> => {
      const res = await api.get<{ readonly keys: readonly ApiKey[] }>(
        "/api-keys",
      );
      return res.keys;
    },
  });
}

export function useApiKey(keyId: string) {
  return useQuery({
    queryKey: ["api-keys", keyId],
    queryFn: async (): Promise<ApiKey> => {
      return api.get<ApiKey>(`/api-keys/${keyId}`);
    },
    enabled: Boolean(keyId),
  });
}

interface CreateApiKeyPayload {
  readonly name: string;
  readonly scopes: string;
  readonly expires_at: string | null;
  readonly description?: string;
  readonly allowed_service_ids?: readonly string[];
  readonly allowed_node_ids?: readonly string[];
  readonly allow_all_services?: boolean;
  readonly allow_all_nodes?: boolean;
}

export function useCreateApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      data: CreateApiKeyFormData,
    ): Promise<ApiKeyCreateResponse> => {
      // Backend expects scopes as a space-separated string, not an array.
      // expires_at must be ISO 8601 datetime (not bare date like "2026-03-25").
      const allowAllServices = data.allow_all_services ?? true;
      const allowAllNodes = data.allow_all_nodes ?? true;
      const payload: CreateApiKeyPayload = {
        name: data.name,
        scopes: data.scopes.join(" "),
        expires_at: data.expires_at
          ? new Date(`${data.expires_at}T23:59:59Z`).toISOString()
          : null,
        description: data.description ?? undefined,
        allowed_service_ids: allowAllServices
          ? undefined
          : (data.allowed_service_ids ?? []),
        allowed_node_ids: allowAllNodes
          ? undefined
          : (data.allowed_node_ids ?? []),
        allow_all_services: allowAllServices,
        allow_all_nodes: allowAllNodes,
      };
      return api.post<ApiKeyCreateResponse>("/api-keys", payload);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["api-keys"] });
    },
  });
}

interface UpdateApiKeyParams {
  readonly keyId: string;
  readonly name?: string;
  readonly description?: string;
  readonly scopes?: string;
  readonly allowed_service_ids?: readonly string[];
  readonly allowed_node_ids?: readonly string[];
  readonly allow_all_services?: boolean;
  readonly allow_all_nodes?: boolean;
}

export function useUpdateApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateApiKeyParams): Promise<ApiKey> => {
      const { keyId, ...body } = params;
      return api.put<ApiKey>(`/api-keys/${keyId}`, body);
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: ["api-keys"] });
      void queryClient.invalidateQueries({
        queryKey: ["api-keys", variables.keyId],
      });
    },
  });
}

export function useDeleteApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      return api.delete<void>(`/api-keys/${id}`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["api-keys"] });
    },
  });
}

export function useRotateApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<ApiKeyCreateResponse> => {
      return api.post<ApiKeyCreateResponse>(`/api-keys/${id}/rotate`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["api-keys"] });
    },
  });
}
