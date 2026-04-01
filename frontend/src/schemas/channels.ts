import { z } from "zod";

const channelPlatformSchema = z.enum([
  "telegram",
  "discord",
  "lark",
  "feishu",
]);

const conversationTypeSchema = z.enum(["private", "group", "channel"]);

export const createChannelBotSchema = z
  .object({
    platform: channelPlatformSchema,
    bot_token: z
      .string()
      .min(1, "Bot token is required")
      .max(512, "Bot token is too long"),
    label: z
      .string()
      .min(1, "Label is required")
      .max(128, "Label must be at most 128 characters"),
    app_id: z.string().max(256).optional(),
    app_secret: z.string().max(512).optional(),
    public_key: z.string().max(256).optional(),
  })
  .superRefine((data, ctx) => {
    if (
      (data.platform === "lark" || data.platform === "feishu") &&
      !data.app_id
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "App ID is required for Lark/Feishu",
        path: ["app_id"],
      });
    }
    if (
      (data.platform === "lark" || data.platform === "feishu") &&
      !data.app_secret
    ) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "App Secret is required for Lark/Feishu",
        path: ["app_secret"],
      });
    }
    if (data.platform === "discord" && !data.public_key) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Public Key is required for Discord",
        path: ["public_key"],
      });
    }
  });

export type CreateChannelBotFormData = z.infer<typeof createChannelBotSchema>;

export const createChannelConversationSchema = z.object({
  channel_bot_id: z.string().uuid("Invalid bot ID"),
  agent_api_key_id: z.string().uuid("Invalid API key ID"),
  platform_conversation_id: z.string().max(256).optional(),
  platform_conversation_type: conversationTypeSchema.optional(),
  platform_sender_id: z.string().max(256).optional(),
  default_agent: z.boolean().optional(),
});

export type CreateChannelConversationFormData = z.infer<
  typeof createChannelConversationSchema
>;

export const updateChannelConversationSchema = z.object({
  agent_api_key_id: z.string().uuid("Invalid API key ID").optional(),
  default_agent: z.boolean().optional(),
  is_active: z.boolean().optional(),
});

export type UpdateChannelConversationFormData = z.infer<
  typeof updateChannelConversationSchema
>;
