import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { NativeStackScreenProps } from "@react-navigation/native-stack";
import { Alert, Pressable, ScrollView, StyleSheet, Switch, Text, View } from "react-native";
import { RootStackParamList } from "../../app/AppNavigator";

import { PrimaryButton } from "../../components/PrimaryButton";
import { ScreenContainer } from "../../components/ScreenContainer";
import { OfflineBanner } from "../../components/OfflineBanner";
import { TelegramLinkModal } from "../../components/TelegramLinkModal";
import { ToastKind, ToastOverlay, ToastState } from "../../components/ToastOverlay";
import { useAuthSession } from "../auth/AuthSessionContext";
import { useNetworkStatus } from "../../hooks/useNetworkStatus";
import { mobileApi } from "../../lib/api/mobileApi";
import { mobileTheme } from "../../theme/mobileTheme";
import { flowStyles } from "../../theme/flowStyles";
import { radius, spacing, typeScale } from "../../theme/designTokens";
import { useEffect, useState } from "react";

type Props = NativeStackScreenProps<RootStackParamList, "AccountSettings">;

function resolveDeleteAccountError(error: unknown): {
  message: string;
  shouldForceSignOut: boolean;
} {
  const raw = error instanceof Error ? error.message : "";
  const code = raw.toLowerCase();

  if (
    code.includes("auth_session_missing") ||
    code.includes("unauthorized") ||
    code.includes("invalid_token") ||
    code.includes("token_expired") ||
    code.includes("request_failed_401")
  ) {
    return { message: "Session expired. Please sign in again.", shouldForceSignOut: true };
  }

  if (code.includes("user_not_found") || code.includes("not found") || code.includes("request_failed_404")) {
    return { message: "Account not found or already deleted.", shouldForceSignOut: true };
  }

  if (code.includes("network request failed") || code.includes("failed to fetch")) {
    return { message: "Network error. Check API server and try again.", shouldForceSignOut: false };
  }

  const fallback = __DEV__ && raw ? raw : "Failed to delete account. Please try again.";
  return { message: fallback, shouldForceSignOut: false };
}

function AccountRow({
  label,
  value,
  isLast,
  onPress,
}: {
  label: string;
  value?: string;
  isLast?: boolean;
  onPress?: () => void;
}) {
  const content = (
    <View style={[styles.accountRow, isLast && styles.accountRowLast]}>
      <Text style={styles.accountRowLabel}>{label}</Text>
      <View style={styles.accountRowRight}>
        {value ? <Text style={styles.accountRowValue}>{value}</Text> : null}
        {onPress ? <Text style={styles.accountRowArrow}>→</Text> : null}
      </View>
    </View>
  );
  if (onPress) {
    return <Pressable onPress={onPress}>{content}</Pressable>;
  }
  return content;
}

function getInitials(name?: string | null, email?: string): string {
  if (name) {
    return name
      .split(" ")
      .map((w) => w[0])
      .join("")
      .toUpperCase()
      .slice(0, 2);
  }
  return (email?.[0] ?? "?").toUpperCase();
}

