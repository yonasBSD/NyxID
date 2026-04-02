import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type {
  ChannelBotListResponse,
  ChannelBotDetail,
  ChannelBotItem,
  CreateChannelBotRequest,
  CreateChannelBotResponse,
} from "@/types/channels";

// -- Queries --

export function useChannelBots() {
  return useQuery({
    queryKey: ["channel-bots"],
    queryFn: async (): Promise<readonly ChannelBotItem[]> => {
      const res = await api.get<ChannelBotListResponse>("/channel-bots");
      return res.bots;
    },
  });
}

export function useChannelBot(id: string) {
  return useQuery({
    queryKey: ["channel-bots", id],
    queryFn: async (): Promise<ChannelBotDetail> => {
      return api.get<ChannelBotDetail>(`/channel-bots/${id}`);
    },
    enabled: Boolean(id),
  });
}

// -- Mutations --

export function useCreateChannelBot() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      data: CreateChannelBotRequest,
    ): Promise<CreateChannelBotResponse> => {
      return api.post<CreateChannelBotResponse>("/channel-bots", data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["channel-bots"] });
    },
  });
}

export function useDeleteChannelBot() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      return api.delete<void>(`/channel-bots/${id}`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["channel-bots"] });
    },
  });
}

export function useVerifyChannelBot() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      return api.post<void>(`/channel-bots/${id}/verify`);
    },
    onSuccess: (_data, id) => {
      void queryClient.invalidateQueries({ queryKey: ["channel-bots", id] });
      void queryClient.invalidateQueries({ queryKey: ["channel-bots"] });
    },
  });
}
