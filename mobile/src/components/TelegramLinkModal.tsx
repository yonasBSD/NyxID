import { useCallback, useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  Linking,
  Modal,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import * as Clipboard from "expo-clipboard";
import Svg, { Path } from "react-native-svg";
import { mobileTheme } from "../theme/mobileTheme";
import { radius, spacing } from "../theme/designTokens";
import { mobileApi } from "../lib/api/mobileApi";
import type { TelegramLinkInfo } from "../lib/api/types";

type TelegramLinkModalProps = {
  visible: boolean;
  onDismiss: () => void;
  onConnected: () => void;
};

const POLL_INTERVAL_MS = 3000;
const POLL_TIMEOUT_MS = 60000;

function TelegramIcon() {
  return (
    <Svg width={18} height={18} viewBox="0 0 24 24" fill="#FFFFFF">
      <Path d="M11.944 0A12 12 0 1 0 24 12.056A12.014 12.014 0 0 0 11.944 0Zm5.654 8.22l-1.69 7.955c-.128.574-.462.713-.937.444l-2.586-1.906l-1.248 1.2a.65.65 0 0 1-.52.254l.186-2.632l4.789-4.326c.208-.186-.045-.29-.324-.103L9.3 13.264l-2.548-.795c-.554-.174-.565-.554.116-.82l9.952-3.836c.462-.166.866.113.716.82Z" />
    </Svg>
  );
}

function CopyIcon() {
  return (
    <Svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke={mobileTheme.textSecondary} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
      <Path d="M15 2H9a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1Z" />
    </Svg>
  );
}

function CheckIcon() {
  return (
    <Svg width={18} height={18} viewBox="0 0 24 24" fill="none" stroke="#34D399" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M20 6L9 17l-5-5" />
    </Svg>
  );
}

export function TelegramLinkModal({ visible, onDismiss, onConnected }: TelegramLinkModalProps) {
  const [linkInfo, setLinkInfo] = useState<TelegramLinkInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [connected, setConnected] = useState(false);
  const [expired, setExpired] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cleanup = useCallback(() => {
    if (pollRef.current) clearInterval(pollRef.current);
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    pollRef.current = null;
    timeoutRef.current = null;
  }, []);

  const reset = useCallback(() => {
    cleanup();
    setLinkInfo(null);
    setIsLoading(false);
    setError(null);
    setCopied(false);
    setConnected(false);
    setExpired(false);
  }, [cleanup]);

  const fetchLink = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    setExpired(false);
    setCopied(false);
    setConnected(false);
    try {
      const info = await mobileApi.telegramLink();
      setLinkInfo(info);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to generate link code");
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Fetch link code when modal opens
  useEffect(() => {
    if (visible) {
      reset();
      void fetchLink();
    } else {
      reset();
    }
  }, [visible, fetchLink, reset]);

  // Start polling + timeout when we have a link code
  useEffect(() => {
    if (!visible || !linkInfo || connected || expired) return;

    pollRef.current = setInterval(async () => {
      try {
        const settings = await mobileApi.getNotificationSettings();
        if (settings.telegram_connected) {
          cleanup();
          setConnected(true);
        }
      } catch {
        // Ignore poll errors
      }
    }, POLL_INTERVAL_MS);

    timeoutRef.current = setTimeout(() => {
      cleanup();
      setExpired(true);
    }, POLL_TIMEOUT_MS);

    return cleanup;
  }, [visible, linkInfo, connected, expired, cleanup]);

  // Auto-dismiss after connection
  useEffect(() => {
    if (connected) {
      const timer = setTimeout(() => onConnected(), 1200);
      return () => clearTimeout(timer);
    }
  }, [connected, onConnected]);

  const handleOpenTelegram = useCallback(async () => {
    if (!linkInfo) return;
    const tgUrl = `tg://resolve?domain=${linkInfo.bot_username}&start=${linkInfo.link_code}`;
    const webUrl = `https://t.me/${linkInfo.bot_username}?start=${linkInfo.link_code}`;

    try {
      const canOpen = await Linking.canOpenURL(tgUrl);
      await Linking.openURL(canOpen ? tgUrl : webUrl);
    } catch {
      try {
        await Linking.openURL(webUrl);
      } catch {
        setError("Could not open Telegram");
      }
    }
  }, [linkInfo]);

  const handleCopy = useCallback(async () => {
    if (!linkInfo) return;
    await Clipboard.setStringAsync(`/start ${linkInfo.link_code}`);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [linkInfo]);

  return (
    <Modal visible={visible} transparent animationType="fade" statusBarTranslucent onRequestClose={onDismiss}>
      <View style={styles.backdrop}>
        <View style={styles.card}>
          {/* Header */}
          <Text style={styles.title}>Connect Telegram</Text>
          <Text style={styles.subtitle}>
            Receive approval notifications directly in Telegram.
          </Text>

          {/* Loading state */}
          {isLoading && (
            <View style={styles.centerWrap}>
              <ActivityIndicator size="small" color={mobileTheme.primary} />
            </View>
          )}

          {/* Error state */}
          {error && (
            <View style={styles.centerWrap}>
              <Text style={styles.errorText}>{error}</Text>
              <Pressable style={styles.retryBtn} onPress={() => void fetchLink()}>
                <Text style={styles.retryText}>Retry</Text>
              </Pressable>
            </View>
          )}

          {/* Connected state */}
          {connected && (
            <View style={styles.centerWrap}>
              <CheckIcon />
              <Text style={styles.successText}>Telegram connected!</Text>
            </View>
          )}

          {/* Expired state */}
          {expired && !connected && (
            <View style={styles.centerWrap}>
              <Text style={styles.errorText}>Link code expired</Text>
              <Pressable style={styles.retryBtn} onPress={() => void fetchLink()}>
                <Text style={styles.retryText}>Generate New Code</Text>
              </Pressable>
            </View>
          )}

          {/* Active link state */}
          {linkInfo && !isLoading && !error && !connected && !expired && (
            <>
              {/* Open in Telegram button */}
              <Pressable style={styles.telegramBtn} onPress={handleOpenTelegram}>
                <TelegramIcon />
                <Text style={styles.telegramBtnText}>Open in Telegram</Text>
              </Pressable>

              {/* Divider */}
              <View style={styles.divider}>
                <View style={styles.dividerLine} />
                <Text style={styles.dividerText}>or copy the code</Text>
                <View style={styles.dividerLine} />
              </View>

              {/* Code display + copy */}
              <View style={styles.codeRow}>
                <Text style={styles.codeText} selectable>/start {linkInfo.link_code}</Text>
                <Pressable style={styles.copyBtn} onPress={() => void handleCopy()}>
                  {copied ? <CheckIcon /> : <CopyIcon />}
                </Pressable>
              </View>

              <Text style={styles.instructionText}>
                Send this to @{linkInfo.bot_username} on Telegram
              </Text>

              {/* Polling indicator */}
              <View style={styles.waitingRow}>
                <ActivityIndicator size="small" color={mobileTheme.textMuted} />
                <Text style={styles.waitingText}>Waiting for connection...</Text>
              </View>
            </>
          )}

          {/* Cancel button */}
          {!connected && (
            <Pressable style={styles.cancelBtn} onPress={onDismiss}>
              <Text style={styles.cancelText}>Cancel</Text>
            </Pressable>
          )}
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    flex: 1,
    backgroundColor: "rgba(0,0,0,0.65)",
    justifyContent: "center",
    alignItems: "center",
    paddingHorizontal: 28,
  },
  card: {
    width: "100%",
    maxWidth: 320,
    backgroundColor: mobileTheme.card,
    borderRadius: radius.lg,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    padding: spacing.xxl,
    gap: spacing.lg,
  },
  title: {
    fontSize: 18,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
    textAlign: "center",
  },
  subtitle: {
    fontSize: 13,
    color: mobileTheme.textSecondary,
    textAlign: "center",
    lineHeight: 19,
  },
  centerWrap: {
    alignItems: "center",
    gap: spacing.sm,
    paddingVertical: spacing.md,
  },
  errorText: {
    fontSize: 13,
    color: "#FCA5A5",
    textAlign: "center",
  },
  successText: {
    fontSize: 14,
    fontWeight: "600",
    color: "#34D399",
    textAlign: "center",
  },
  retryBtn: {
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.sm,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: mobileTheme.border,
  },
  retryText: {
    fontSize: 12,
    fontWeight: "600",
    color: mobileTheme.textSecondary,
  },
  telegramBtn: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: spacing.sm,
    paddingVertical: 12,
    borderRadius: radius.sm,
    backgroundColor: "#229ED9",
  },
  telegramBtnText: {
    fontSize: 14,
    fontWeight: "700",
    color: "#FFFFFF",
  },
  divider: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.sm,
  },
  dividerLine: {
    flex: 1,
    height: 1,
    backgroundColor: mobileTheme.borderSoft,
  },
  dividerText: {
    fontSize: 11,
    color: mobileTheme.textMuted,
  },
  codeRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.sm,
    paddingVertical: 10,
    paddingHorizontal: 14,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: mobileTheme.bg,
  },
  codeText: {
    flex: 1,
    fontSize: 13,
    fontWeight: "600",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  copyBtn: {
    padding: spacing.xs,
  },
  instructionText: {
    fontSize: 11,
    color: mobileTheme.textMuted,
    textAlign: "center",
  },
  waitingRow: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: spacing.sm,
  },
  waitingText: {
    fontSize: 11,
    color: mobileTheme.textMuted,
  },
  cancelBtn: {
    alignItems: "center",
    paddingVertical: spacing.sm,
  },
  cancelText: {
    fontSize: 13,
    fontWeight: "600",
    color: mobileTheme.textMuted,
  },
});
