import { createIdempotencyKey } from "./idempotency";
import {
  deleteCurrentUserAccountRequest,
  getCurrentUserProfileRequest,
  getChallengeRequest,
  getNotificationSettingsRequest,
  listApprovalsRequest,
  listApprovalRequestsRequest,
  listChallengesRequest,
  loginWithPasswordRequest,
  registerWithPasswordRequest,
  registerPushTokenRequest,
  revokeApprovalRequest,
  rotatePushTokenRequest,
  submitChallengeDecisionRequest,
  telegramDisconnectRequest,
  telegramLinkRequest,
  unregisterPushTokenRequest,
  updateNotificationSettingsRequest,
} from "./http";
import {
  AccountProfile,
  ApprovalItem,
  ChallengeDetail,
  DeleteAccountResponse,
  NotificationSettings,
  PageResponse,
  SubmitDecisionOptions,
  PushTokenRegisterRequest,
  PushTokenRegisterResponse,
  TelegramLinkInfo,
  UpdateNotificationSettingsInput,
} from "./types";

type LoginWithPasswordInput = {
  email: string;
  password: string;
  mfaCode?: string;
};

type LoginWithPasswordResponse = {
  userId: string;
  accessToken: string;
  expiresIn: number;
  refreshToken?: string;
};

type RegisterWithPasswordInput = {
  email: string;
  password: string;
  displayName?: string;
};

type RegisterWithPasswordResponse = {
  userId: string;
  message: string;
};

type SocialProvider = "google" | "github" | "apple";

function getApiBaseUrl(): string {
  const rawBaseUrl = process.env.EXPO_PUBLIC_API_BASE_URL ?? "http://localhost:3001/api/v1";
  const normalized = rawBaseUrl.replace(/\/+$/, "");

  if (normalized.endsWith("/mobile")) {
    return normalized.slice(0, -"/mobile".length);
  }

  return normalized;
}

export const mobileApi = {
  async loginWithPassword(input: LoginWithPasswordInput): Promise<LoginWithPasswordResponse> {
    const response = await loginWithPasswordRequest({
      email: input.email,
      password: input.password,
      mfa_code: input.mfaCode,
    });

    return {
      userId: response.user_id,
      accessToken: response.access_token,
      expiresIn: response.expires_in,
      refreshToken: response.refresh_token,
    };
  },
  async registerWithPassword(
    input: RegisterWithPasswordInput
  ): Promise<RegisterWithPasswordResponse> {
    const response = await registerWithPasswordRequest({
      email: input.email,
      password: input.password,
      display_name: input.displayName,
    });

    return {
      userId: response.user_id,
      message: response.message,
    };
  },
  getSocialAuthorizeUrl(provider: SocialProvider, redirectUri: string): string {
    const encodedRedirectUri = encodeURIComponent(redirectUri);
    return `${getApiBaseUrl()}/auth/social/${provider}?client=mobile&redirect_uri=${encodedRedirectUri}`;
  },
  async getChallenges(): Promise<PageResponse<ChallengeDetail>> {
    return listChallengesRequest();
  },
  async getNotificationSettings(): Promise<NotificationSettings> {
    return getNotificationSettingsRequest();
  },
  async updateNotificationSettings(input: UpdateNotificationSettingsInput): Promise<NotificationSettings> {
    return updateNotificationSettingsRequest(input);
  },
  async telegramLink(): Promise<TelegramLinkInfo> {
    return telegramLinkRequest();
  },
  async telegramDisconnect(): Promise<{ message: string }> {
    return telegramDisconnectRequest();
  },
  async getChallengeById(challengeId: string): Promise<ChallengeDetail> {
    return getChallengeRequest(challengeId);
  },
  async submitDecision(
    challengeId: string,
    decision: "APPROVE" | "DENY",
    durationSec?: number,
    options?: SubmitDecisionOptions
  ): Promise<{ challenge_id: string; status: string; approval_id?: string }> {
    const idempotencyKey =
      options?.idempotencyKey ?? createIdempotencyKey("decision", challengeId);
    return submitChallengeDecisionRequest(challengeId, decision, durationSec, idempotencyKey);
  },
  async getApprovals(): Promise<PageResponse<ApprovalItem>> {
    return listApprovalsRequest();
  },
  async getHistory(page = 1, perPage = 20): Promise<PageResponse<ChallengeDetail>> {
    const response = await listApprovalRequestsRequest({ page, per_page: perPage });
    return {
      ...response,
      items: response.items.filter((item) => item.status !== "PENDING"),
    };
  },
  async revoke(approvalId: string): Promise<{ message: string }> {
    return revokeApprovalRequest(approvalId);
  },
  async registerPushToken(
    payload: PushTokenRegisterRequest
  ): Promise<PushTokenRegisterResponse> {
    return registerPushTokenRequest(payload);
  },
  async rotatePushToken(
    payload: PushTokenRegisterRequest
  ): Promise<PushTokenRegisterResponse> {
    return rotatePushTokenRequest(payload);
  },
  async unregisterPushToken(payload: PushTokenRegisterRequest): Promise<void> {
    await unregisterPushTokenRequest(payload);
  },
  async getAccountProfile(): Promise<AccountProfile> {
    return getCurrentUserProfileRequest();
  },
  async deleteAccount(): Promise<DeleteAccountResponse> {
    return deleteCurrentUserAccountRequest();
  },
};
