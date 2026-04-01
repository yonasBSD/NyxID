import { useState } from "react";
import { Modal, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import Svg, { Path, Rect } from "react-native-svg";
import { mobileTheme } from "../theme/mobileTheme";
import { radius, spacing } from "../theme/designTokens";
import type { SecretInputRequest } from "../lib/api/chatTypes";

type SecureKeyModalProps = {
  visible: boolean;
  fields: SecretInputRequest[];
  onSubmit: (values: { param_name: string; value: string }[]) => void;
  onCancel: () => void;
};

function ShieldCheckIcon() {
  return (
    <Svg width={18} height={18} viewBox="0 0 24 24" fill="none" stroke="#22C55E" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <Path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
      <Path d="M9 12l2 2 4-4" />
    </Svg>
  );
}

function LockIcon() {
  return (
    <Svg width={10} height={10} viewBox="0 0 12 12" fill="none" stroke="#22C55E" strokeWidth={1.5}>
      <Rect x={3} y={5.5} width={6} height={5} rx={1} />
      <Path d="M4.5 5.5V4a1.5 1.5 0 0 1 3 0v1.5" />
    </Svg>
  );
}

export function SecureKeyModal({ visible, fields, onSubmit, onCancel }: SecureKeyModalProps) {
  const [values, setValues] = useState<Record<string, string>>({});

  const handleSubmit = () => {
    onSubmit(fields.map((f) => ({ param_name: f.param_name, value: values[f.param_name] ?? "" })));
    setValues({});
  };

  const handleCancel = () => {
    setValues({});
    onCancel();
  };

  const serviceLabel = fields[0]?.label ?? "Service";

  return (
    <Modal visible={visible} transparent animationType="fade" onRequestClose={handleCancel}>
      <View style={styles.backdrop}>
        <View style={styles.modal}>
          <View style={styles.iconWrap}>
            <ShieldCheckIcon />
          </View>
          <Text style={styles.title}>Secure API Key Input</Text>
          <Text style={styles.desc}>
            This key is stored directly in encrypted storage. It is{" "}
            <Text style={styles.descBold}>never sent to the LLM</Text> or processed by Nyx.
          </Text>

          <View style={styles.trustBadge}>
            <LockIcon />
            <Text style={styles.trustText}>End-to-end encrypted · LLM-isolated</Text>
          </View>

          <Text style={styles.fieldLabel}>SERVICE</Text>
          <View style={styles.serviceRow}>
            <View style={styles.serviceDot}>
              <Text style={styles.serviceDotText}>◆</Text>
            </View>
            <Text style={styles.serviceText}>{serviceLabel}</Text>
          </View>

          {fields.map((field) => (
            <View key={field.param_name}>
              <Text style={styles.fieldLabel}>{field.label.toUpperCase()}</Text>
              <TextInput
                style={styles.input}
                secureTextEntry
                placeholder={field.placeholder || "sk-••••••••••••••••••••"}
                placeholderTextColor={mobileTheme.textMuted}
                value={values[field.param_name] ?? ""}
                onChangeText={(text) => setValues((prev) => ({ ...prev, [field.param_name]: text }))}
                autoCapitalize="none"
                autoCorrect={false}
              />
            </View>
          ))}

          <View style={styles.actions}>
            <Pressable style={styles.cancelBtn} onPress={handleCancel}>
              <Text style={styles.cancelText}>Cancel</Text>
            </Pressable>
            <Pressable style={styles.submitBtn} onPress={handleSubmit}>
              <Text style={styles.submitText}>Save Key</Text>
            </Pressable>
          </View>
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
    padding: spacing.xxl,
  },
  modal: {
    width: 270,
    backgroundColor: mobileTheme.card,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    borderRadius: radius.lg,
    padding: 20,
    gap: 12,
  },
  iconWrap: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: "rgba(34,197,94,0.12)",
    borderWidth: 1,
    borderColor: "rgba(34,197,94,0.25)",
    alignItems: "center",
    justifyContent: "center",
    alignSelf: "center",
  },
  title: {
    fontSize: 15,
    fontWeight: "700",
    textAlign: "center",
    color: mobileTheme.textPrimary,
    fontFamily: "SpaceGrotesk_700Bold",
  },
  desc: {
    fontSize: 11,
    color: mobileTheme.textSecondary,
    textAlign: "center",
    lineHeight: 16.5,
  },
  descBold: {
    fontWeight: "700",
    color: mobileTheme.textPrimary,
  },
  trustBadge: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 5,
    paddingVertical: 6,
    paddingHorizontal: 10,
    borderRadius: radius.sm,
    backgroundColor: "rgba(34,197,94,0.08)",
    borderWidth: 1,
    borderColor: "rgba(34,197,94,0.2)",
  },
  trustText: {
    fontSize: 10,
    fontWeight: "700",
    color: mobileTheme.success,
    letterSpacing: 0.3,
  },
  fieldLabel: {
    fontSize: 10,
    fontWeight: "600",
    color: mobileTheme.textMuted,
    letterSpacing: 0.4,
  },
  serviceRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingVertical: 8,
    paddingHorizontal: 10,
    borderRadius: radius.sm,
    backgroundColor: mobileTheme.bg,
    borderWidth: 1,
    borderColor: mobileTheme.border,
  },
  serviceDot: {
    width: 20,
    height: 20,
    borderRadius: 4,
    backgroundColor: "rgba(16,163,127,0.15)",
    alignItems: "center",
    justifyContent: "center",
  },
  serviceDotText: {
    fontSize: 10,
    fontWeight: "800",
    color: "#10A37F",
  },
  serviceText: {
    fontSize: 13,
    fontWeight: "600",
    color: mobileTheme.textPrimary,
  },
  input: {
    paddingVertical: 10,
    paddingHorizontal: 12,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: mobileTheme.bg,
    fontSize: 13,
    color: mobileTheme.textMuted,
    fontFamily: "monospace",
    letterSpacing: 2,
  },
  actions: {
    flexDirection: "row",
    gap: 8,
  },
  cancelBtn: {
    paddingVertical: 8,
    paddingHorizontal: 16,
    borderRadius: radius.sm,
    borderWidth: 1,
    borderColor: mobileTheme.border,
    backgroundColor: "transparent",
  },
  cancelText: {
    fontSize: 12,
    fontWeight: "600",
    color: mobileTheme.textSecondary,
  },
  submitBtn: {
    flex: 1,
    paddingVertical: 8,
    borderRadius: radius.sm,
    backgroundColor: mobileTheme.primary,
    alignItems: "center",
  },
  submitText: {
    fontSize: 12,
    fontWeight: "700",
    color: "#FFFFFF",
  },
});
