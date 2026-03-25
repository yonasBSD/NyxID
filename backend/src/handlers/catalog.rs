use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::AppState;
use crate::errors::AppResult;
use crate::mw::auth::AuthUser;
use crate::services::catalog_service;

#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogEntryResponse {
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub base_url: String,
    pub auth_method: String,
    pub auth_key_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<String>,
    pub requires_gateway_url: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_mode: Option<String>,
    // SSH fields
    pub service_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_ca_public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_allowed_principals: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_certificate_ttl_minutes: Option<u32>,
    // OAuth config fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_token_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_pkce: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogListResponse {
    pub entries: Vec<CatalogEntryResponse>,
}

#[utoipa::path(
    get,
    path = "/api/v1/catalog",
    responses(
        (status = 200, description = "List of available service catalog entries", body = CatalogListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "Catalog"
)]
/// GET /api/v1/catalog
pub async fn list_catalog(
    State(state): State<AppState>,
    _auth_user: AuthUser,
) -> AppResult<Json<CatalogListResponse>> {
    let entries = catalog_service::list_catalog(&state.db).await?;
    let items = entries.into_iter().map(catalog_entry_response).collect();
    Ok(Json(CatalogListResponse { entries: items }))
}

#[utoipa::path(
    get,
    path = "/api/v1/catalog/{slug}",
    params(
        ("slug" = String, Path, description = "Catalog service slug")
    ),
    responses(
        (status = 200, description = "Catalog entry details", body = CatalogEntryResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Catalog entry not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Catalog"
)]
/// GET /api/v1/catalog/{slug}
pub async fn get_catalog_entry(
    State(state): State<AppState>,
    _auth_user: AuthUser,
    Path(slug): Path<String>,
) -> AppResult<Json<CatalogEntryResponse>> {
    let entry = catalog_service::get_catalog_entry(&state.db, &slug).await?;
    Ok(Json(catalog_entry_response(entry)))
}

fn catalog_entry_response(entry: catalog_service::CatalogEntry) -> CatalogEntryResponse {
    let supports_pkce = if entry.supports_pkce {
        Some(true)
    } else {
        None
    };

    CatalogEntryResponse {
        slug: entry.slug,
        name: entry.name,
        description: entry.description,
        base_url: entry.base_url,
        auth_method: entry.auth_method,
        auth_key_name: entry.auth_key_name,
        provider_config_id: entry.provider_config_id,
        provider_type: entry.provider_type,
        requires_gateway_url: entry.requires_gateway_url,
        api_key_instructions: entry.api_key_instructions,
        api_key_url: entry.api_key_url,
        icon_url: entry.icon_url,
        documentation_url: entry.documentation_url,
        credential_mode: entry.credential_mode,
        service_type: entry.service_type,
        ssh_host: entry.ssh_host,
        ssh_port: entry.ssh_port,
        ssh_ca_public_key: entry.ssh_ca_public_key,
        ssh_allowed_principals: entry.ssh_allowed_principals,
        ssh_certificate_ttl_minutes: entry.ssh_certificate_ttl_minutes,
        authorization_url: entry.authorization_url,
        token_url: entry.token_url,
        device_code_url: entry.device_code_url,
        device_verification_url: entry.device_verification_url,
        device_token_url: entry.device_token_url,
        default_scopes: entry.default_scopes,
        supports_pkce,
        device_code_format: entry.device_code_format,
        token_endpoint_auth_method: entry.token_endpoint_auth_method,
        extra_auth_params: entry.extra_auth_params,
    }
}
