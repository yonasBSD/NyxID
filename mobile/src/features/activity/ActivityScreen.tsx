import { useCallback, useEffect, useState } from "react";
import {
  Alert,
  FlatList,
  RefreshControl,
  SectionList,
  StyleSheet,
  Text,
  View,
} from "react-native";
import { useNavigation } from "@react-navigation/native";
import type { NativeStackNavigationProp } from "@react-navigation/native-stack";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ScreenContainer } from "../../components/ScreenContainer";

import { ToastOverlay, type ToastState } from "../../components/ToastOverlay";
import { SegmentControl } from "../../components/SegmentControl";
import { ChallengeCard } from "../../components/ChallengeCard";
import { GrantCard } from "../../components/GrantCard";
import { HistoryCard, HistorySectionHeader } from "../../components/HistoryCard";
import { EmptyState } from "../../components/EmptyState";
import { OfflineBanner } from "../../components/OfflineBanner";
import { FullScreenLoading } from "../../components/FullScreenLoading";
import { useNetworkStatus } from "../../hooks/useNetworkStatus";
import { mobileApi } from "../../lib/api/mobileApi";
import { createIdempotencyKey } from "../../lib/api/idempotency";
import { getDecisionErrorMessage, formatGrantDuration } from "./challengeUiState";
import { mobileTheme } from "../../theme/mobileTheme";
import { spacing, typeScale } from "../../theme/designTokens";
import type { RootStackParamList } from "../../app/AppNavigator";
import type { ActivitySegment } from "./activityTypes";
import type { ChallengeDetail, ApprovalItem } from "../../lib/api/types";

type Nav = NativeStackNavigationProp<RootStackParamList>;

function groupHistoryByDate(items: ChallengeDetail[]) {
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  const todayStr = today.toDateString();
  const yesterdayStr = yesterday.toDateString();

  const groups: Record<string, ChallengeDetail[]> = {};
  for (const item of items) {
    const d = new Date(item.created_at);
    let key: string;
    if (d.toDateString() === todayStr) key = "Today";
    else if (d.toDateString() === yesterdayStr) key = "Yesterday";
    else key = d.toLocaleDateString("en-US", { month: "short", day: "numeric" });

    if (!groups[key]) groups[key] = [];
    groups[key]!.push(item);
  }

  return Object.entries(groups).map(([title, data]) => ({ title, data }));
}

