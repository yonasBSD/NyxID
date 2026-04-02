import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Dimensions,
  FlatList,
  Modal,
  Pressable,
  RefreshControl,
  ScrollView,
  SectionList,
  StyleSheet,
  Text,
  View,
} from "react-native";
import Animated, {
  useSharedValue,
  useAnimatedStyle,
  withSpring,
  withTiming,
  runOnJS,
} from "react-native-reanimated";
import {
  GestureHandlerRootView,
  Gesture,
  GestureDetector,
} from "react-native-gesture-handler";
import { useNavigation, useRoute, type RouteProp } from "@react-navigation/native";
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
import { getDecisionErrorMessage, formatGrantDuration, getChallengeActionState } from "./challengeUiState";
import { StatusBadge } from "../../components/StatusBadge";
import { PrimaryButton } from "../../components/PrimaryButton";
import { mobileTheme } from "../../theme/mobileTheme";
import { flowStyles } from "../../theme/flowStyles";
import { radius, spacing, typeScale } from "../../theme/designTokens";
import type { RootStackParamList } from "../../app/AppNavigator";
import type { ActivitySegment } from "./activityTypes";
import type { ChallengeDetail, ApprovalItem } from "../../lib/api/types";
import { usePushPollingActive } from "../../lib/notifications/pushPollingSignal";

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

const SCREEN_HEIGHT = Dimensions.get("window").height;
const SHEET_TOP = 120;
const SHEET_HEIGHT = SCREEN_HEIGHT - SHEET_TOP;
const CLOSE_THRESHOLD = 60;

function DetailRow({ label, value, isLast, valueColor }: {
  label: string;
  value: string;
  isLast?: boolean;
  valueColor?: string;
}) {
  return (
    <View style={isLast ? flowStyles.rowLast : flowStyles.row}>
      <Text style={flowStyles.rowLabel}>{label}</Text>
      <Text style={[flowStyles.rowValue, valueColor ? { color: valueColor } : undefined]}>
        {value}
      </Text>
    </View>
  );
}

