#!/usr/bin/env bash
# SECURITY MANIFEST:
# Environment variables accessed: NYXID_BASE_URL, NYXID_API_KEY, NYXID_ACCESS_TOKEN
# External endpoints called: $NYXID_BASE_URL/api/v1/keys (primary), $NYXID_BASE_URL/api/v1/proxy/services (fallback)
# Local files read: none
# Local files written: none
set -euo pipefail

BASE_URL="${NYXID_BASE_URL:-https://nyx-api.chrono-ai.fun}"

# Prefer the nyxid CLI when available
if command -v nyxid >/dev/null 2>&1; then
  exec nyxid service list --output json --base-url "${BASE_URL}"
fi

# Fallback to curl
auth_args=()
if [[ -n "${NYXID_API_KEY:-}" ]]; then
  auth_args=(-H "X-API-Key: ${NYXID_API_KEY}")
elif [[ -n "${NYXID_ACCESS_TOKEN:-}" ]]; then
  auth_args=(-H "Authorization: Bearer ${NYXID_ACCESS_TOKEN}")
else
  echo "Set NYXID_API_KEY or NYXID_ACCESS_TOKEN, or install the nyxid CLI." >&2
  exit 1
fi

# Try /api/v1/keys first (shows all user services including custom slugs)
# Fall back to /api/v1/proxy/services for older backends
result=$(curl -fsS "${auth_args[@]}" "${BASE_URL%/}/api/v1/keys" 2>/dev/null) && echo "$result" && exit 0
curl -fsS "${auth_args[@]}" "${BASE_URL%/}/api/v1/proxy/services"
