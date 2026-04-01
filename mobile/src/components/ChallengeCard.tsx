import { Pressable, StyleSheet, Text, View } from "react-native";
import { radius, spacing, typeScale } from "../theme/designTokens";
import { mobileTheme } from "../theme/mobileTheme";
import { StatusBadge } from "./StatusBadge";
import type { ChallengeDetail } from "../lib/api/types";

type ChallengeCardProps = {
  challenge: ChallengeDetail;
  grantDurationLabel?: string;
  isMutating?: boolean;
  onPress?: () => void;
  onApprove?: () => void;
  onDeny?: () => void;
};

function formatTimeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function ChallengeCard({
  challenge,
  grantDurationLabel = "30 days",
  isMutating = false,
  onPress,
  onApprove,
  onDeny,
}: ChallengeCardProps) {
  const riskVariant = challenge.risk_level === "high" ? "riskHigh" : "riskMedium";
  const modeLabel = challenge.approval_mode === "grant" ? "Grant mode" : "Per-request";
  const durationLabel = challenge.approval_mode === "grant" ? ` · ${grantDurationLabel}` : "";
  const timeAgo = formatTimeAgo(challenge.created_at);

  return (
    <Pressable style={styles.card} onPress={onPress} disabled={isMutating}>
      <View style={styles.header}>
        <Text style={styles.title} numberOfLines={1}>
          {challenge.action} {challenge.resource}
        </Text>
        <StatusBadge
          variant={riskVariant}
          label={challenge.risk_level === "high" ? "HIGH" : "MED"}
        />
      </View>
      <Text style={styles.resource} numberOfLines={1}>
        {challenge.title}
      </Text>
      <Text style={styles.meta}>
        {modeLabel}{durationLabel} · {timeAgo}
      </Text>
      <View style={styles.actions}>
        <Pressable
          style={[styles.approveBtn, isMutating && styles.btnDisabled]}
          onPress={onApprove}
          disabled={isMutating}
        >
          <Text style={styles.approveBtnText}>Approve</Text>
        </Pressable>
        <Pressable
          style={[styles.denyBtn, isMutating && styles.btnDisabled]}
          onPress={onDeny}
          disabled={isMutating}
        >
          <Text style={styles.denyBtnText}>Deny</Text>
        </Pressable>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  card: {
    borderRadius: radius.md,
    backgroundColor: mobileTheme.cardSoft,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    padding: spacing.lg,
    gap: 6,
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
  resource: {
    fontSize: 13,
    color: mobileTheme.textSecondary,
  },
  meta: {
    fontSize: 12,
    color: mobileTheme.textMuted,
  },
  actions: {
    flexDirection: "row",
    gap: 6,
    marginTop: spacing.xs,
  },
  approveBtn: {
    flex: 1,
    paddingVertical: 8,
    borderRadius: radius.sm,
    backgroundColor: mobileTheme.primary,
    alignItems: "center",
  },
  approveBtnText: {
    fontSize: 13,
    fontWeight: "700",
    color: "#FFFFFF",
  },
  denyBtn: {
    flex: 1,
    paddingVertical: 8,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: "rgba(239,68,68,0.3)",
    backgroundColor: "transparent",
    alignItems: "center",
  },
  denyBtnText: {
    fontSize: 13,
    fontWeight: "700",
    color: mobileTheme.danger,
  },
  btnDisabled: {
    opacity: 0.5,
  },
});
