import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type {
  ChannelConversationListResponse,
  ChannelConversationItem,
  CreateChannelConversationRequest,
  UpdateChannelConversationRequest,
} from "@/types/channels";

// -- Queries --

export function useChannelConversations(botId?: string) {
  const params = botId ? `?bot_id=${botId}` : "";
  return useQuery({
    queryKey: ["channel-conversations", botId ?? "all"],
    queryFn: async (): Promise<readonly ChannelConversationItem[]> => {
      const res = await api.get<ChannelConversationListResponse>(
        `/channel-conversations${params}`,
      );
      return res.conversations;
    },
  });
}

// -- Mutations --

export function useCreateChannelConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (
      data: CreateChannelConversationRequest,
    ): Promise<ChannelConversationItem> => {
      return api.post<ChannelConversationItem>(
        "/channel-conversations",
        data,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["channel-conversations"],
      });
      void queryClient.invalidateQueries({ queryKey: ["channel-bots"] });
    },
  });
}

export function useUpdateChannelConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({
      id,
      ...data
    }: { readonly id: string } & UpdateChannelConversationRequest): Promise<ChannelConversationItem> => {
      return api.put<ChannelConversationItem>(
        `/channel-conversations/${id}`,
        data,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["channel-conversations"],
      });
    },
  });
}

export function useDeleteChannelConversation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      return api.delete<void>(`/channel-conversations/${id}`);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["channel-conversations"],
      });
      void queryClient.invalidateQueries({ queryKey: ["channel-bots"] });
    },
  });
}