export function AccountSettingsScreen({ navigation }: Props) {
  const [toast, setToast] = useState<ToastState | null>(null);
  const queryClient = useQueryClient();
  const { signOut } = useAuthSession();
  const { isConnected } = useNetworkStatus();

  const {
    data: profile,
    isLoading: isProfileLoading,
    isError: isProfileError,
    error: profileError,
    refetch: refetchProfile,
  } = useQuery({
    queryKey: ["account", "profile"],
    queryFn: () => mobileApi.getAccountProfile(),
  });

  const showToast = (message: string, kind: ToastKind) => setToast({ message, kind });

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), 2400);
    return () => clearTimeout(timer);
  }, [toast]);

  const {
    data: notifSettings,
    isLoading: isNotifLoading,
    refetch: refetchNotifSettings,
  } = useQuery({
    queryKey: ["account", "notificationSettings"],
    queryFn: () => mobileApi.getNotificationSettings(),
  });

  const notifMutation = useMutation({
    mutationFn: (payload: { telegram_enabled?: boolean; push_enabled?: boolean }) =>
      mobileApi.updateNotificationSettings(payload),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["account", "notificationSettings"] });
      showToast("Notification settings updated", "success");
    },
    onError: (error) => {
      const msg = error instanceof Error ? error.message : "Failed to update settings";
      showToast(msg, "error");
      void refetchNotifSettings();
    },
  });

  const [isTelegramLinkVisible, setIsTelegramLinkVisible] = useState(false);

  const telegramDisconnectMutation = useMutation({
    mutationFn: () => mobileApi.telegramDisconnect(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["account", "notificationSettings"] });
      showToast("Telegram disconnected", "success");
    },
    onError: (error) => {
      showToast(error instanceof Error ? error.message : "Failed to disconnect", "error");
    },
  });

  const handleTelegramRowPress = () => {
    if (notifSettings?.telegram_connected) {
      Alert.alert(
        "Disconnect Telegram",
        `Disconnect @${notifSettings.telegram_username ?? "Telegram"}? You will no longer receive Telegram notifications.`,
        [
          { text: "Cancel", style: "cancel" },
          {
            text: "Disconnect",
            style: "destructive",
            onPress: () => telegramDisconnectMutation.mutate(),
          },
        ]
      );
    } else {
      setIsTelegramLinkVisible(true);
    }
  };

  const handleTelegramConnected = () => {
    setIsTelegramLinkVisible(false);
    void queryClient.invalidateQueries({ queryKey: ["account", "notificationSettings"] });
    showToast("Telegram connected!", "success");
  };

  const handleTogglePush = (newValue: boolean) => {
    if (newValue) {
      notifMutation.mutate({ push_enabled: true });
      return;
    }
    Alert.alert(
      "Disable Push Notifications",
      "You will no longer receive push notifications for approval requests. Are you sure?",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Disable",
          style: "destructive",
          onPress: () => notifMutation.mutate({ push_enabled: false }),
        },
      ]
    );
  };

  const handleToggleTelegram = (newValue: boolean) => {
    if (newValue) {
      notifMutation.mutate({ telegram_enabled: true });
      return;
    }
    Alert.alert(
      "Disable Telegram Notifications",
      "You will no longer receive Telegram notifications for approval requests. Are you sure?",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Disable",
          style: "destructive",
          onPress: () => notifMutation.mutate({ telegram_enabled: false }),
        },
      ]
    );
  };

  const deleteAccountMutation = useMutation({
    mutationFn: () => mobileApi.deleteAccount(),
    onSuccess: async () => {
      showToast("Account deleted. You have been signed out.", "success");
      try { await signOut(); } catch {}
      queryClient.clear();
    },
    onError: (error) => {
      const resolved = resolveDeleteAccountError(error);
      showToast(resolved.message, "error");
      if (resolved.shouldForceSignOut) {
        void signOut().then(() => queryClient.clear()).catch(() => {});
      }
    },
  });

  const handleSignOut = () => {
    Alert.alert("Sign Out", "Do you want to sign out from this account?", [
      { text: "Cancel", style: "cancel" },
      {
        text: "Sign Out",
        style: "destructive",
        onPress: () => {
          showToast("You are signed out.", "info");
          void signOut().then(() => queryClient.clear()).catch(() => {});
        },
      },
    ]);
  };

  const handleDeleteAccount = () => {
    Alert.alert(
      "Delete Account",
      "This action is permanent and will permanently delete your account and server-side data.",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Delete",
          style: "destructive",
          onPress: () => {
            setToast(null);
            deleteAccountMutation.mutate();
          },
        },
      ]
    );
  };

  const initials = getInitials(profile?.display_name, profile?.email);
  const isOffline = !isConnected;
  const profileOpacity = isOffline ? 0.5 : 1;

  return (
    <ScreenContainer>
      <ScrollView
        style={flowStyles.content}
        contentContainerStyle={[flowStyles.scrollContent, { paddingHorizontal: spacing.xxl }]}
        showsVerticalScrollIndicator={false}
      >
        {isOffline && <OfflineBanner subtitle="Some features unavailable" onRetry={() => refetchProfile()} />}

        {/* User identity header */}
        <View style={styles.identityHeader}>
          <View style={styles.avatarCircle}>
            <Text style={styles.avatarText}>{initials}</Text>
          </View>
          <View style={styles.identityInfo}>
            <Text style={styles.identityName}>{profile?.display_name ?? "User"}</Text>
            <Text style={styles.identityEmail}>{profile?.email ?? "..."}</Text>
          </View>
          <View style={[styles.statusBadge, isOffline && styles.statusBadgeOffline]}>
            <Text style={[styles.statusBadgeText, isOffline && styles.statusBadgeTextOffline]}>
              {isOffline ? "OFFLINE" : "ACTIVE"}
            </Text>
          </View>
        </View>

        {/* Profile card */}
        <View style={[styles.card, { opacity: profileOpacity }]}>
          <Text style={styles.cardTitle}>Profile</Text>
          {isProfileLoading ? (
            <Text style={styles.metaText}>Loading...</Text>
          ) : isProfileError || !profile ? (
            <>
              <Text style={styles.errorText}>Failed to load profile</Text>
              <PrimaryButton label="Retry" kind="ghost" onPress={() => refetchProfile()} />
            </>
          ) : (
            <>
              <AccountRow label="Display Name" value={profile.display_name ?? "Not set"} />
              <AccountRow label="Email" value={profile.email} />
              <AccountRow label="Sign-in Method" value="GitHub" isLast />
            </>
          )}
        </View>

        {/* Notifications card */}
        <View style={[styles.card, isOffline && { opacity: 0.35 }]}>
          <Text style={styles.cardTitle}>Notifications</Text>
          {isOffline ? (
            <Text style={styles.offlineNote}>Requires network connection</Text>
          ) : isNotifLoading ? (
            <Text style={styles.metaText}>Loading...</Text>
          ) : (
            <>
              <View style={styles.accountRow}>
                <Text style={styles.accountRowLabel}>Push Notifications</Text>
                <View style={styles.accountRowRight}>
                  {notifSettings && notifSettings.push_device_count > 0 ? (
                    <Switch
                      value={notifSettings.push_enabled}
                      onValueChange={handleTogglePush}
                      disabled={notifMutation.isPending}
                      trackColor={{ false: mobileTheme.borderSoft, true: mobileTheme.success }}
                    />
                  ) : (
                    <Text style={styles.accountRowValue}>No device</Text>
                  )}
                </View>
              </View>
              {notifSettings?.telegram_connected ? (
                <>
                  <Pressable onPress={handleTelegramRowPress}>
                    <View style={styles.accountRow}>
                      <Text style={styles.accountRowLabel}>Telegram</Text>
                      <View style={styles.accountRowRight}>
                        <Text style={styles.accountRowValue}>@{notifSettings.telegram_username ?? "Connected"}</Text>
                        <Text style={styles.accountRowArrow}>→</Text>
                      </View>
                    </View>
                  </Pressable>
                  <View style={[styles.accountRow, styles.accountRowLast]}>
                    <Text style={styles.accountRowLabel}>Telegram Alerts</Text>
                    <Switch
                      value={notifSettings.telegram_enabled}
                      onValueChange={handleToggleTelegram}
                      disabled={notifMutation.isPending}
                      trackColor={{ false: mobileTheme.borderSoft, true: mobileTheme.success }}
                    />
                  </View>
                </>
              ) : (
                <Pressable onPress={handleTelegramRowPress}>
                  <View style={[styles.accountRow, styles.accountRowLast]}>
                    <Text style={styles.accountRowLabel}>Telegram</Text>
                    <View style={styles.accountRowRight}>
                      <Text style={styles.accountRowValue}>Not linked</Text>
                      <Text style={styles.accountRowArrow}>→</Text>
                    </View>
                  </View>
                </Pressable>
              )}
            </>
          )}
        </View>

        {/* Actions */}
        <View style={styles.actionsWrap}>
          <PrimaryButton
            label="Sign Out"
            kind="ghost"
            disabled={deleteAccountMutation.isPending || isOffline}
            onPress={handleSignOut}
          />
          <PrimaryButton
            label={deleteAccountMutation.isPending ? "Deleting..." : "Delete Account"}
            kind="danger"
            disabled={deleteAccountMutation.isPending || isOffline}
            onPress={handleDeleteAccount}
          />
        </View>

        {/* Legal links */}
        <View style={styles.legalRow}>
          <Pressable onPress={() => navigation.navigate("TermsOfService")}>
            <Text style={styles.legalLink}>Terms of Service</Text>
          </Pressable>
          <Text style={styles.legalDot}>·</Text>
          <Pressable onPress={() => navigation.navigate("PrivacyPolicy")}>
            <Text style={styles.legalLink}>Privacy Policy</Text>
          </Pressable>
        </View>
      </ScrollView>
      <TelegramLinkModal
        visible={isTelegramLinkVisible}
        onDismiss={() => setIsTelegramLinkVisible(false)}
        onConnected={handleTelegramConnected}
      />
      <ToastOverlay toast={toast} />
    </ScreenContainer>
  );
}