function ChallengeDetailSheet({
  challenge,
  grantDurationLabel,
  onClose,
  onApprove,
  onDeny,
  isMutating,
}: {
  challenge: ChallengeDetail | null;
  grantDurationLabel: string;
  onClose: () => void;
  onApprove?: (id: string) => void;
  onDeny?: (id: string) => void;
  isMutating?: boolean;
}) {
  const [modalVisible, setModalVisible] = useState(false);
  const isDismissing = useRef(false);
  // Keep a snapshot of the challenge so we can render during the dismiss animation
  const displayChallenge = useRef<ChallengeDetail | null>(null);
  const translateY = useSharedValue(SHEET_HEIGHT);

  if (challenge) {
    displayChallenge.current = challenge;
  }

  useEffect(() => {
    if (challenge) {
      isDismissing.current = false;
      translateY.value = SHEET_HEIGHT;
      setModalVisible(true);
      requestAnimationFrame(() => {
        translateY.value = withSpring(0, { damping: 28, stiffness: 300 });
      });
    } else if (modalVisible) {
      // Animate out, then hide the modal
      isDismissing.current = true;
      translateY.value = withTiming(SHEET_HEIGHT, { duration: 280 }, (finished) => {
        if (finished) {
          runOnJS(setModalVisible)(false);
        }
      });
    }
  }, [challenge, translateY]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleClose = useCallback(() => {
    if (isDismissing.current) return;
    isDismissing.current = true;
    translateY.value = withTiming(SHEET_HEIGHT, { duration: 280 }, (finished) => {
      if (finished) {
        runOnJS(onClose)();
        runOnJS(setModalVisible)(false);
      }
    });
  }, [onClose, translateY]);

  const panGesture = useMemo(
    () =>
      Gesture.Pan()
        .onUpdate((e) => {
          "worklet";
          if (e.translationY > 0) {
            translateY.value = e.translationY;
          }
        })
        .onEnd((e) => {
          "worklet";
          if (e.translationY > CLOSE_THRESHOLD) {
            translateY.value = withTiming(SHEET_HEIGHT, { duration: 250 }, (finished) => {
              if (finished) {
                runOnJS(onClose)();
                runOnJS(setModalVisible)(false);
              }
            });
          } else {
            translateY.value = withSpring(0, { damping: 28, stiffness: 300 });
          }
        }),
    [onClose, translateY]
  );

  const sheetAnimatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: translateY.value }],
  }));

  const backdropAnimatedStyle = useAnimatedStyle(() => ({
    opacity: 0.55 * Math.max(0, 1 - translateY.value / SHEET_HEIGHT),
  }));

  const shown = displayChallenge.current;
  if (!shown) return null;

  const actionState = getChallengeActionState(shown);
  const riskColor = shown.risk_level === "high" ? "#FCA5A5" : "#FCD34D";
  const isGrantMode = shown.approval_mode === "grant";

  return (
    <Modal
      visible={modalVisible}
      transparent
      animationType="none"
      statusBarTranslucent
      onRequestClose={handleClose}
    >
      <GestureHandlerRootView style={sheetStyles.modalRoot}>
        <Animated.View style={[sheetStyles.backdrop, backdropAnimatedStyle]} pointerEvents="auto">
          <Pressable style={StyleSheet.absoluteFill} onPress={handleClose} />
        </Animated.View>

        <Animated.View style={[sheetStyles.sheet, sheetAnimatedStyle]}>
          <GestureDetector gesture={panGesture}>
            <Animated.View style={sheetStyles.handleArea}>
              <View style={sheetStyles.handle} />
            </Animated.View>
          </GestureDetector>

          <View style={sheetStyles.sheetHeader}>
            <Text style={sheetStyles.sheetTitle}>Challenge Detail</Text>
            <Pressable style={sheetStyles.closeBtn} onPress={handleClose}>
              <Text style={sheetStyles.closeBtnText}>✕</Text>
            </Pressable>
          </View>

          <ScrollView style={sheetStyles.sheetBody} contentContainerStyle={sheetStyles.sheetBodyContent}>
            <View style={sheetStyles.detailCard}>
              <Text style={flowStyles.cardTitle}>Request Context</Text>
              <DetailRow label="Action" value={shown.action} />
              <DetailRow label="Resource" value={shown.resource} />
              <DetailRow label="Service" value={shown.title} />
              <DetailRow label="Client" value={shown.request_context.client} />
              <DetailRow label="Risk Level" value={shown.risk_level.toUpperCase()} valueColor={riskColor} />
              <DetailRow label="Status" value={actionState.statusLabel} />
              {isGrantMode && <DetailRow label="Grant Duration" value={grantDurationLabel} />}
              <DetailRow label="Location" value={shown.request_context.location} isLast />
            </View>

            {actionState.reason ? (
              <View style={sheetStyles.stateNotice}>
                <Text style={sheetStyles.stateNoticeText}>{actionState.reason}</Text>
              </View>
            ) : null}

            {actionState.canDecide && onApprove && onDeny && (
              <View style={flowStyles.actionWrap}>
                <PrimaryButton
                  label="Approve"
                  onPress={() => onApprove(shown.id)}
                  disabled={isMutating}
                />
                <PrimaryButton
                  label="Deny"
                  kind="danger"
                  onPress={() => onDeny(shown.id)}
                  disabled={isMutating}
                />
              </View>
            )}
          </ScrollView>
        </Animated.View>
      </GestureHandlerRootView>
    </Modal>
  );
}

