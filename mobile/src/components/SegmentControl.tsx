import { useEffect, useMemo, useRef } from "react";
import { Animated, LayoutChangeEvent, Pressable, StyleSheet, Text, View } from "react-native";
import { radius, spacing } from "../theme/designTokens";
import { useTheme } from "../theme/ThemeContext";
import type { ThemeColors } from "../theme/mobileTheme";

type Segment = {
  label: string;
  count?: number;
};

type SegmentControlProps = {
  segments: Segment[];
  activeIndex: number;
  onPress: (index: number) => void;
};

export function SegmentControl({ segments, activeIndex, onPress }: SegmentControlProps) {
  const { colors } = useTheme();
  const styles = useMemo(() => createStyles(colors), [colors]);

  const translateX = useRef(new Animated.Value(0)).current;
  const segmentWidth = useRef(0);

  useEffect(() => {
    Animated.spring(translateX, {
      toValue: activeIndex * segmentWidth.current,
      useNativeDriver: true,
      tension: 300,
      friction: 30,
    }).start();
  }, [activeIndex, translateX]);

  const onContainerLayout = (e: LayoutChangeEvent) => {
    const totalWidth = e.nativeEvent.layout.width - 4; // subtract container padding (2+2)
    segmentWidth.current = totalWidth / segments.length;
    translateX.setValue(activeIndex * segmentWidth.current);
  };

  return (
    <View style={styles.container} onLayout={onContainerLayout}>
      <Animated.View
        style={[
          styles.highlight,
          {
            width: `${100 / segments.length}%` as unknown as number,
            transform: [{ translateX }],
          },
        ]}
      />
      {segments.map((seg, i) => {
        const isActive = i === activeIndex;
        return (
          <Pressable
            key={seg.label}
            style={styles.item}
            onPress={() => onPress(i)}
          >
            <Text style={[styles.label, isActive && styles.labelActive]}>{seg.label}</Text>
            {seg.count !== undefined && seg.count > 0 ? (
              <View style={[styles.countBadge, isActive && styles.countBadgeActive]}>
                <Text style={[styles.countText, isActive && styles.countTextActive]}>
                  {seg.count}
                </Text>
              </View>
            ) : null}
          </Pressable>
        );
      })}
    </View>
  );
}

const createStyles = (c: ThemeColors) =>
  StyleSheet.create({
    container: {
      flexDirection: "row",
      backgroundColor: c.primaryGlow,
      borderWidth: 1,
      borderColor: c.border,
      borderRadius: radius.sm,
      padding: 2,
      marginBottom: spacing.xxl,
      position: "relative",
    },
    highlight: {
      position: "absolute",
      top: 2,
      left: 2,
      bottom: 2,
      borderRadius: 6,
      backgroundColor: c.primary,
    },
    item: {
      flex: 1,
      flexDirection: "row",
      alignItems: "center",
      justifyContent: "center",
      gap: 4,
      height: 41,
      paddingHorizontal: 4,
      borderRadius: 6,
      zIndex: 1,
    },
    label: {
      fontSize: 12,
      fontWeight: "600",
      color: c.textSecondary,
    },
    labelActive: {
      color: c.onPrimary,
    },
    countBadge: {
      minWidth: 16,
      height: 16,
      borderRadius: 8,
      backgroundColor: c.primaryGlow,
      alignItems: "center",
      justifyContent: "center",
      paddingHorizontal: 4,
    },
    countBadgeActive: {
      backgroundColor: "rgba(255,255,255,0.2)",
    },
    countText: {
      fontSize: 9,
      fontWeight: "700",
      color: c.primary,
    },
    countTextActive: {
      color: c.onPrimary,
    },
  });
