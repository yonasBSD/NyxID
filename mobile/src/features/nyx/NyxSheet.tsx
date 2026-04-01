import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Dimensions,
  FlatList,
  Keyboard,
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import Animated, {
  useSharedValue,
  useAnimatedStyle,
  withSpring,
  withTiming,
  runOnJS,
} from "react-native-reanimated";
import {
  GestureHandlerRootView,
  Gesture,
  GestureDetector,
} from "react-native-gesture-handler";
import Svg, { Circle, Path, Defs, LinearGradient, Stop } from "react-native-svg";
import { useNyxChat, type NyxMessage } from "../../hooks/useNyxChat";
import { SecureKeyModal } from "../../components/SecureKeyModal";
import { mobileTheme } from "../../theme/mobileTheme";
import { radius } from "../../theme/designTokens";

const SCREEN_HEIGHT = Dimensions.get("window").height;
const SHEET_TOP = 80;
const SHEET_HEIGHT = SCREEN_HEIGHT - SHEET_TOP;
const CLOSE_THRESHOLD = 50;

type NyxSheetProps = {
  isOpen: boolean;
  onClose: () => void;
};

const WELCOME_MESSAGE: NyxMessage = {
  _id: "welcome",
  text: "I can help with FAQs and basic platform tasks \u2014 check pending approvals, show grants, or explain NyxID features.",
  createdAt: new Date(),
  user: { _id: "nyx", name: "Nyx" },
};

const SUGGESTED_CHIPS = ["What's pending?", "Show grants", "What is PKCE?"];

function NyxAvatar() {
  return (
    <View style={styles.avatar}>
      <Svg width={16} height={16} viewBox="0 0 130 130" fill="none">
        <Defs>
          <LinearGradient id="nav" gradientUnits="userSpaceOnUse" x1="56" y1="62" x2="86" y2="62" gradientTransform="rotate(160 71 62)">
            <Stop offset="0" stopColor="#C4B5FD" />
            <Stop offset="1" stopColor="#7C3AED" />
          </LinearGradient>
        </Defs>
        <Circle cx={65} cy={65} r={40} fill="none" stroke="rgba(167,139,250,0.3)" strokeWidth={1} />
        <Path d="M24 0q6 8 6 20 0 12-6 20-14-4-20-12-4-14-2-24 4-4 22-4z" transform="translate(56 42)" fill="url(#nav)" />
        <Circle cx={31.5} cy={49.5} r={1.5} fill="#C4B5FD" />
      </Svg>
    </View>
  );
}

function ChatBubble({ message, onChipPress }: { message: NyxMessage; onChipPress?: (text: string) => void }) {
  const isBot = message.user._id === "nyx";
  const isWelcome = message._id === "welcome";

  return (
    <View style={[styles.bubble, isBot ? styles.bubbleBot : styles.bubbleUser]}>
      {isBot && <Text style={styles.botName}>NYX</Text>}
      <Text style={styles.bubbleText}>{message.text}</Text>
      {isWelcome && (
        <>
          <View style={styles.scopeBadge}>
            <Svg width={10} height={10} viewBox="0 0 12 12" fill="none" stroke={mobileTheme.primary} strokeWidth={1.5}>
              <Path d="M6 11s4-2 4-5V3l-4-1.5L2 3v3c0 3 4 5 4 5z" />
            </Svg>
            <Text style={styles.scopeText}>Limited scope · No secrets handled</Text>
          </View>
          <View style={styles.chips}>
            {SUGGESTED_CHIPS.map((chip) => (
              <Pressable key={chip} style={styles.chip} onPress={() => onChipPress?.(chip)}>
                <Text style={styles.chipText}>{chip}</Text>
              </Pressable>
            ))}
          </View>
        </>
      )}
    </View>
  );
}

