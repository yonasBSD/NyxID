import { Pressable, StyleSheet, Text, View } from "react-native";
import { radius, spacing } from "../theme/designTokens";
import { mobileTheme } from "../theme/mobileTheme";
import { StatusBadge } from "./StatusBadge";
import type { ChallengeDetail, ChallengeStatus } from "../lib/api/types";

type HistoryCardProps = {
  item: ChallengeDetail;
  onPress?: () => void;
};

function decisionVariant(status: ChallengeStatus): "decisionApproved" | "decisionDenied" | "decisionExpired" {
  if (status === "APPROVED") return "decisionApproved";
  if (status === "DENIED") return "decisionDenied";
  return "decisionExpired";
}

function decisionLabel(status: ChallengeStatus): string {
  if (status === "APPROVED") return "APPROVED";
  if (status === "DENIED") return "DENIED";
  return "EXPIRED";
}

function decisionDescription(status: ChallengeStatus): string {
  if (status === "APPROVED") return "You approved this";
  if (status === "DENIED") return "You denied this";
  return "This request expired";
}

function formatTime(dateStr: string): string {
  const d = new Date(dateStr);
  return d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
}

export function HistoryCard({ item, onPress }: HistoryCardProps) {
  const modeLabel = item.approval_mode === "grant" ? "Grant" : "Per-request";

  return (
    <Pressable style={styles.card} onPress={onPress}>
      <View style={styles.header}>
        <Text style={styles.title} numberOfLines={1}>
          {item.action} {item.resource}
        </Text>
        <StatusBadge variant={decisionVariant(item.status)} label={decisionLabel(item.status)} />
      </View>
      <Text style={styles.secondary} numberOfLines={1}>
        {item.title} · {modeLabel}
      </Text>
      <Text style={styles.meta}>
        {formatTime(item.created_at)} · {decisionDescription(item.status)}
      </Text>
    </Pressable>
  );
}

export function HistorySectionHeader({ title }: { title: string }) {
  return <Text style={styles.sectionHeader}>{title}</Text>;
}

const styles = StyleSheet.create({
  card: {
    borderRadius: radius.md,
    backgroundColor: mobileTheme.cardSoft,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    padding: spacing.lg,
    gap: 4,
    opacity: 0.8,
  },
  header: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    gap: spacing.sm,
  },
  title: {
    flex: 1,
    fontSize: 14,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
  },
  secondary: {
    fontSize: 13,
    color: mobileTheme.textSecondary,
  },
  meta: {
    fontSize: 12,
    color: mobileTheme.textMuted,
  },
  sectionHeader: {
    fontSize: 11,
    fontWeight: "600",
    color: mobileTheme.textMuted,
    textTransform: "uppercase",
    letterSpacing: 0.4,
    marginBottom: spacing.sm,
    marginTop: spacing.xs,
  },
});
