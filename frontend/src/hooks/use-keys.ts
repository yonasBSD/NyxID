import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type {
  KeyInfo,
  KeyListResponse,
  CatalogEntry,
  CatalogListResponse,
} from "@/types/keys";

// -- Queries --

export function useKeys() {
  return useQuery({
    queryKey: ["keys"],
    queryFn: async (): Promise<readonly KeyInfo[]> => {
      const res = await api.get<KeyListResponse>("/keys");
      return res.keys;
    },
  });
}

export function useKey(keyId: string) {
  return useQuery({
    queryKey: ["keys", keyId],
    queryFn: async (): Promise<KeyInfo> => {
      return api.get<KeyInfo>(`/keys/${keyId}`);
    },
    enabled: Boolean(keyId),
  });
}

export function useCatalog() {
  return useQuery({
    queryKey: ["catalog"],
    queryFn: async (): Promise<readonly CatalogEntry[]> => {
      const res = await api.get<CatalogListResponse>("/catalog");
      return res.entries;
    },
  });
}

// -- Mutations --

interface CreateKeyParams {
  readonly service_slug?: string;
  readonly credential?: string;
  readonly label: string;
  readonly endpoint_url?: string;
  readonly slug?: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
  readonly node_id?: string;
  readonly service_type?: string;
  readonly ssh_host?: string;
  readonly ssh_port?: number;
  readonly ssh_certificate_auth?: boolean;
  readonly ssh_principals?: string;
  readonly ssh_certificate_ttl_minutes?: number;
}

export function useCreateKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: CreateKeyParams): Promise<KeyInfo> => {
      return api.post<KeyInfo>("/keys", params);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({ queryKey: ["llm-status"] });
    },
  });
}

export function useDeleteKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (keyId: string): Promise<void> => {
      return api.delete<void>(`/keys/${keyId}`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({ queryKey: ["llm-status"] });
    },
  });
}

interface UpdateKeyParams {
  readonly keyId: string;
  readonly label?: string;
  readonly endpoint_url?: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
  readonly node_id?: string;
  readonly is_active?: boolean;
}

export function useUpdateKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateKeyParams): Promise<KeyInfo> => {
      const { keyId, ...body } = params;
      return api.put<KeyInfo>(`/keys/${keyId}`, body);
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({
        queryKey: ["keys", variables.keyId],
      });
    },
  });
}

interface UpdateEndpointParams {
  readonly endpointId: string;
  readonly url?: string;
  readonly label?: string;
}

export function useUpdateEndpoint() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateEndpointParams): Promise<void> => {
      return api.put<void>(`/endpoints/${params.endpointId}`, {
        url: params.url,
        label: params.label,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}

interface UpdateUserServiceParams {
  readonly serviceId: string;
  readonly auth_method?: string;
  readonly auth_key_name?: string;
  readonly node_id?: string;
  readonly node_priority?: number;
  readonly is_active?: boolean;
}

export function useUpdateUserService() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateUserServiceParams): Promise<void> => {
      const { serviceId, ...body } = params;
      return api.put<void>(`/user-services/${serviceId}`, body);
    },
    onSuccess: (_data, variables) => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
      void queryClient.invalidateQueries({
        queryKey: ["keys", variables.serviceId],
      });
    },
  });
}

interface UpdateExternalApiKeyParams {
  readonly keyId: string;
  readonly label?: string;
  readonly credential?: string;
}

export function useUpdateExternalApiKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (params: UpdateExternalApiKeyParams): Promise<void> => {
      const { keyId, ...body } = params;
      return api.put<void>(`/api-keys/external/${keyId}`, body);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["keys"] });
    },
  });
}
