export type ThemeColors = {
  bg: string;
  card: string;
  cardSoft: string;
  border: string;
  borderSoft: string;
  textPrimary: string;
  textSecondary: string;
  textMuted: string;
  success: string;
  successSoft: string;
  info: string;
  infoSoft: string;
  warning: string;
  warningSoft: string;
  danger: string;
  dangerSoft: string;
  dangerSoftBg: string;
  primary: string;
  primaryDim: string;
  primaryGlow: string;
  onPrimary: string;
  ghostBg: string;
  ghostText: string;
  fabBg: string;
  fabBorder: string;
  overlayBg: string;
  handleBg: string;
  navActive: string;
  shadowColor: string;
  riskHigh: { bg: string; text: string; border: string };
  riskMedium: { bg: string; text: string; border: string };
  riskLow: { bg: string; text: string; border: string };
};

export const darkColors: ThemeColors = {
  bg: "#10101A",
  card: "#171726",
  cardSoft: "#121222",
  border: "#2A2739",
  borderSoft: "#1E1B2E",
  textPrimary: "#F0EEFF",
  textSecondary: "#A9A3BE",
  textMuted: "#8F88AB",
  success: "#34D399",
  successSoft: "#34D39940",
  info: "#60A5FA",
  infoSoft: "#60A5FA40",
  warning: "#F59E0B",
  warningSoft: "#F59E0B40",
  danger: "#EF4444",
  dangerSoft: "#FCA5A5",
  dangerSoftBg: "rgba(239,68,68,0.1)",
  primary: "#8B5CF6",
  primaryDim: "#6D42D9",
  primaryGlow: "rgba(139, 92, 246, 0.12)",
  onPrimary: "#FFFFFF",
  ghostBg: "rgba(139,92,246,0.06)",
  ghostText: "#F8F7FF",
  fabBg: "rgba(16,16,26,1)",
  fabBorder: "rgba(139,92,246,0.35)",
  overlayBg: "#000",
  handleBg: "rgba(255,255,255,0.15)",
  navActive: "#232136",
  shadowColor: "#000",
  riskHigh: { bg: "#7F1D1D30", text: "#FCA5A5", border: "#F8717140" },
  riskMedium: { bg: "#78350F30", text: "#FCD34D", border: "#F59E0B40" },
  riskLow: { bg: "rgba(52,211,153,0.12)", text: "#6EE7B7", border: "rgba(52,211,153,0.2)" },
};

export const lightColors: ThemeColors = {
  bg: "#F8F7FC",
  card: "#FFFFFF",
  cardSoft: "#F3F2F8",
  border: "#DDD9E8",
  borderSoft: "#ECEAF3",
  textPrimary: "#1A1730",
  textSecondary: "#5E5875",
  textMuted: "#8F88AB",
  success: "#10B981",
  successSoft: "rgba(16,185,129,0.2)",
  info: "#3B82F6",
  infoSoft: "rgba(59,130,246,0.2)",
  warning: "#D97706",
  warningSoft: "rgba(217,119,6,0.2)",
  danger: "#DC2626",
  dangerSoft: "#DC2626",
  dangerSoftBg: "rgba(220,38,38,0.06)",
  primary: "#7C3AED",
  primaryDim: "#6D28D9",
  primaryGlow: "rgba(124, 58, 237, 0.06)",
  onPrimary: "#FFFFFF",
  ghostBg: "rgba(124,58,237,0.04)",
  ghostText: "#7C3AED",
  fabBg: "rgba(80,78,93,1)",
  fabBorder: "rgba(139,92,246,0.35)",
  overlayBg: "rgba(0,0,0,0.3)",
  handleBg: "rgba(0,0,0,0.12)",
  navActive: "#EDEBF7",
  shadowColor: "rgba(30,20,60,0.15)",
  riskHigh: { bg: "rgba(220,38,38,0.08)", text: "#DC2626", border: "rgba(220,38,38,0.2)" },
  riskMedium: { bg: "rgba(217,119,6,0.08)", text: "#D97706", border: "rgba(217,119,6,0.2)" },
  riskLow: { bg: "rgba(16,185,129,0.08)", text: "#10B981", border: "rgba(16,185,129,0.2)" },
};

/** @deprecated Use `useTheme()` from ThemeContext instead. */
export const mobileTheme = darkColors;
