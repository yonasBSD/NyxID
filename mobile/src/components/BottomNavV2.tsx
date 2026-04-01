import { useEffect, useRef } from "react";
import { Animated, LayoutChangeEvent, Pressable, StyleSheet, Text, View } from "react-native";
import Svg, { Path, Circle } from "react-native-svg";
import { mobileTheme } from "../theme/mobileTheme";
import { radius, spacing } from "../theme/designTokens";

export type BottomNavV2Tab = "activity" | "account";

type BottomNavV2Props = {
  active: BottomNavV2Tab;
  onTabPress: (tab: BottomNavV2Tab) => void;
};

function ShieldCheckIcon({ color }: { color: string }) {
  return (
    <Svg width={18} height={18} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
      <Path d="M9 12l2 2 4-4" />
    </Svg>
  );
}

function PersonIcon({ color }: { color: string }) {
  return (
    <Svg width={18} height={18} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
      <Circle cx={12} cy={7} r={4} />
    </Svg>
  );
}

const PADDING = spacing.xs + spacing.xxs; // 6
const GAP = spacing.xs + spacing.xxs; // 6
const FAB_WIDTH = 56;

export function BottomNavV2({ active, onTabPress }: BottomNavV2Props) {
  const translateX = useRef(new Animated.Value(0)).current;
  const tabWidth = useRef(0);
  const containerWidth = useRef(0);

  const activeIndex = active === "activity" ? 0 : 1;

  const computeOffset = (index: number) => {
    if (index === 0) return 0;
    // second tab starts after: tabWidth + GAP + FAB_WIDTH + GAP
    return tabWidth.current + GAP + FAB_WIDTH + GAP;
  };

  useEffect(() => {
    Animated.spring(translateX, {
      toValue: computeOffset(activeIndex),
      useNativeDriver: true,
      tension: 300,
      friction: 30,
    }).start();
  }, [activeIndex, translateX]);

  const onLayout = (e: LayoutChangeEvent) => {
    const w = e.nativeEvent.layout.width;
    containerWidth.current = w;
    // inner width = total - 2*padding
    const inner = w - 2 * PADDING;
    // inner = tabWidth + GAP + FAB_WIDTH + GAP + tabWidth
    // tabWidth = (inner - FAB_WIDTH - 2*GAP) / 2
    tabWidth.current = (inner - FAB_WIDTH - 2 * GAP) / 2;
    translateX.setValue(computeOffset(activeIndex));
  };

  return (
    <View style={styles.wrap} onLayout={onLayout}>
      <Animated.View
        style={[
          styles.activeHighlight,
          {
            width: tabWidth.current || "35%",
            transform: [{ translateX }],
          },
        ]}
      />
      <Pressable
        style={styles.item}
        onPress={() => onTabPress("activity")}
      >
        <ShieldCheckIcon color={active === "activity" ? mobileTheme.textPrimary : mobileTheme.textMuted} />
        <Text style={[styles.text, active === "activity" && styles.textActive]}>Activity</Text>
      </Pressable>

      <View style={styles.fabSpacer}>
        <Text style={styles.fabLabel}>Ask Nyx</Text>
      </View>

      <Pressable
        style={styles.item}
        onPress={() => onTabPress("account")}
      >
        <PersonIcon color={active === "account" ? mobileTheme.textPrimary : mobileTheme.textMuted} />
        <Text style={[styles.text, active === "account" && styles.textActive]}>Account</Text>
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: {
    backgroundColor: mobileTheme.card,
    borderRadius: radius.xl,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    padding: PADDING,
    flexDirection: "row",
    gap: GAP,
    position: "relative",
  },
  activeHighlight: {
    position: "absolute",
    top: PADDING,
    left: PADDING,
    bottom: PADDING,
    borderRadius: radius.md,
    backgroundColor: mobileTheme.navActive,
  },
  item: {
    flex: 1,
    paddingVertical: spacing.sm,
    borderRadius: radius.md,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "transparent",
    gap: 3,
    zIndex: 1,
  },
  fabSpacer: {
    width: FAB_WIDTH,
    alignItems: "center",
    justifyContent: "flex-end",
    paddingBottom: 2,
    zIndex: 1,
  },
  text: {
    color: mobileTheme.textMuted,
    fontSize: 10,
    fontWeight: "600",
    letterSpacing: 0.02,
  },
  textActive: {
    color: mobileTheme.textPrimary,
  },
  fabLabel: {
    color: mobileTheme.primary,
    fontSize: 10,
    fontWeight: "600",
    letterSpacing: 0.02,
  },
});
