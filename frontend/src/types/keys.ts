export interface KeyInfo {
  readonly id: string;
  readonly label: string;
  readonly slug: string;
  readonly endpoint_url: string;
  readonly endpoint_id: string;
  readonly api_key_id: string;
  readonly credential_type: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly status: string;
  readonly catalog_service_id: string | null;
  readonly catalog_service_name: string | null;
  readonly node_id: string | null;
  readonly node_priority: number;
  readonly is_active: boolean;
  readonly expires_at: string | null;
  readonly last_used_at: string | null;
  readonly error_message: string | null;
  readonly created_at: string;
  readonly service_type: string;
  readonly ssh_host: string | null;
  readonly ssh_port: number | null;
  readonly ssh_ca_public_key: string | null;
  readonly ssh_allowed_principals: readonly string[] | null;
  readonly ssh_certificate_ttl_minutes: number | null;
}

export interface KeyListResponse {
  readonly keys: readonly KeyInfo[];
}

export interface CatalogEntry {
  readonly slug: string;
  readonly name: string;
  readonly description: string | null;
  readonly base_url: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly provider_config_id: string | null;
  readonly provider_type: string | null;
  readonly requires_gateway_url: boolean;
  readonly credential_mode: string | null;
  readonly api_key_instructions: string | null;
  readonly api_key_url: string | null;
  readonly icon_url: string | null;
  readonly documentation_url: string | null;
  readonly service_type: string;
  readonly ssh_host: string | null;
  readonly ssh_port: number | null;
  readonly ssh_ca_public_key: string | null;
  readonly ssh_allowed_principals: readonly string[] | null;
  readonly ssh_certificate_ttl_minutes: number | null;
  readonly authorization_url: string | null;
  readonly token_url: string | null;
  readonly device_code_url: string | null;
  readonly default_scopes: readonly string[] | null;
  readonly supports_pkce: boolean | null;
  readonly oauth_client_id: string | null;
  readonly client_id_param_name: string | null;
}

export interface CatalogListResponse {
  readonly entries: readonly CatalogEntry[];
}

export interface UserEndpointInfo {
  readonly id: string;
  readonly label: string;
  readonly url: string;
  readonly catalog_service_id: string | null;
  readonly created_at: string;
  readonly updated_at: string;
}

export interface UserServiceInfo {
  readonly id: string;
  readonly slug: string;
  readonly endpoint_id: string;
  readonly api_key_id: string;
  readonly auth_method: string;
  readonly auth_key_name: string;
  readonly catalog_service_id: string | null;
  readonly node_id: string | null;
  readonly node_priority: number;
  readonly is_active: boolean;
  readonly created_at: string;
  readonly updated_at: string;
}

export interface AllowedServiceInfo {
  readonly id: string;
  readonly slug: string;
  readonly label: string;
  readonly catalog_service_name: string | null;
}

export interface AllowedNodeInfo {
  readonly id: string;
  readonly name: string;
  readonly status: string;
}

export interface NyxIdApiKeyInfo {
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly key_prefix: string;
  readonly scopes: string;
  readonly last_used_at: string | null;
  readonly expires_at: string | null;
  readonly is_active: boolean;
  readonly created_at: string;
  readonly allowed_service_ids: readonly string[];
  readonly allowed_node_ids: readonly string[];
  readonly allow_all_services: boolean;
  readonly allow_all_nodes: boolean;
  readonly allowed_services: readonly AllowedServiceInfo[];
  readonly allowed_nodes: readonly AllowedNodeInfo[];
}