export function ActivityScreen() {
  const navigation = useNavigation<Nav>();
  const queryClient = useQueryClient();
  const { isConnected } = useNetworkStatus();
  const [activeSegment, setActiveSegment] = useState<ActivitySegment>("pending");
  const [toast, setToast] = useState<ToastState | null>(null);
  const [mutatingIds, setMutatingIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2400);
    return () => clearTimeout(t);
  }, [toast]);

  // --- Queries ---
  const pendingQuery = useQuery({
    queryKey: ["challenges", "pending"],
    queryFn: mobileApi.getChallenges,
  });

  const approvalsQuery = useQuery({
    queryKey: ["approvals"],
    queryFn: mobileApi.getApprovals,
  });

  const settingsQuery = useQuery({
    queryKey: ["notifications", "settings"],
    queryFn: mobileApi.getNotificationSettings,
  });

  const historyQuery = useQuery({
    queryKey: ["challenges", "history"],
    queryFn: () => mobileApi.getHistory(1, 50),
  });

  const pendingItems = pendingQuery.data?.items ?? [];
  const activeItems = approvalsQuery.data?.items ?? [];
  const historyItems = historyQuery.data?.items ?? [];
  const grantDurationLabel = formatGrantDuration(settingsQuery.data?.grant_expiry_days);

  const pendingCount = pendingItems.length;
  const activeCount = activeItems.length;

  // --- Mutations ---
  const decideMutation = useMutation({
    mutationFn: async ({ id, decision }: { id: string; decision: "APPROVE" | "DENY" }) => {
      const durationSec = decision === "APPROVE" ? (settingsQuery.data?.grant_expiry_days ?? 30) * 86400 : undefined;
      const idempotencyKey = createIdempotencyKey("decision", id);
      return mobileApi.submitDecision(id, decision, durationSec);
    },
    onMutate: ({ id }) => {
      setMutatingIds((prev) => new Set(prev).add(id));
    },
    onSuccess: (_, { decision }) => {
      void queryClient.invalidateQueries({ queryKey: ["challenges"] });
      void queryClient.invalidateQueries({ queryKey: ["approvals"] });
      setToast({ message: decision === "APPROVE" ? "Approved" : "Denied", kind: "success" });
      if (decision === "APPROVE") setActiveSegment("active");
    },
    onError: (error, { id }) => {
      setToast({ message: getDecisionErrorMessage(error), kind: "error" });
    },
    onSettled: (_, __, { id }) => {
      setMutatingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    },
  });

  const revokeMutation = useMutation({
    mutationFn: (approvalId: string) => mobileApi.revoke(approvalId),
    onMutate: (id) => {
      setMutatingIds((prev) => new Set(prev).add(id));
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["approvals"] });
      setToast({ message: "Revoked", kind: "success" });
    },
    onError: () => {
      setToast({ message: "Failed to revoke. Try again.", kind: "error" });
    },
    onSettled: (_, __, id) => {
      setMutatingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    },
  });

  const handleRevoke = useCallback((grant: ApprovalItem) => {
    Alert.alert("Revoke Access", `Revoke access for ${grant.service_name}?`, [
      { text: "Cancel", style: "cancel" },
      { text: "Revoke", style: "destructive", onPress: () => revokeMutation.mutate(grant.id) },
    ]);
  }, [revokeMutation]);

  const handleRefresh = useCallback(() => {
    if (activeSegment === "pending") void pendingQuery.refetch();
    if (activeSegment === "active") void approvalsQuery.refetch();
    if (activeSegment === "history") void historyQuery.refetch();
  }, [activeSegment, pendingQuery, approvalsQuery, historyQuery]);

  const isRefreshing =
    (activeSegment === "pending" && pendingQuery.isRefetching) ||
    (activeSegment === "active" && approvalsQuery.isRefetching) ||
    (activeSegment === "history" && historyQuery.isRefetching);

  // --- Loading states ---
  const isInitialLoading =
    pendingQuery.isLoading && approvalsQuery.isLoading;

  if (isInitialLoading) {
    return <FullScreenLoading title="Loading activity..." subtitle="Fetching your challenges and grants" />;
  }

  // --- Sorted active items (urgent first) ---
  const sortedActiveItems = [...activeItems].sort(
    (a, b) => new Date(a.expires_at).getTime() - new Date(b.expires_at).getTime()
  );

  const historySections = groupHistoryByDate(
    [...historyItems].sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
  );

  const segments = [
    { label: "Pending", count: pendingCount },
    { label: "Active", count: activeCount },
    { label: "History" },
  ];

  const segmentIndex = activeSegment === "pending" ? 0 : activeSegment === "active" ? 1 : 2;

  return (
    <ScreenContainer>
      <View style={styles.header}>
        <Text style={styles.title}>Activity</Text>
        <Text style={styles.subtitle}>
          {pendingCount} pending · {activeCount} active grant{activeCount !== 1 ? "s" : ""}
        </Text>
      </View>

      <View style={styles.segmentWrap}>
        {!isConnected && <OfflineBanner onRetry={handleRefresh} />}
        <SegmentControl
          segments={segments}
          activeIndex={segmentIndex}
          onPress={(i) => {
            const seg: ActivitySegment[] = ["pending", "active", "history"];
            setActiveSegment(seg[i] ?? "pending");
          }}
        />
      </View>

      {activeSegment === "pending" && (
        pendingItems.length === 0 ? (
          <View style={styles.emptyWrap}>
            <EmptyState preset="pendingEmpty" />
          </View>
        ) : (
          <FlatList
            data={pendingItems}
            keyExtractor={(item) => item.id}
            renderItem={({ item }) => (
              <ChallengeCard
                challenge={item}
                grantDurationLabel={grantDurationLabel}
                isMutating={mutatingIds.has(item.id)}
                onPress={() => navigation.navigate("ActivityDetail", { challengeId: item.id })}
                onApprove={() => decideMutation.mutate({ id: item.id, decision: "APPROVE" })}
                onDeny={() => decideMutation.mutate({ id: item.id, decision: "DENY" })}
              />
            )}
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            refreshControl={
              <RefreshControl refreshing={isRefreshing} onRefresh={handleRefresh} tintColor={mobileTheme.primary} />
            }
          />
        )
      )}

      {activeSegment === "active" && (
        sortedActiveItems.length === 0 ? (
          <View style={styles.emptyWrap}>
            <EmptyState preset="activeEmpty" />
          </View>
        ) : (
          <FlatList
            data={sortedActiveItems}
            keyExtractor={(item) => item.id}
            renderItem={({ item }) => (
              <GrantCard
                grant={item}
                isMutating={mutatingIds.has(item.id)}
                onRevoke={() => handleRevoke(item)}
              />
            )}
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            refreshControl={
              <RefreshControl refreshing={isRefreshing} onRefresh={handleRefresh} tintColor={mobileTheme.primary} />
            }
          />
        )
      )}

      {activeSegment === "history" && (
        historyItems.length === 0 ? (
          <View style={styles.emptyWrap}>
            <EmptyState preset="historyEmpty" />
          </View>
        ) : (
          <SectionList
            sections={historySections}
            keyExtractor={(item) => item.id}
            renderItem={({ item }) => <HistoryCard item={item} />}
            renderSectionHeader={({ section }) => <HistorySectionHeader title={section.title} />}
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            SectionSeparatorComponent={() => <View style={styles.sectionSep} />}
            refreshControl={
              <RefreshControl refreshing={isRefreshing} onRefresh={handleRefresh} tintColor={mobileTheme.primary} />
            }
          />
        )
      )}

      <ToastOverlay toast={toast} bottom={64} />
    </ScreenContainer>
  );
}

const styles = StyleSheet.create({
  header: {
    paddingHorizontal: spacing.xxl,
    paddingTop: spacing.sm,
    minHeight: 41,
    gap: 2,
  },
  title: {
    fontSize: 26,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  subtitle: {
    fontSize: 13,
    color: mobileTheme.textSecondary,
    marginBottom: spacing.md,
  },
  segmentWrap: {
    paddingHorizontal: spacing.xxl,
  },
  listContent: {
    paddingHorizontal: spacing.xxl,
    paddingBottom: spacing.huge,
  },
  separator: {
    height: spacing.sm,
  },
  sectionSep: {
    height: spacing.xs,
  },
  emptyWrap: {
    paddingHorizontal: spacing.xxl,
    paddingTop: spacing.xxl,
  },
});
