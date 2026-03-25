#!/usr/bin/env bash
# SECURITY MANIFEST:
# Environment variables accessed: NYXID_BASE_URL, NYXID_API_KEY, NYXID_ACCESS_TOKEN
# External endpoints called: $NYXID_BASE_URL/api/v1/proxy/s/{service}/{path} (only)
# Local files read: none
# Local files written: none
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <service> <method> <path> [json-body]" >&2
  exit 1
fi

SERVICE="$1"
METHOD="$2"
PATH_PART="$3"
BODY="${4:-}"
BASE_URL="${NYXID_BASE_URL:-https://nyx-api.chrono-ai.fun}"

# Prefer the nyxid CLI when available
if command -v nyxid >/dev/null 2>&1; then
  proxy_args=(proxy request --service "${SERVICE}" --method "${METHOD}" --path "${PATH_PART}" --base-url "${BASE_URL}")
  if [[ -n "${BODY}" ]]; then
    proxy_args+=(--body "${BODY}")
  fi
  exec nyxid "${proxy_args[@]}"
fi

# Fallback to curl
auth_args=()
if [[ -n "${NYXID_API_KEY:-}" ]]; then
  auth_args=(-H "X-API-Key: ${NYXID_API_KEY}")
elif [[ -n "${NYXID_ACCESS_TOKEN:-}" ]]; then
  auth_args=(-H "Authorization: Bearer ${NYXID_ACCESS_TOKEN}")
else
  echo "Set NYXID_API_KEY or NYXID_ACCESS_TOKEN before calling NyxID." >&2
  exit 1
fi

url="${BASE_URL%/}/api/v1/proxy/s/${SERVICE}/${PATH_PART#/}"

if [[ -n "${BODY}" ]]; then
  curl -fsS -X "${METHOD}" "${auth_args[@]}" -H "Content-Type: application/json" "${url}" -d "${BODY}"
else
  curl -fsS -X "${METHOD}" "${auth_args[@]}" "${url}"
fi
