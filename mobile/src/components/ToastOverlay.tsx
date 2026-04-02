import { useCallback, useEffect, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import Animated, {
  useSharedValue,
  useAnimatedStyle,
  withSpring,
  withTiming,
  runOnJS,
} from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { mobileTheme } from "../theme/mobileTheme";
import { radius } from "../theme/designTokens";

export type ToastKind = "success" | "error" | "info";

export type ToastState = {
  message: string;
  kind: ToastKind;
  action?: { label: string; onPress: () => void };
};

type Props = {
  toast: ToastState | null;
  bottom?: number; // kept for backward compat, ignored (top-positioned now)
};

const OFFSCREEN_Y = -120;

const ACCENT: Record<ToastKind, string> = {
  success: "#34d399",
  error: "#f87171",
  info: "#60a5fa",
};

export function ToastOverlay({ toast }: Props) {
  const insets = useSafeAreaInsets();
  const [visibleToast, setVisibleToast] = useState<ToastState | null>(null);
  const translateY = useSharedValue(OFFSCREEN_Y);
  const opacity = useSharedValue(0);

  const clearVisibleToast = useCallback(() => {
    setVisibleToast(null);
  }, []);

  useEffect(() => {
    if (toast) {
      setVisibleToast(toast);
      translateY.value = withSpring(0, { damping: 20, stiffness: 250 });
      opacity.value = withTiming(1, { duration: 150 });
    } else if (visibleToast) {
      translateY.value = withTiming(OFFSCREEN_Y, { duration: 200 });
      opacity.value = withTiming(0, { duration: 200 }, (finished) => {
        if (finished) runOnJS(clearVisibleToast)();
      });
    }
  }, [toast, translateY, opacity, clearVisibleToast]); // eslint-disable-line react-hooks/exhaustive-deps

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: translateY.value }],
    opacity: opacity.value,
  }));

  if (!visibleToast) return null;

  const kind = visibleToast.kind;
  const action = visibleToast.action;

  return (
    <View style={[styles.wrap, { top: insets.top + 14 }]} pointerEvents="box-none">
      <Animated.View style={[styles.toast, animatedStyle]}>
        <View style={[styles.dot, { backgroundColor: ACCENT[kind] }]} />
        <Text style={styles.text} numberOfLines={1}>
          {visibleToast.message}
        </Text>
        {action && (
          <>
            <View style={styles.divider} />
            <Pressable onPress={action.onPress} hitSlop={8}>
              <Text style={styles.actionText}>{action.label}</Text>
            </Pressable>
          </>
        )}
      </Animated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: {
    position: "absolute",
    left: 0,
    right: 0,
    alignItems: "center",
    zIndex: 10000,
  },
  toast: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    borderRadius: radius.pill,
    backgroundColor: mobileTheme.card,
    borderWidth: 1,
    borderColor: "rgba(255,255,255,0.08)",
    paddingHorizontal: 16,
    paddingVertical: 12,
    maxWidth: "85%",
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 12,
    elevation: 8,
  },
  dot: {
    width: 10,
    height: 10,
    borderRadius: 5,
  },
  text: {
    flex: 1,
    fontSize: 14,
    fontWeight: "500",
    lineHeight: 18,
    color: mobileTheme.textPrimary,
  },
  divider: {
    width: 1,
    height: 16,
    backgroundColor: "rgba(255,255,255,0.15)",
  },
  actionText: {
    fontSize: 14,
    fontWeight: "500",
    color: mobileTheme.textPrimary,
  },
});