export function ActivityScreen() {
  const navigation = useNavigation<Nav>();
  const route = useRoute<RouteProp<RootStackParamList, "Activity">>();
  const queryClient = useQueryClient();
  const { isConnected } = useNetworkStatus();
  const isPolling = usePushPollingActive();
  const [activeSegment, setActiveSegment] = useState<ActivitySegment>("pending");
  const [toast, setToast] = useState<ToastState | null>(null);
  const [mutatingIds, setMutatingIds] = useState<Set<string>>(new Set());
  const [detailChallenge, setDetailChallenge] = useState<ChallengeDetail | null>(null);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2400);
    return () => clearTimeout(t);
  }, [toast]);

  // --- Queries ---
  const pendingQuery = useQuery({
    queryKey: ["challenges", "pending"],
    queryFn: mobileApi.getChallenges,
    refetchInterval: isPolling ? 3000 : false,
  });

  const approvalsQuery = useQuery({
    queryKey: ["approvals"],
    queryFn: mobileApi.getApprovals,
    refetchInterval: isPolling ? 3000 : false,
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

  // --- Deep-link / push-notification: auto-open sheet for a specific challenge ---
  const deepLinkChallengeId = route.params?.challengeId;
  const deepLinkConsumedRef = useRef<string | null>(null);

  useEffect(() => {
    if (!deepLinkChallengeId || deepLinkChallengeId === deepLinkConsumedRef.current) return;
    deepLinkConsumedRef.current = deepLinkChallengeId;
    navigation.setParams({ challengeId: undefined });

    const found = pendingItems.find((c) => c.id === deepLinkChallengeId);
    if (found) {
      setDetailChallenge(found);
      return;
    }

    // Not in local cache yet — fetch directly
    mobileApi.getChallengeById(deepLinkChallengeId).then((challenge) => {
      setDetailChallenge(challenge);
    }).catch(() => {
      setToast({ message: "Challenge not found", kind: "error" });
    });
  }, [deepLinkChallengeId, pendingItems, navigation]);

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
      setDetailChallenge(null);
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
                onPress={() => setDetailChallenge(item)}
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
            renderItem={({ item }) => (
              <HistoryCard
                item={item}
                onPress={() => setDetailChallenge(item)}
              />
            )}
            renderSectionHeader={({ section }) => <HistorySectionHeader title={section.title} />}
            stickySectionHeadersEnabled
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            SectionSeparatorComponent={() => <View style={styles.sectionSep} />}
            refreshControl={
              <RefreshControl refreshing={isRefreshing} onRefresh={handleRefresh} tintColor={mobileTheme.primary} />
            }
          />
        )
      )}

      <ChallengeDetailSheet
        challenge={detailChallenge}
        grantDurationLabel={grantDurationLabel}
        onClose={() => setDetailChallenge(null)}
        onApprove={(id) => decideMutation.mutate({ id, decision: "APPROVE" })}
        onDeny={(id) => decideMutation.mutate({ id, decision: "DENY" })}
        isMutating={mutatingIds.has(detailChallenge?.id ?? "")}
      />
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

const sheetStyles = StyleSheet.create({
  modalRoot: {
    flex: 1,
  },
  backdrop: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "#000",
  },
  sheet: {
    position: "absolute",
    top: SHEET_TOP,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: mobileTheme.bg,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    borderWidth: 1,
    borderBottomWidth: 0,
    borderColor: mobileTheme.border,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: -10 },
    shadowOpacity: 0.4,
    shadowRadius: 40,
    elevation: 24,
    overflow: "hidden",
  },
  handleArea: {
    alignItems: "center",
    paddingTop: 10,
    paddingBottom: 6,
  },
  handle: {
    width: 36,
    height: 4,
    borderRadius: 2,
    backgroundColor: "rgba(255,255,255,0.15)",
  },
  sheetHeader: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: spacing.xxl,
    paddingBottom: spacing.lg,
    borderBottomWidth: 1,
    borderBottomColor: mobileTheme.borderSoft,
  },
  sheetTitle: {
    fontSize: 18,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  closeBtn: {
    width: 30,
    height: 30,
    borderRadius: 15,
    backgroundColor: "rgba(255,255,255,0.06)",
    borderWidth: 1,
    borderColor: mobileTheme.borderSoft,
    alignItems: "center",
    justifyContent: "center",
  },
  closeBtnText: {
    fontSize: 14,
    fontWeight: "600",
    color: mobileTheme.textMuted,
  },
  sheetBody: {
    flex: 1,
    paddingHorizontal: spacing.xxl,
    paddingTop: spacing.xl,
  },
  sheetBodyContent: {
    paddingBottom: spacing.huge,
    gap: spacing.lg,
  },
  stateNotice: {
    borderRadius: radius.sm,
    backgroundColor: "rgba(245,158,11,0.1)",
    borderWidth: 1,
    borderColor: "rgba(245,158,11,0.25)",
    padding: spacing.lg,
  },
  stateNoticeText: {
    fontSize: 13,
    color: mobileTheme.warning,
    lineHeight: 20,
  },
  detailCard: {
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: mobileTheme.cardSoft,
    padding: spacing.lg,
    gap: spacing.md,
  },
});
