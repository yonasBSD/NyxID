import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type {
  CreateServicePoolInput,
  ServicePool,
  ServicePoolListResponse,
  ServicePoolMember,
  SetPoolMembersInput,
  UpdateServicePoolInput,
} from "@/schemas/pools";

const SERVICE_POOLS_KEY = ["service-pools"] as const;

export function useServicePools() {
  return useQuery({
    queryKey: SERVICE_POOLS_KEY,
    queryFn: async (): Promise<readonly ServicePool[]> => {
      const res = await api.get<ServicePoolListResponse>("/service-pools");
      return res.pools;
    },
  });
}

export function useServicePool(poolId: string | null | undefined) {
  return useQuery({
    queryKey: [...SERVICE_POOLS_KEY, poolId],
    queryFn: async (): Promise<ServicePool> => {
      return api.get<ServicePool>(`/service-pools/${encodeURIComponent(poolId!)}`);
    },
    enabled: Boolean(poolId),
  });
}

function invalidatePools(
  queryClient: ReturnType<typeof useQueryClient>,
  poolId?: string,
) {
  void queryClient.invalidateQueries({ queryKey: SERVICE_POOLS_KEY });
  void queryClient.invalidateQueries({ queryKey: ["keys"] });
  if (poolId) {
    void queryClient.invalidateQueries({
      queryKey: [...SERVICE_POOLS_KEY, poolId],
    });
  }
}

export function useCreateServicePool() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: CreateServicePoolInput): Promise<ServicePool> => {
      return api.post<ServicePool>("/service-pools", input);
    },
    onSuccess: () => invalidatePools(queryClient),
  });
}

export function useUpdateServicePool() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      input: UpdateServicePoolInput & { readonly poolId: string },
    ): Promise<ServicePool> => {
      const { poolId, ...body } = input;
      return api.put<ServicePool>(
        `/service-pools/${encodeURIComponent(poolId)}`,
        body,
      );
    },
    onSuccess: (_data, variables) => invalidatePools(queryClient, variables.poolId),
  });
}

export function useDeleteServicePool() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (poolId: string): Promise<void> => {
      return api.delete<void>(`/service-pools/${encodeURIComponent(poolId)}`);
    },
    onSuccess: () => invalidatePools(queryClient),
  });
}

export function useSetServicePoolMembers() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      input: SetPoolMembersInput & { readonly poolId: string },
    ): Promise<ServicePool> => {
      const { poolId, members } = input;
      return api.put<ServicePool>(
        `/service-pools/${encodeURIComponent(poolId)}/members`,
        { members },
      );
    },
    onSuccess: (_data, variables) => invalidatePools(queryClient, variables.poolId),
  });
}

export function useAddServicePoolMember() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      input: ServicePoolMember & { readonly poolId: string },
    ): Promise<ServicePool> => {
      const { poolId, ...member } = input;
      return api.post<ServicePool>(
        `/service-pools/${encodeURIComponent(poolId)}/members`,
        member,
      );
    },
    onSuccess: (_data, variables) => invalidatePools(queryClient, variables.poolId),
  });
}

export function useRemoveServicePoolMember() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input: {
      readonly poolId: string;
      readonly userServiceId: string;
    }): Promise<ServicePool> => {
      return api.delete<ServicePool>(
        `/service-pools/${encodeURIComponent(input.poolId)}/members/${encodeURIComponent(input.userServiceId)}`,
      );
    },
    onSuccess: (_data, variables) => invalidatePools(queryClient, variables.poolId),
  });
}