const styles = StyleSheet.create({
  identityHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    marginBottom: spacing.xxl,
  },
  avatarCircle: {
    width: 44,
    height: 44,
    borderRadius: 22,
    // RN doesn't support linear-gradient natively on View bg.
    // Use two overlapping halves to approximate gradient(135deg, #8B5CF6, #6D42D9).
    backgroundColor: "#7A4FE3",
    alignItems: "center",
    justifyContent: "center",
  },
  avatarText: {
    fontSize: 18,
    fontWeight: "700",
    color: "#FFFFFF",
    fontFamily: "SpaceGrotesk_700Bold",
  },
  identityInfo: {
    flex: 1,
    minWidth: 0,
  },
  identityName: {
    fontSize: 18,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  identityEmail: {
    fontSize: 12,
    color: mobileTheme.textMuted,
    marginTop: 1,
  },
  statusBadge: {
    paddingHorizontal: 10,
    paddingVertical: 3,
    borderRadius: 20,
    backgroundColor: "rgba(52,211,153,0.1)",
    borderWidth: 1,
    borderColor: "rgba(52,211,153,0.2)",
  },
  statusBadgeOffline: {
    backgroundColor: "rgba(239,68,68,0.1)",
    borderColor: "rgba(239,68,68,0.2)",
  },
  statusBadgeText: {
    fontSize: 10,
    fontWeight: "700",
    color: mobileTheme.success,
  },
  statusBadgeTextOffline: {
    color: "#FCA5A5",
  },
  card: {
    borderRadius: radius.lg,
    borderWidth: 1,
    borderColor: mobileTheme.borderSoft,
    backgroundColor: mobileTheme.card,
    padding: spacing.xl,
    gap: 0,
    marginBottom: spacing.xl,
  },
  cardTitle: {
    ...typeScale.title,
    color: mobileTheme.textPrimary,
    marginBottom: 0,
  },
  accountRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingVertical: 14,
    borderBottomWidth: 1,
    borderBottomColor: mobileTheme.borderSoft,
  },
  accountRowLast: {
    borderBottomWidth: 0,
  },
  accountRowLabel: {
    fontSize: 14,
    fontWeight: "500",
    color: mobileTheme.textSecondary,
  },
  accountRowRight: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  accountRowValue: {
    fontSize: 14,
    fontWeight: "600",
    color: mobileTheme.textPrimary,
  },
  accountRowArrow: {
    fontSize: 14,
    color: mobileTheme.textMuted,
  },
  actionsWrap: {
    gap: spacing.md,
    marginTop: spacing.xs,
  },
  metaText: {
    color: mobileTheme.textSecondary,
    ...typeScale.body,
    paddingVertical: spacing.md,
  },
  errorText: {
    color: "#FCA5A5",
    ...typeScale.caption,
    paddingVertical: spacing.md,
  },
  offlineNote: {
    fontSize: 12,
    color: mobileTheme.textMuted,
    textAlign: "center",
    paddingVertical: spacing.md,
  },
  legalRow: {
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 8,
    marginTop: spacing.xxl,
    paddingBottom: spacing.md,
  },
  legalLink: {
    fontSize: 12,
    color: mobileTheme.textMuted,
    textDecorationLine: "underline",
  },
  legalDot: {
    fontSize: 12,
    color: mobileTheme.textMuted,
  },
});
