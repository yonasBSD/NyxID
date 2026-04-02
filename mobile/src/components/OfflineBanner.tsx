import { useMemo } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { Feather } from "@expo/vector-icons";
import { radius, spacing } from "../theme/designTokens";
import { useTheme } from "../theme/ThemeContext";
import type { ThemeColors } from "../theme/mobileTheme";

type OfflineBannerProps = {
  subtitle?: string;
  onRetry?: () => void;
};

export function OfflineBanner({ subtitle = "Showing cached data", onRetry }: OfflineBannerProps) {
  const { colors } = useTheme();
  const styles = useMemo(() => createStyles(colors), [colors]);

  return (
    <View style={styles.banner}>
      <Feather name="wifi-off" size={16} color={colors.dangerSoft} />
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

const createStyles = (c: ThemeColors) =>
  StyleSheet.create({
    banner: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.sm,
      paddingHorizontal: spacing.lg,
      paddingVertical: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: c.dangerSoftBg,
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
      color: c.dangerSoft,
    },
    subtitle: {
      fontSize: 10,
      color: c.textMuted,
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
      color: c.dangerSoft,
    },
  });
