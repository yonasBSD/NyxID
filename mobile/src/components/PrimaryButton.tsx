import { useMemo } from "react";
import { Pressable, StyleSheet, Text } from "react-native";
import { useTheme } from "../theme/ThemeContext";
import type { ThemeColors } from "../theme/mobileTheme";
import { radius, spacing, typeScale } from "../theme/designTokens";

type PrimaryButtonProps = {
  label: string;
  onPress: () => void;
  kind?: "primary" | "ghost" | "danger";
  disabled?: boolean;
};

export function PrimaryButton({
  label,
  onPress,
  kind = "primary",
  disabled = false,
}: PrimaryButtonProps) {
  const { colors } = useTheme();
  const styles = useMemo(() => createStyles(colors), [colors]);

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      style={[
        styles.base,
        kind === "ghost" && styles.ghost,
        kind === "danger" && styles.danger,
        disabled && styles.disabled,
      ]}
    >
      <Text style={[styles.label, kind === "ghost" && styles.ghostLabel, kind === "danger" && styles.dangerLabel, disabled && styles.labelDisabled]}>{label}</Text>
    </Pressable>
  );
}

const createStyles = (c: ThemeColors) =>
  StyleSheet.create({
    base: {
      backgroundColor: c.primary,
      borderRadius: radius.md,
      paddingVertical: spacing.lg,
      paddingHorizontal: spacing.xxl,
      alignItems: "center",
      borderWidth: 1,
      borderColor: "transparent",
    },
    ghost: {
      backgroundColor: c.ghostBg,
      borderColor: c.borderSoft,
    },
    danger: {
      backgroundColor: c.dangerSoftBg,
      borderColor: c.danger,
    },
    disabled: {
      opacity: 0.6,
    },
    label: {
      color: c.onPrimary,
      ...typeScale.bodyStrong,
    },
    ghostLabel: {
      color: c.ghostText,
    },
    dangerLabel: {
      color: c.dangerSoft,
    },
    labelDisabled: {
      color: c.textMuted,
    },
  });
