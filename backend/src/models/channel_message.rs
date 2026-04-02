use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const COLLECTION_NAME: &str = "channel_messages";

/// An attachment on an inbound or outbound channel message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageAttachment {
    /// Attachment content category: "image", "file", "audio", "video"
    pub content_type: String,
    /// Download URL (may be platform-specific or pre-signed)
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

/// A single message flowing through the channel bot relay pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelMessage {
    #[serde(rename = "_id")]
    pub id: String,
    pub channel_bot_id: String,
    pub conversation_id: String,
    /// The actual platform chat/channel ID (e.g. Telegram chat_id).
    /// Stored separately from the conversation route's platform_conversation_id
    /// because default routes may use a wildcard.
    #[serde(default)]
    pub platform_conversation_id: Option<String>,
    pub user_id: String,
    /// Message direction: "inbound" (platform -> agent) or "outbound" (agent -> platform)
    pub direction: String,
    /// Platform identifier: "telegram", "discord", "lark", "feishu"
    pub platform: String,
    /// Platform-assigned message identifier
    #[serde(default)]
    pub platform_message_id: Option<String>,
    /// Platform-native sender identifier
    #[serde(default)]
    pub sender_platform_id: Option<String>,
    /// Display name of the message sender
    #[serde(default)]
    pub sender_display_name: Option<String>,
    /// Content type: "text", "image", "file", "audio", "video", "unknown"
    pub content_type: String,
    /// Text body of the message (if content_type is "text" or message has a caption)
    #[serde(default)]
    pub text: Option<String>,
    /// File/media attachments
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,
    /// Raw platform-specific webhook payload (for debugging / replay)
    #[serde(default)]
    pub raw_platform_data: Option<serde_json::Value>,
    /// The agent API key that handled this message (set for inbound after routing)
    #[serde(default)]
    pub agent_api_key_id: Option<String>,
    /// Callback delivery status: "pending", "delivered", "failed", "timeout"
    #[serde(default)]
    pub callback_status: Option<String>,
    /// Internal message ID this message is a reply to
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
    /// Platform message ID of the sent reply (set after outbound delivery)
    #[serde(default)]
    pub platform_reply_message_id: Option<String>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name() {
        assert_eq!(COLLECTION_NAME, "channel_messages");
    }

    fn make_message() -> ChannelMessage {
        ChannelMessage {
            id: uuid::Uuid::new_v4().to_string(),
            channel_bot_id: uuid::Uuid::new_v4().to_string(),
            conversation_id: uuid::Uuid::new_v4().to_string(),
            platform_conversation_id: Some("chat_789".to_string()),
            user_id: uuid::Uuid::new_v4().to_string(),
            direction: "inbound".to_string(),
            platform: "telegram".to_string(),
            platform_message_id: Some("msg_123".to_string()),
            sender_platform_id: Some("user_456".to_string()),
            sender_display_name: Some("Alice".to_string()),
            content_type: "text".to_string(),
            text: Some("Hello world".to_string()),
            attachments: vec![],
            raw_platform_data: None,
            agent_api_key_id: None,
            callback_status: None,
            reply_to_message_id: None,
            platform_reply_message_id: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn bson_roundtrip() {
        let msg = make_message();
        let doc = bson::to_document(&msg).expect("serialize");
        let restored: ChannelMessage = bson::from_document(doc).expect("deserialize");
        assert_eq!(msg.id, restored.id);
        assert_eq!(msg.direction, restored.direction);
        assert_eq!(msg.content_type, restored.content_type);
        assert_eq!(msg.text, restored.text);
    }

    #[test]
    fn bson_roundtrip_with_attachment() {
        let mut msg = make_message();
        msg.attachments = vec![MessageAttachment {
            content_type: "image".to_string(),
            url: "https://example.com/photo.jpg".to_string(),
            filename: Some("photo.jpg".to_string()),
            mime_type: Some("image/jpeg".to_string()),
            size_bytes: Some(102400),
        }];
        let doc = bson::to_document(&msg).expect("serialize");
        let restored: ChannelMessage = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.attachments.len(), 1);
        assert_eq!(restored.attachments[0].content_type, "image");
        assert_eq!(restored.attachments[0].size_bytes, Some(102400));
    }

    #[test]
    fn bson_roundtrip_with_callback_status() {
        let mut msg = make_message();
        msg.callback_status = Some("delivered".to_string());
        msg.agent_api_key_id = Some(uuid::Uuid::new_v4().to_string());
        msg.reply_to_message_id = Some(uuid::Uuid::new_v4().to_string());
        msg.platform_reply_message_id = Some("platform_msg_789".to_string());
        let doc = bson::to_document(&msg).expect("serialize");
        let restored: ChannelMessage = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.callback_status.as_deref(), Some("delivered"));
        assert!(restored.agent_api_key_id.is_some());
        assert!(restored.reply_to_message_id.is_some());
        assert_eq!(
            restored.platform_reply_message_id.as_deref(),
            Some("platform_msg_789")
        );
    }

    #[test]
    fn bson_all_fields_serialized() {
        let msg = make_message();
        let doc = bson::to_document(&msg).expect("serialize");
        assert!(doc.contains_key("_id"));
        assert!(doc.contains_key("channel_bot_id"));
        assert!(doc.contains_key("conversation_id"));
        assert!(doc.contains_key("user_id"));
        assert!(doc.contains_key("direction"));
        assert!(doc.contains_key("platform"));
        assert!(doc.contains_key("content_type"));
        assert!(doc.contains_key("created_at"));
    }

    #[test]
    fn bson_backward_compat_missing_optional_fields() {
        let msg = make_message();
        let mut doc = bson::to_document(&msg).expect("serialize");
        doc.remove("platform_message_id");
        doc.remove("sender_platform_id");
        doc.remove("sender_display_name");
        doc.remove("text");
        doc.remove("attachments");
        doc.remove("raw_platform_data");
        doc.remove("agent_api_key_id");
        doc.remove("callback_status");
        doc.remove("reply_to_message_id");
        doc.remove("platform_reply_message_id");
        let restored: ChannelMessage = bson::from_document(doc).expect("deserialize");
        assert_eq!(restored.platform_message_id, None);
        assert_eq!(restored.text, None);
        assert!(restored.attachments.is_empty());
        assert_eq!(restored.callback_status, None);
    }

    #[test]
    fn bson_roundtrip_with_raw_platform_data() {
        let mut msg = make_message();
        msg.raw_platform_data =
            Some(serde_json::json!({"update_id": 12345, "message": {"text": "hi"}}));
        let doc = bson::to_document(&msg).expect("serialize");
        let restored: ChannelMessage = bson::from_document(doc).expect("deserialize");
        assert!(restored.raw_platform_data.is_some());
    }
}
