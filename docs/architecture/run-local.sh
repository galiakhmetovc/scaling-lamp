#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PORT="${STRUCTURIZR_PORT:-8080}"

docker run --rm \
  --user "$(id -u):$(id -g)" \
  -p "$PORT:8080" \
  -v "$SCRIPT_DIR:/usr/local/structurizr" \
  structurizr/structurizr local
