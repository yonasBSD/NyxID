import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api-client";
import type {
  CreateInviteCodeRequest,
  DeactivateInviteCodeResponse,
  InviteCode,
  InviteCodeListResponse,
} from "@/types/admin";

const QUERY_KEY = ["admin", "invite-codes"] as const;

/// List all invite codes (admin only).
export function useAdminInviteCodes() {
  return useQuery({
    queryKey: QUERY_KEY,
    queryFn: async (): Promise<InviteCodeListResponse> => {
      return api.get<InviteCodeListResponse>("/admin/invite-codes");
    },
  });
}

/// Create a new invite code. Returns the created InviteCode (including its
/// generated code string and id).
export function useCreateInviteCode() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (body: CreateInviteCodeRequest): Promise<InviteCode> => {
      return api.post<InviteCode>("/admin/invite-codes", body);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}

/// Deactivate an invite code by ID. Irreversible — admins should mint a new
/// code if a user needs another attempt.
export function useDeactivateInviteCode() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (id: string): Promise<DeactivateInviteCodeResponse> => {
      return api.delete<DeactivateInviteCodeResponse>(
        `/admin/invite-codes/${id}`,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}