export function NyxSheet({ isOpen, onClose }: NyxSheetProps) {
  const [modalVisible, setModalVisible] = useState(false);
  const translateY = useSharedValue(SHEET_HEIGHT);
  const inputRef = useRef<string>("");
  const textInputRef = useRef<TextInput>(null);
  const { messages, isLoading, requiresSecretInput, sendMessage, submitSecretInput, clearChat } = useNyxChat();

  // isOpen is the source of truth — react to it for both open and close
  useEffect(() => {
    if (isOpen) {
      setModalVisible(true);
      translateY.value = withSpring(0, { damping: 28, stiffness: 300 });
    } else {
      translateY.value = withTiming(SHEET_HEIGHT, { duration: 250 });
      // Hide modal after animation; cleanup cancels if isOpen flips back to true
      const timer = setTimeout(() => setModalVisible(false), 300);
      return () => clearTimeout(timer);
    }
  }, [isOpen, translateY]);

  // Close actions call onClose() immediately — animation follows via useEffect
  const handleClose = useCallback(() => {
    clearChat();
    onClose();
  }, [clearChat, onClose]);

  // Drag gesture — handle area only
  const panGesture = useMemo(
    () =>
      Gesture.Pan()
        .onUpdate((e) => {
          "worklet";
          if (e.translationY > 0) {
            translateY.value = e.translationY;
          }
        })
        .onEnd((e) => {
          "worklet";
          if (e.translationY > CLOSE_THRESHOLD) {
            // Start animation immediately for smooth feel
            translateY.value = withTiming(SHEET_HEIGHT, { duration: 250 });
            runOnJS(handleClose)();
          } else {
            translateY.value = withSpring(0, { damping: 28, stiffness: 300 });
          }
        }),
    [handleClose, translateY]
  );

  const sheetAnimatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: translateY.value }],
  }));

  const backdropAnimatedStyle = useAnimatedStyle(() => ({
    opacity: 0.55 * Math.max(0, 1 - translateY.value / SHEET_HEIGHT),
  }));

  const allMessages = [...messages, WELCOME_MESSAGE];

  const handleSend = useCallback(() => {
    const text = inputRef.current.trim();
    if (!text || isLoading) return;
    inputRef.current = "";
    textInputRef.current?.clear();
    Keyboard.dismiss();
    void sendMessage(text);
  }, [isLoading, sendMessage]);

  const handleChipPress = useCallback(
    (text: string) => {
      void sendMessage(text);
    },
    [sendMessage]
  );

  return (
    <Modal
      visible={modalVisible}
      transparent
      animationType="none"
      statusBarTranslucent
      onRequestClose={handleClose}
    >
      <GestureHandlerRootView style={styles.modalRoot}>
        {/* Backdrop */}
        <Animated.View style={[styles.backdrop, backdropAnimatedStyle]} pointerEvents="auto">
          <Pressable style={StyleSheet.absoluteFill} onPress={handleClose} />
        </Animated.View>

        {/* Sheet */}
        <Animated.View style={[styles.sheet, sheetAnimatedStyle]}>
          {/* Drag handle */}
          <GestureDetector gesture={panGesture}>
            <Animated.View style={styles.handleArea}>
              <View style={styles.handle} />
            </Animated.View>
          </GestureDetector>

          {/* Header */}
          <View style={styles.header}>
            <View style={styles.headerLeft}>
              <NyxAvatar />
              <View>
                <Text style={styles.headerTitle}>Nyx</Text>
                <Text style={styles.headerStatus}>Online · FAQ & Platform Tasks</Text>
              </View>
            </View>
            <Pressable style={styles.closeBtn} onPress={handleClose}>
              <Text style={styles.closeBtnText}>✕</Text>
            </Pressable>
          </View>

          {/* Messages + Input */}
          <KeyboardAvoidingView
            style={styles.chatArea}
            behavior={Platform.OS === "ios" ? "padding" : undefined}
            keyboardVerticalOffset={SHEET_TOP}
          >
            <FlatList
              data={allMessages}
              keyExtractor={(item: NyxMessage) => item._id}
              renderItem={({ item }: { item: NyxMessage }) => (
                <ChatBubble
                  message={item}
                  onChipPress={item._id === "welcome" ? handleChipPress : undefined}
                />
              )}
              inverted
              contentContainerStyle={styles.messageList}
              ItemSeparatorComponent={() => <View style={styles.messageSep} />}
              keyboardShouldPersistTaps="handled"
              style={styles.messagesFlex}
            />

            <View style={styles.inputArea}>
              <View style={styles.inputBar}>
                <TextInput
                  ref={textInputRef}
                  style={styles.inputText}
                  placeholder="Ask about approvals, grants, or features..."
                  placeholderTextColor={mobileTheme.textMuted}
                  onChangeText={(t: string) => {
                    inputRef.current = t;
                  }}
                  onSubmitEditing={handleSend}
                  returnKeyType="send"
                  autoCapitalize="none"
                />
                <Pressable style={styles.sendBtn} onPress={handleSend}>
                  <Svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="#FFFFFF" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round">
                    <Path d="M12 19V5" />
                    <Path d="M5 12l7-7 7 7" />
                  </Svg>
                </Pressable>
              </View>
            </View>
          </KeyboardAvoidingView>
        </Animated.View>
      </GestureHandlerRootView>

      {requiresSecretInput && requiresSecretInput.length > 0 && (
        <SecureKeyModal
          visible
          fields={requiresSecretInput}
          onSubmit={(vals) => void submitSecretInput(vals)}
          onCancel={() => void sendMessage("cancel")}
        />
      )}
    </Modal>
  );
}

