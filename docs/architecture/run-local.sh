#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

docker run -it --rm -p 8080:8080 \
  -v "$SCRIPT_DIR:/usr/local/structurizr" \
  structurizr/structurizr local
