import { useCallback, useEffect, useState } from "react";
import { StyleSheet, Text, View } from "react-native";
import Animated, {
  useSharedValue,
  useAnimatedStyle,
  withSpring,
  withTiming,
  runOnJS,
} from "react-native-reanimated";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import Svg, { Path, Circle } from "react-native-svg";
import { mobileTheme } from "../theme/mobileTheme";

export type ToastKind = "success" | "error" | "info";

export type ToastState = {
  message: string;
  kind: ToastKind;
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

function SuccessIcon() {
  return (
    <Svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke={ACCENT.success} strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M20 6L9 17l-5-5" />
    </Svg>
  );
}

function ErrorIcon() {
  return (
    <Svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke={ACCENT.error} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Circle cx={12} cy={12} r={10} />
      <Path d="M15 9l-6 6" />
      <Path d="M9 9l6 6" />
    </Svg>
  );
}

function InfoIcon() {
  return (
    <Svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke={ACCENT.info} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Circle cx={12} cy={12} r={10} />
      <Path d="M12 16v-4" />
      <Path d="M12 8h.01" />
    </Svg>
  );
}

const ICONS: Record<ToastKind, () => React.JSX.Element> = {
  success: SuccessIcon,
  error: ErrorIcon,
  info: InfoIcon,
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
  const Icon = ICONS[kind];

  return (
    <View style={[styles.wrap, { top: insets.top + 8 }]} pointerEvents="none">
      <Animated.View
        style={[
          styles.toast,
          { borderLeftColor: ACCENT[kind] },
          animatedStyle,
        ]}
      >
        <Icon />
        <Text style={styles.text}>{visibleToast.message}</Text>
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
    paddingHorizontal: 20,
    zIndex: 10000,
  },
  toast: {
    width: "100%",
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    borderRadius: 10,
    backgroundColor: mobileTheme.card,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    borderLeftWidth: 3,
    paddingHorizontal: 14,
    paddingVertical: 12,
  },
  text: {
    flex: 1,
    fontSize: 13,
    fontWeight: "500",
    lineHeight: 18,
    color: mobileTheme.textPrimary,
  },
});
