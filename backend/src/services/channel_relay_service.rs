//! Channel relay service: message storage, agent callback delivery, and reply
//! handling.
//!
//! Responsible for persisting inbound/outbound messages, forwarding inbound
//! messages to agent callback URLs with HMAC signatures, and parsing
//! synchronous reply payloads.

use chrono::Utc;
use futures::TryStreamExt;
use hmac::{Hmac, Mac};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::config::AppConfig;
use crate::errors::{AppError, AppResult};
use crate::models::channel_message::{COLLECTION_NAME, ChannelMessage, MessageAttachment};
use crate::services::channel_platform::InboundMessage;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Callback payload types (sent to agent)
// ---------------------------------------------------------------------------

/// Normalized message payload delivered to the agent's callback URL.
#[derive(Debug, Clone, Serialize)]
pub struct CallbackPayload {
    pub message_id: String,
    pub platform: String,
    pub agent: CallbackAgent,
    pub conversation: CallbackConversation,
    pub sender: CallbackSender,
    pub content: CallbackContent,
    /// NyxID internal message ID of the message being replied to (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    /// Platform-native message ID of the message being replied to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_platform_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackAgent {
    pub api_key_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackConversation {
    pub id: String,
    pub platform_id: String,
    #[serde(rename = "type")]
    pub conversation_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackSender {
    pub platform_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<CallbackAttachment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackAttachment {
    pub content_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

// ---------------------------------------------------------------------------
// Agent reply payload types (received from agent)
// ---------------------------------------------------------------------------

/// Parsed from the agent's synchronous 200 response body.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentReplyPayload {
    pub reply: Option<AgentReply>,
}

/// A reply the agent wants sent back to the chat platform.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentReply {
    pub text: Option<String>,
    pub reply_to_platform_message_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Message storage
// ---------------------------------------------------------------------------

/// Persist an inbound message (platform -> agent direction).
#[allow(clippy::too_many_arguments)]
pub async fn store_inbound_message(
    db: &mongodb::Database,
    channel_bot_id: &str,
    conversation_id: &str,
    user_id: &str,
    platform: &str,
    inbound: &InboundMessage,
    agent_api_key_id: &str,
) -> AppResult<ChannelMessage> {
    let attachments: Vec<MessageAttachment> = inbound
        .attachments
        .iter()
        .map(|a| MessageAttachment {
            content_type: a.content_type.clone(),
            url: a.url.clone(),
            filename: a.filename.clone(),
            mime_type: a.mime_type.clone(),
            size_bytes: a.size_bytes,
        })
        .collect();

    let message = ChannelMessage {
        id: uuid::Uuid::new_v4().to_string(),
        channel_bot_id: channel_bot_id.to_string(),
        conversation_id: conversation_id.to_string(),
        platform_conversation_id: Some(inbound.conversation_id.clone()),
        user_id: user_id.to_string(),
        direction: "inbound".to_string(),
        platform: platform.to_string(),
        platform_message_id: Some(inbound.platform_message_id.clone()),
        sender_platform_id: Some(inbound.sender_platform_id.clone()),
        sender_display_name: inbound.sender_display_name.clone(),
        content_type: inbound.content_type.clone(),
        text: inbound.text.clone(),
        attachments,
        raw_platform_data: Some(inbound.raw_data.clone()),
        agent_api_key_id: Some(agent_api_key_id.to_string()),
        callback_status: Some("pending".to_string()),
        reply_to_message_id: None,
        platform_reply_message_id: None,
        created_at: Utc::now(),
    };

    db.collection::<ChannelMessage>(COLLECTION_NAME)
        .insert_one(&message)
        .await?;

    Ok(message)
}

/// Persist an outbound message (agent -> platform direction).
#[allow(clippy::too_many_arguments)]
pub async fn store_outbound_message(
    db: &mongodb::Database,
    channel_bot_id: &str,
    conversation_id: &str,
    user_id: &str,
    platform: &str,
    text: &str,
    agent_api_key_id: &str,
    reply_to_message_id: Option<&str>,
    platform_message_id: Option<&str>,
) -> AppResult<ChannelMessage> {
    let message = ChannelMessage {
        id: uuid::Uuid::new_v4().to_string(),
        channel_bot_id: channel_bot_id.to_string(),
        conversation_id: conversation_id.to_string(),
        platform_conversation_id: None,
        user_id: user_id.to_string(),
        direction: "outbound".to_string(),
        platform: platform.to_string(),
        platform_message_id: platform_message_id.map(String::from),
        sender_platform_id: None,
        sender_display_name: None,
        content_type: "text".to_string(),
        text: Some(text.to_string()),
        attachments: vec![],
        raw_platform_data: None,
        agent_api_key_id: Some(agent_api_key_id.to_string()),
        callback_status: None,
        reply_to_message_id: reply_to_message_id.map(String::from),
        platform_reply_message_id: None,
        created_at: Utc::now(),
    };

    db.collection::<ChannelMessage>(COLLECTION_NAME)
        .insert_one(&message)
        .await?;

    Ok(message)
}

// ---------------------------------------------------------------------------
// Callback delivery
// ---------------------------------------------------------------------------

/// Forward an inbound message to the agent's callback URL.
///
/// Signs the request body with HMAC-SHA256 using the API key hash as the
/// signing key. Returns the agent's synchronous reply (if 200), or `None`
/// for 202 (async processing).
pub async fn forward_to_agent(
    http_client: &reqwest::Client,
    config: &AppConfig,
    callback_url: &str,
    payload: &CallbackPayload,
    api_key_hash: &str,
) -> AppResult<Option<AgentReplyPayload>> {
    let body_bytes = serde_json::to_vec(payload)
        .map_err(|e| AppError::Internal(format!("failed to serialize callback payload: {e}")))?;

    let signature = compute_hmac_signature(api_key_hash.as_bytes(), &body_bytes)?;
    let timestamp = Utc::now().to_rfc3339();

    let timeout =
        std::time::Duration::from_secs(u64::from(config.channel_relay_callback_timeout_secs));

    let response = http_client
        .post(callback_url)
        .header("Content-Type", "application/json")
        .header("X-NyxID-Signature", &signature)
        .header("X-NyxID-Message-Id", &payload.message_id)
        .header("X-NyxID-Timestamp", &timestamp)
        .header("X-NyxID-Platform", &payload.platform)
        .timeout(timeout)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| AppError::ChannelRelayFailed(format!("callback request failed: {e}")))?;

    let status = response.status();

    if status.as_u16() == 202 {
        return Ok(None);
    }

    if !status.is_success() {
        return Err(AppError::ChannelRelayFailed(format!(
            "callback returned HTTP {status}"
        )));
    }

    // 200: try to parse a reply payload
    let resp_bytes = response.bytes().await.map_err(|e| {
        AppError::ChannelRelayFailed(format!("failed to read callback response body: {e}"))
    })?;

    if resp_bytes.is_empty() {
        return Ok(None);
    }

    let reply: AgentReplyPayload = serde_json::from_slice(&resp_bytes).map_err(|e| {
        AppError::ChannelRelayFailed(format!("failed to parse callback response: {e}"))
    })?;

    Ok(Some(reply))
}

/// Update the callback delivery status on a stored message.
pub async fn update_callback_status(
    db: &mongodb::Database,
    message_id: &str,
    status: &str,
) -> AppResult<()> {
    db.collection::<ChannelMessage>(COLLECTION_NAME)
        .update_one(
            doc! { "_id": message_id },
            doc! { "$set": { "callback_status": status }},
        )
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Message queries
// ---------------------------------------------------------------------------

/// Get a single message by ID.
pub async fn get_message(db: &mongodb::Database, message_id: &str) -> AppResult<ChannelMessage> {
    db.collection::<ChannelMessage>(COLLECTION_NAME)
        .find_one(doc! { "_id": message_id })
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Message not found: {message_id}")))
}

/// List messages for a conversation with pagination (newest first).
///
/// Returns `(messages, total_count)`.
pub async fn list_messages(
    db: &mongodb::Database,
    conversation_id: &str,
    page: u64,
    per_page: u64,
) -> AppResult<(Vec<ChannelMessage>, u64)> {
    let filter = doc! { "conversation_id": conversation_id };

    let total = db
        .collection::<ChannelMessage>(COLLECTION_NAME)
        .count_documents(filter.clone())
        .await?;

    let skip = (page.saturating_sub(1)) * per_page;
    let messages: Vec<ChannelMessage> = db
        .collection::<ChannelMessage>(COLLECTION_NAME)
        .find(filter)
        .sort(doc! { "created_at": -1 })
        .skip(skip)
        .limit(per_page as i64)
        .await?
        .try_collect()
        .await?;

    Ok((messages, total))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute HMAC-SHA256 signature of the given body using the provided key.
fn compute_hmac_signature(key: &[u8], body: &[u8]) -> AppResult<String> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| AppError::Internal(format!("HMAC key error: {e}")))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

// ---------------------------------------------------------------------------
// Payload builders
// ---------------------------------------------------------------------------

/// Build a [`CallbackPayload`] from the stored message and routing context.
pub fn build_callback_payload(
    message: &ChannelMessage,
    conversation: &crate::models::channel_conversation::ChannelConversation,
    api_key_id: &str,
    api_key_name: &str,
    inbound: &InboundMessage,
) -> CallbackPayload {
    let attachments: Vec<CallbackAttachment> = inbound
        .attachments
        .iter()
        .map(|a| CallbackAttachment {
            content_type: a.content_type.clone(),
            url: a.url.clone(),
            filename: a.filename.clone(),
            mime_type: a.mime_type.clone(),
            size_bytes: a.size_bytes,
        })
        .collect();

    CallbackPayload {
        message_id: message.id.clone(),
        platform: message.platform.clone(),
        agent: CallbackAgent {
            api_key_id: api_key_id.to_string(),
            name: api_key_name.to_string(),
        },
        conversation: CallbackConversation {
            id: conversation.id.clone(),
            // Use the real platform chat ID from the inbound message, not the
            // route's configured value (which may be "*" for default routes).
            platform_id: inbound.conversation_id.clone(),
            conversation_type: conversation.platform_conversation_type.clone(),
        },
        sender: CallbackSender {
            platform_id: inbound.sender_platform_id.clone(),
            display_name: inbound.sender_display_name.clone(),
        },
        content: CallbackContent {
            content_type: inbound.content_type.clone(),
            text: inbound.text.clone(),
            attachments,
        },
        reply_to_message_id: None, // NyxID ID not available without a DB lookup
        reply_to_platform_message_id: inbound.reply_to_platform_message_id.clone(),
        thread_id: inbound.thread_id.clone(),
        timestamp: message.created_at.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_signature_is_deterministic() {
        let key = b"test_api_key_hash";
        let body = b"{\"message_id\":\"abc\"}";
        let sig1 = compute_hmac_signature(key, body).unwrap();
        let sig2 = compute_hmac_signature(key, body).unwrap();
        assert_eq!(sig1, sig2);
        // HMAC-SHA256 produces 64-char hex string
        assert_eq!(sig1.len(), 64);
    }

    #[test]
    fn hmac_different_keys_different_signatures() {
        let body = b"same body";
        let sig_a = compute_hmac_signature(b"key_a", body).unwrap();
        let sig_b = compute_hmac_signature(b"key_b", body).unwrap();
        assert_ne!(sig_a, sig_b);
    }

    #[test]
    fn hmac_different_bodies_different_signatures() {
        let key = b"same_key";
        let sig_a = compute_hmac_signature(key, b"body_a").unwrap();
        let sig_b = compute_hmac_signature(key, b"body_b").unwrap();
        assert_ne!(sig_a, sig_b);
    }

    #[test]
    fn callback_payload_serializes_to_json() {
        let payload = CallbackPayload {
            message_id: "msg-1".to_string(),
            platform: "telegram".to_string(),
            agent: CallbackAgent {
                api_key_id: "key-1".to_string(),
                name: "test-agent".to_string(),
            },
            conversation: CallbackConversation {
                id: "conv-1".to_string(),
                platform_id: "12345".to_string(),
                conversation_type: "private".to_string(),
            },
            sender: CallbackSender {
                platform_id: "user-1".to_string(),
                display_name: Some("Alice".to_string()),
            },
            content: CallbackContent {
                content_type: "text".to_string(),
                text: Some("Hello".to_string()),
                attachments: vec![],
            },
            reply_to_message_id: None,
            reply_to_platform_message_id: None,
            thread_id: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["message_id"], "msg-1");
        assert_eq!(json["agent"]["api_key_id"], "key-1");
        assert_eq!(json["conversation"]["type"], "private");
        assert_eq!(json["content"]["type"], "text");
        // Optional None fields should be absent
        assert!(json.get("reply_to_message_id").is_none());
        assert!(json.get("thread_id").is_none());
    }

    #[test]
    fn callback_payload_omits_empty_attachments() {
        let content = CallbackContent {
            content_type: "text".to_string(),
            text: Some("Hi".to_string()),
            attachments: vec![],
        };
        let json = serde_json::to_value(&content).unwrap();
        // Empty attachments should be omitted
        assert!(json.get("attachments").is_none());
    }

    #[test]
    fn callback_payload_includes_nonempty_attachments() {
        let content = CallbackContent {
            content_type: "image".to_string(),
            text: None,
            attachments: vec![CallbackAttachment {
                content_type: "image".to_string(),
                url: "https://example.com/photo.jpg".to_string(),
                filename: Some("photo.jpg".to_string()),
                mime_type: Some("image/jpeg".to_string()),
                size_bytes: Some(1024),
            }],
        };
        let json = serde_json::to_value(&content).unwrap();
        let atts = json["attachments"].as_array().unwrap();
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0]["content_type"], "image");
    }

    #[test]
    fn agent_reply_payload_deserializes_with_reply() {
        let json = serde_json::json!({
            "reply": {
                "text": "Got it!",
                "reply_to_platform_message_id": "42",
                "metadata": { "parse_mode": "Markdown" }
            }
        });
        let payload: AgentReplyPayload = serde_json::from_value(json).unwrap();
        let reply = payload.reply.unwrap();
        assert_eq!(reply.text.as_deref(), Some("Got it!"));
        assert_eq!(reply.reply_to_platform_message_id.as_deref(), Some("42"));
        assert!(reply.metadata.is_some());
    }

    #[test]
    fn agent_reply_payload_deserializes_without_reply() {
        let json = serde_json::json!({ "reply": null });
        let payload: AgentReplyPayload = serde_json::from_value(json).unwrap();
        assert!(payload.reply.is_none());
    }

    #[test]
    fn agent_reply_payload_deserializes_empty_object() {
        let json = serde_json::json!({});
        let payload: AgentReplyPayload = serde_json::from_value(json).unwrap();
        assert!(payload.reply.is_none());
    }
}
