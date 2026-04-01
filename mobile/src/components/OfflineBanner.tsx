import { Pressable, StyleSheet, Text, View } from "react-native";
import { Feather } from "@expo/vector-icons";
import { radius, spacing } from "../theme/designTokens";
import { mobileTheme } from "../theme/mobileTheme";

type OfflineBannerProps = {
  subtitle?: string;
  onRetry?: () => void;
};

export function OfflineBanner({ subtitle = "Showing cached data", onRetry }: OfflineBannerProps) {
  return (
    <View style={styles.banner}>
      <Feather name="wifi-off" size={16} color="#FCA5A5" />
      <View style={styles.textWrap}>
        <Text style={styles.title}>No connection</Text>
        <Text style={styles.subtitle}>{subtitle}</Text>
      </View>
      {onRetry ? (
        <Pressable style={styles.retryBtn} onPress={onRetry}>
          <Text style={styles.retryText}>Retry</Text>
        </Pressable>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.sm,
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    borderRadius: radius.sm,
    backgroundColor: "rgba(239,68,68,0.1)",
    borderWidth: 1,
    borderColor: "rgba(239,68,68,0.25)",
    marginBottom: spacing.lg,
  },
  textWrap: {
    flex: 1,
  },
  title: {
    fontSize: 12,
    fontWeight: "600",
    color: "#FCA5A5",
  },
  subtitle: {
    fontSize: 10,
    color: mobileTheme.textMuted,
    marginTop: 1,
  },
  retryBtn: {
    paddingHorizontal: spacing.md,
    paddingVertical: spacing.xs,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: "rgba(239,68,68,0.3)",
    backgroundColor: "rgba(239,68,68,0.08)",
  },
  retryText: {
    fontSize: 10,
    fontWeight: "700",
    color: "#FCA5A5",
  },
});
