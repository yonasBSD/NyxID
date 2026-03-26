#!/usr/bin/env bash
# SECURITY MANIFEST:
# Environment variables accessed: none
# External endpoints called: none (nyxid CLI manages connectivity)
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

proxy_args=(proxy request --service "${SERVICE}" --method "${METHOD}" --path "${PATH_PART}")
if [[ -n "${BODY}" ]]; then
  proxy_args+=(--body "${BODY}")
fi

exec nyxid "${proxy_args[@]}"