const styles = StyleSheet.create({
  modalRoot: {
    flex: 1,
  },
  backdrop: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "#000",
  },
  sheet: {
    position: "absolute",
    top: SHEET_TOP,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: mobileTheme.bg,
    borderTopLeftRadius: 24,
    borderTopRightRadius: 24,
    borderWidth: 1,
    borderBottomWidth: 0,
    borderColor: mobileTheme.border,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: -10 },
    shadowOpacity: 0.4,
    shadowRadius: 40,
    elevation: 24,
    overflow: "hidden",
  },
  handleArea: {
    alignItems: "center",
    paddingTop: 10,
    paddingBottom: 6,
  },
  handle: {
    width: 36,
    height: 4,
    borderRadius: 2,
    backgroundColor: "rgba(255,255,255,0.15)",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 20,
    paddingVertical: 4,
    paddingBottom: 14,
    borderBottomWidth: 1,
    borderBottomColor: mobileTheme.borderSoft,
  },
  headerLeft: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  avatar: {
    width: 30,
    height: 30,
    borderRadius: 15,
    backgroundColor: mobileTheme.primaryGlow,
    borderWidth: 1,
    borderColor: "rgba(139,92,246,0.25)",
    alignItems: "center",
    justifyContent: "center",
  },
  headerTitle: {
    fontSize: 15,
    fontWeight: "700",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  headerStatus: {
    fontSize: 10,
    fontWeight: "600",
    color: mobileTheme.success,
  },
  closeBtn: {
    width: 30,
    height: 30,
    borderRadius: 15,
    backgroundColor: "rgba(255,255,255,0.06)",
    borderWidth: 1,
    borderColor: mobileTheme.borderSoft,
    alignItems: "center",
    justifyContent: "center",
  },
  closeBtnText: {
    fontSize: 14,
    fontWeight: "600",
    color: mobileTheme.textMuted,
  },
  chatArea: {
    flex: 1,
  },
  messagesFlex: {
    flex: 1,
  },
  messageList: {
    padding: 14,
    paddingHorizontal: 20,
  },
  messageSep: {
    height: 10,
  },
  bubble: {
    maxWidth: "82%",
    padding: 10,
    paddingHorizontal: 14,
    gap: 4,
  },
  bubbleBot: {
    backgroundColor: mobileTheme.card,
    borderWidth: 1,
    borderColor: mobileTheme.borderSoft,
    borderRadius: 12,
    borderBottomLeftRadius: 4,
    alignSelf: "flex-start",
  },
  bubbleUser: {
    backgroundColor: mobileTheme.primaryDim,
    borderRadius: 12,
    borderBottomRightRadius: 4,
    alignSelf: "flex-end",
  },
  botName: {
    fontSize: 10,
    fontWeight: "700",
    color: mobileTheme.primary,
    letterSpacing: 0.3,
    marginBottom: 4,
  },
  bubbleText: {
    fontSize: 13,
    lineHeight: 19.5,
    color: mobileTheme.textPrimary,
  },
  scopeBadge: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
    paddingVertical: 3,
    paddingHorizontal: 8,
    borderRadius: 10,
    backgroundColor: "rgba(139,92,246,0.1)",
    borderWidth: 1,
    borderColor: "rgba(139,92,246,0.2)",
    alignSelf: "flex-start",
    marginTop: 4,
  },
  scopeText: {
    fontSize: 9,
    fontWeight: "700",
    color: mobileTheme.primary,
    letterSpacing: 0.3,
    textTransform: "uppercase",
  },
  chips: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 6,
    marginTop: 6,
  },
  chip: {
    paddingVertical: 5,
    paddingHorizontal: 12,
    borderRadius: 20,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: "rgba(255,255,255,0.03)",
  },
  chipText: {
    fontSize: 11,
    fontWeight: "600",
    color: mobileTheme.textSecondary,
  },
  inputArea: {
    paddingHorizontal: 20,
    paddingVertical: 12,
    paddingBottom: 20,
  },
  inputBar: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingVertical: 10,
    paddingHorizontal: 14,
    borderRadius: radius.pill,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: mobileTheme.cardSoft,
  },
  inputText: {
    flex: 1,
    fontSize: 13,
    color: mobileTheme.textPrimary,
  },
  sendBtn: {
    width: 30,
    height: 30,
    borderRadius: 15,
    backgroundColor: mobileTheme.primary,
    borderWidth: 1,
    borderColor: "rgba(139,92,246,0.5)",
    alignItems: "center",
    justifyContent: "center",
  },
});
