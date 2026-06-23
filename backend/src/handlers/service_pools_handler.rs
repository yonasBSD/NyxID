use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::errors::{AppError, AppResult};
use crate::models::service_pool::{
    COLLECTION_NAME as SERVICE_POOLS, PoolStrategy, ServicePool, ServicePoolMember,
};
use crate::mw::auth::AuthUser;
use crate::services::{org_service, service_pool_service};

#[derive(Debug, Deserialize, IntoParams)]
pub struct PoolListQuery {
    /// Optional org owner. Omit for personal pools.
    pub org_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PoolMemberRequest {
    pub user_service_id: String,
    #[serde(default = "default_member_weight")]
    pub weight: u32,
    #[serde(default = "default_member_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateServicePoolRequest {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub strategy: PoolStrategy,
    #[serde(default)]
    pub members: Vec<PoolMemberRequest>,
    #[serde(default)]
    pub is_active: Option<bool>,
    /// Optional org owner. Omit for a personal pool.
    #[serde(default)]
    pub org_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateServicePoolRequest {
    pub slug: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub strategy: Option<PoolStrategy>,
    #[serde(default)]
    pub members: Option<Vec<PoolMemberRequest>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetPoolMembersRequest {
    #[serde(default)]
    pub members: Vec<PoolMemberRequest>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PoolMemberResponse {
    pub user_service_id: String,
    pub weight: u32,
    pub enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServicePoolResponse {
    pub id: String,
    pub user_id: String,
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub strategy: String,
    pub members: Vec<PoolMemberResponse>,
    pub rr_counter: i64,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServicePoolListResponse {
    pub pools: Vec<ServicePoolResponse>,
}

fn default_member_weight() -> u32 {
    1
}

fn default_member_enabled() -> bool {
    true
}

async fn resolve_requested_owner(
    state: &AppState,
    actor: &str,
    org_id: Option<&str>,
    action: &str,
) -> AppResult<String> {
    if let Some(org_id) = org_id.filter(|id| !id.is_empty()) {
        let access = org_service::resolve_owner_access(&state.db, actor, org_id).await?;
        if !access.can_write() {
            return Err(AppError::OrgRoleInsufficient(format!(
                "admin access to the target org is required to {action} service pools"
            )));
        }
        Ok(org_id.to_string())
    } else {
        Ok(actor.to_string())
    }
}

async fn resolve_pool_write_owner(
    state: &AppState,
    actor: &str,
    pool_id: &str,
) -> AppResult<String> {
    let pool = state
        .db
        .collection::<ServicePool>(SERVICE_POOLS)
        .find_one(doc! { "_id": pool_id })
        .await?
        .ok_or_else(|| AppError::ServicePoolNotFound(pool_id.to_string()))?;

    let access = org_service::resolve_owner_access(&state.db, actor, &pool.user_id).await?;
    if !access.can_read() {
        return Err(AppError::ServicePoolNotFound(pool_id.to_string()));
    }
    if !access.can_write() {
        return Err(AppError::OrgRoleInsufficient(
            "you do not have permission to modify this service pool".to_string(),
        ));
    }
    Ok(pool.user_id)
}

#[utoipa::path(
    get,
    path = "/api/v1/service-pools",
    params(PoolListQuery),
    responses(
        (status = 200, description = "List of service pools", body = ServicePoolListResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn list_pools(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Query(query): Query<PoolListQuery>,
) -> AppResult<Json<ServicePoolListResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_requested_owner(&state, &actor, query.org_id.as_deref(), "list").await?;
    let pools = service_pool_service::list_pools(&state.db, &owner_id).await?;
    Ok(Json(ServicePoolListResponse {
        pools: pools.into_iter().map(pool_response).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/service-pools",
    request_body = CreateServicePoolRequest,
    responses(
        (status = 201, description = "Created service pool", body = ServicePoolResponse),
        (status = 400, description = "Invalid pool", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 409, description = "Slug taken", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn create_pool(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(body): Json<CreateServicePoolRequest>,
) -> AppResult<(StatusCode, Json<ServicePoolResponse>)> {
    let actor = auth_user.user_id.to_string();
    let owner_id =
        resolve_requested_owner(&state, &actor, body.org_id.as_deref(), "create").await?;
    let pool = service_pool_service::create_pool(
        &state.db,
        &owner_id,
        service_pool_service::CreatePoolInput {
            slug: body.slug,
            name: body.name,
            description: body.description,
            strategy: body.strategy,
            members: body.members.into_iter().map(member_from_request).collect(),
            is_active: body.is_active,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(pool_response(pool))))
}

#[utoipa::path(
    get,
    path = "/api/v1/service-pools/{pool_id}",
    params(("pool_id" = String, Path, description = "Service pool ID")),
    responses(
        (status = 200, description = "Service pool", body = ServicePoolResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn get_pool(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(pool_id): Path<String>,
) -> AppResult<Json<ServicePoolResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    let pool = service_pool_service::get_pool(&state.db, &owner_id, &pool_id).await?;
    Ok(Json(pool_response(pool)))
}

#[utoipa::path(
    put,
    path = "/api/v1/service-pools/{pool_id}",
    params(("pool_id" = String, Path, description = "Service pool ID")),
    request_body = UpdateServicePoolRequest,
    responses(
        (status = 200, description = "Updated service pool", body = ServicePoolResponse),
        (status = 400, description = "Invalid pool", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn update_pool(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(pool_id): Path<String>,
    Json(body): Json<UpdateServicePoolRequest>,
) -> AppResult<Json<ServicePoolResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    let pool = service_pool_service::update_pool(
        &state.db,
        &owner_id,
        &pool_id,
        service_pool_service::UpdatePoolInput {
            slug: body.slug,
            name: body.name,
            description: body.description,
            strategy: body.strategy,
            members: body
                .members
                .map(|members| members.into_iter().map(member_from_request).collect()),
            is_active: body.is_active,
        },
    )
    .await?;

    Ok(Json(pool_response(pool)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/service-pools/{pool_id}",
    params(("pool_id" = String, Path, description = "Service pool ID")),
    responses(
        (status = 204, description = "Service pool deleted"),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn delete_pool(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(pool_id): Path<String>,
) -> AppResult<StatusCode> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    service_pool_service::delete_pool(&state.db, &owner_id, &pool_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/v1/service-pools/{pool_id}/members",
    params(("pool_id" = String, Path, description = "Service pool ID")),
    request_body = SetPoolMembersRequest,
    responses(
        (status = 200, description = "Updated service pool members", body = ServicePoolResponse),
        (status = 400, description = "Invalid members", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn set_members(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(pool_id): Path<String>,
    Json(body): Json<SetPoolMembersRequest>,
) -> AppResult<Json<ServicePoolResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    let pool = service_pool_service::set_members(
        &state.db,
        &owner_id,
        &pool_id,
        body.members.into_iter().map(member_from_request).collect(),
    )
    .await?;
    Ok(Json(pool_response(pool)))
}

#[utoipa::path(
    post,
    path = "/api/v1/service-pools/{pool_id}/members",
    params(("pool_id" = String, Path, description = "Service pool ID")),
    request_body = PoolMemberRequest,
    responses(
        (status = 200, description = "Added or updated service pool member", body = ServicePoolResponse),
        (status = 400, description = "Invalid member", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(pool_id): Path<String>,
    Json(body): Json<PoolMemberRequest>,
) -> AppResult<Json<ServicePoolResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    let pool =
        service_pool_service::add_member(&state.db, &owner_id, &pool_id, member_from_request(body))
            .await?;
    Ok(Json(pool_response(pool)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/service-pools/{pool_id}/members/{user_service_id}",
    params(
        ("pool_id" = String, Path, description = "Service pool ID"),
        ("user_service_id" = String, Path, description = "UserService ID")
    ),
    responses(
        (status = 200, description = "Removed service pool member", body = ServicePoolResponse),
        (status = 400, description = "Invalid member", body = crate::errors::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::errors::ErrorResponse),
        (status = 404, description = "Pool not found", body = crate::errors::ErrorResponse)
    ),
    tag = "Service Pools"
)]
pub async fn remove_member(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path((pool_id, user_service_id)): Path<(String, String)>,
) -> AppResult<Json<ServicePoolResponse>> {
    let actor = auth_user.user_id.to_string();
    let owner_id = resolve_pool_write_owner(&state, &actor, &pool_id).await?;
    let pool =
        service_pool_service::remove_member(&state.db, &owner_id, &pool_id, &user_service_id)
            .await?;
    Ok(Json(pool_response(pool)))
}

fn member_from_request(member: PoolMemberRequest) -> ServicePoolMember {
    ServicePoolMember {
        user_service_id: member.user_service_id,
        weight: member.weight,
        enabled: member.enabled,
    }
}

fn pool_response(pool: ServicePool) -> ServicePoolResponse {
    let strategy = PoolStrategy::parse(pool.strategy.as_str()).unwrap_or(pool.strategy);
    ServicePoolResponse {
        id: pool.id,
        user_id: pool.user_id,
        slug: pool.slug,
        name: pool.name,
        description: pool.description,
        strategy: strategy.as_str().to_string(),
        members: pool
            .members
            .into_iter()
            .map(|member| PoolMemberResponse {
                user_service_id: member.user_service_id,
                weight: member.weight,
                enabled: member.enabled,
            })
            .collect(),
        rr_counter: pool.rr_counter,
        is_active: pool.is_active,
        created_at: pool.created_at.to_rfc3339(),
        updated_at: pool.updated_at.to_rfc3339(),
    }
}
