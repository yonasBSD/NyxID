use axum::Json;
use serde::{Deserialize, Serialize};

use crate::errors::AppResult;
use crate::mw::auth::AuthUser;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    #[allow(dead_code)]
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub intent: String,
    pub intent_type: String,
}

pub async fn post_chat(
    _auth_user: AuthUser,
    Json(_body): Json<ChatRequest>,
) -> AppResult<Json<ChatResponse>> {
    Ok(Json(ChatResponse {
        reply: "To use the AI assistant, please connect to Aevatar. Visit your account settings to set up the integration.".to_string(),
        intent: "aevatar_redirect".to_string(),
        intent_type: "faq".to_string(),
    }))
}
