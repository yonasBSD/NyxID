import { StyleSheet, Text, View } from "react-native";
import { radius, spacing } from "../theme/designTokens";
import { mobileTheme } from "../theme/mobileTheme";

type EmptyStatePreset = "pendingEmpty" | "activeEmpty" | "historyEmpty";

type EmptyStateProps = {
  preset: EmptyStatePreset;
};

const presets: Record<EmptyStatePreset, {
  icon: string;
  iconColor: string;
  iconBorderColor: string;
  iconBg: string;
  title: string;
  subtitle: string;
}> = {
  pendingEmpty: {
    icon: "!",
    iconColor: mobileTheme.warning,
    iconBorderColor: "#F59E0B70",
    iconBg: "#78350F30",
    title: "No pending challenges",
    subtitle: "New high-risk requests will appear here.",
  },
  activeEmpty: {
    icon: "\u2713",
    iconColor: mobileTheme.success,
    iconBorderColor: "#34D39970",
    iconBg: "#064E3B55",
    title: "No active approvals",
    subtitle: "Approvals appear here after challenge decisions.",
  },
  historyEmpty: {
    icon: "\u21BA",
    iconColor: mobileTheme.info,
    iconBorderColor: "#60A5FA70",
    iconBg: "#1E3A5F55",
    title: "No decision history",
    subtitle: "Past approvals, denials, and expirations appear here.",
  },
};

export function EmptyState({ preset }: EmptyStateProps) {
  const p = presets[preset];
  return (
    <View style={styles.container}>
      <View style={[styles.iconWrap, { borderColor: p.iconBorderColor, backgroundColor: p.iconBg }]}>
        <Text style={[styles.icon, { color: p.iconColor }]}>{p.icon}</Text>
      </View>
      <Text style={styles.title}>{p.title}</Text>
      <Text style={styles.subtitle}>{p.subtitle}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    borderRadius: radius.md,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: mobileTheme.cardSoft,
    padding: spacing.xxl,
    alignItems: "center",
    gap: spacing.sm,
  },
  iconWrap: {
    width: 34,
    height: 34,
    borderRadius: 17,
    borderWidth: 1,
    alignItems: "center",
    justifyContent: "center",
  },
  icon: {
    fontSize: 14,
    fontWeight: "700",
  },
  title: {
    fontSize: 14,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    textAlign: "center",
  },
  subtitle: {
    fontSize: 12,
    color: mobileTheme.textMuted,
    lineHeight: 18,
    textAlign: "center",
  },
});
