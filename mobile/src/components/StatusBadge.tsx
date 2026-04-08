import { useMemo } from "react";
import { StyleSheet, Text, View } from "react-native";
import { spacing } from "../theme/designTokens";
import { useTheme } from "../theme/ThemeContext";
import type { ThemeColors } from "../theme/mobileTheme";

type BadgeVariant =
  | "riskHigh"
  | "riskMedium"
  | "riskLow"
  | "expiryUrgent"
  | "expiryNormal"
  | "decisionApproved"
  | "decisionDenied"
  | "decisionExpired"
  | "modeChip";

type StatusBadgeProps = {
  variant: BadgeVariant;
  label: string;
};

function getVariantStyles(c: ThemeColors): Record<BadgeVariant, { bg: string; text: string; border: string }> {
  return {
    riskHigh: c.riskHigh,
    riskMedium: c.riskMedium,
    riskLow: c.riskLow,
    expiryUrgent: { bg: c.dangerSoftBg, text: c.danger, border: "rgba(239,68,68,0.2)" },
    expiryNormal: { bg: "rgba(52,211,153,0.1)", text: c.success, border: "rgba(52,211,153,0.2)" },
    decisionApproved: { bg: "rgba(52,211,153,0.12)", text: c.success, border: "rgba(52,211,153,0.2)" },
    decisionDenied: { bg: "rgba(239,68,68,0.12)", text: c.danger, border: "rgba(239,68,68,0.2)" },
    decisionExpired: { bg: "rgba(143,136,171,0.12)", text: c.textMuted, border: "rgba(143,136,171,0.2)" },
    modeChip: { bg: c.primaryGlow, text: c.primary, border: "rgba(139,92,246,0.2)" },
  };
}

export function StatusBadge({ variant, label }: StatusBadgeProps) {
  const { colors } = useTheme();
  const styles = useMemo(() => createStyles(colors), [colors]);
  const variantStyles = useMemo(() => getVariantStyles(colors), [colors]);

  const v = variantStyles[variant];
  return (
    <View style={[styles.badge, { backgroundColor: v.bg, borderColor: v.border }]}>
      <Text style={[styles.text, { color: v.text }]}>{label}</Text>
    </View>
  );
}

const createStyles = (c: ThemeColors) =>
  StyleSheet.create({
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
