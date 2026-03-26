#!/usr/bin/env bash
# SECURITY MANIFEST:
# Environment variables accessed: none
# External endpoints called: none (nyxid CLI manages connectivity)
# Local files read: none
# Local files written: none
set -euo pipefail

exec nyxid service list --output json
