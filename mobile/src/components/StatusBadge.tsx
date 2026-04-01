import { StyleSheet, Text, View } from "react-native";
import { radius, spacing, typeScale } from "../theme/designTokens";
import { mobileTheme } from "../theme/mobileTheme";

type BadgeVariant =
  | "riskHigh"
  | "riskMedium"
  | "expiryUrgent"
  | "expiryNormal"
  | "decisionApproved"
  | "decisionDenied"
  | "decisionExpired";

type StatusBadgeProps = {
  variant: BadgeVariant;
  label: string;
};

const variantStyles: Record<BadgeVariant, { bg: string; text: string; border: string }> = {
  riskHigh: { bg: "#7F1D1D30", text: "#FCA5A5", border: "#F8717140" },
  riskMedium: { bg: "#78350F30", text: "#FCD34D", border: "#F59E0B40" },
  expiryUrgent: { bg: "rgba(239,68,68,0.12)", text: mobileTheme.danger, border: "rgba(239,68,68,0.2)" },
  expiryNormal: { bg: "rgba(52,211,153,0.1)", text: mobileTheme.success, border: "rgba(52,211,153,0.2)" },
  decisionApproved: { bg: "rgba(52,211,153,0.12)", text: mobileTheme.success, border: "rgba(52,211,153,0.2)" },
  decisionDenied: { bg: "rgba(239,68,68,0.12)", text: mobileTheme.danger, border: "rgba(239,68,68,0.2)" },
  decisionExpired: { bg: "rgba(143,136,171,0.12)", text: mobileTheme.textMuted, border: "rgba(143,136,171,0.2)" },
};

export function StatusBadge({ variant, label }: StatusBadgeProps) {
  const v = variantStyles[variant];
  return (
    <View style={[styles.badge, { backgroundColor: v.bg, borderColor: v.border }]}>
      <Text style={[styles.text, { color: v.text }]}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  badge: {
    paddingHorizontal: spacing.sm,
    paddingVertical: spacing.xxs,
    borderRadius: 6,
    borderWidth: 1,
    alignSelf: "flex-start",
  },
  text: {
    fontSize: 10,
    fontWeight: "700",
    letterSpacing: 0.3,
  },
});
